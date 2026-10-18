//! Optional installed-engine rendering. Does not modify source files or file associations.
use crate::{fsops, model::*};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};
static HELPER: Mutex<()> = Mutex::new(());
static CONVERSION: Mutex<()> = Mutex::new(());
fn command() -> Command {
    let mut c = Command::new("powershell.exe");
    c.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000);
    }
    c
}
fn conversion_command(root: &Path, engine: &str, kind: &str) -> Result<Command> {
    let detected = detect(root)?;
    let bits = detected["engines"].as_array().and_then(|items|items.iter()
        .find(|e|e["engine"]==engine && e["kind"]==kind))
        .and_then(|e|e["bits"].as_u64())
        .ok_or_else(||err("OFFICE_ENGINE","所选 Office 组件未注册可用的本机自动化服务，请修复安装或选择其他引擎"))?;
    if bits == 32 && cfg!(windows) {
        let host = PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(||"C:\\Windows".into()))
            .join("SysWOW64/WindowsPowerShell/v1.0/powershell.exe");
        if !host.is_file() { return Err(err("OFFICE_ENGINE","缺少与 32 位 Office 匹配的自动化宿主")); }
        let mut c=Command::new(host);
        c.args(["-NoProfile","-NonInteractive","-ExecutionPolicy","Bypass"]);
        #[cfg(windows)] {use std::os::windows::process::CommandExt;c.creation_flags(0x08000000);}
        Ok(c)
    } else { Ok(command()) }
}
fn helper(root: &Path) -> Result<PathBuf> {
    let _guard = HELPER
        .lock()
        .map_err(|_| err("OFFICE_ENGINE", "引擎初始化失败"))?;
    let dir = root.join("cache/office-layout");
    fs::create_dir_all(&dir).map_err(|e| err("CACHE_ERROR", e))?;
    let script = dir.join("engine-v1.ps1");
    let content = include_str!("office-engine.ps1");
    if fs::read(&script).ok().as_deref() != Some(content.as_bytes()) {
        fs::write(&script, content).map_err(|e| err("CACHE_ERROR", e))?;
    }
    Ok(script)
}
fn libreoffice() -> Option<PathBuf> {
    ["ProgramFiles", "ProgramFiles(x86)"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|p| PathBuf::from(p).join("LibreOffice/program/soffice.exe"))
        .find(|p| p.is_file())
}
pub fn detect(root: &Path) -> Result<Value> {
    let output = command()
        .arg("-File")
        .arg(helper(root)?)
        .args(["-Mode", "detect"])
        .output()
        .map_err(|e| err("OFFICE_ENGINE", e))?;
    if !output.status.success() {
        return Err(err(
            "OFFICE_ENGINE",
            String::from_utf8_lossy(&output.stderr),
        ));
    }
    let mut value: Value =
        serde_json::from_slice(&output.stdout).map_err(|e| err("OFFICE_ENGINE", e))?;
    if libreoffice().is_some() {
        for kind in ["word", "excel", "powerpoint"] {
            value["engines"]
                .as_array_mut()
                .unwrap()
                .push(json!({"engine":"libreoffice","kind":kind}));
        }
    }
    Ok(value)
}
pub fn kind(extension: &str) -> Option<&'static str> {
    match extension.to_ascii_lowercase().as_str() {
        "doc" | "docx" | "docm" | "rtf" => Some("word"),
        "xls" | "xlsx" | "xlsm" => Some("excel"),
        "ppt" | "pptx" | "pptm" => Some("powerpoint"),
        _ => None,
    }
}
fn artifact(root: &Path, f: &FileRecord, engine: &str, format: &str) -> Result<PathBuf> {
    let _guard = CONVERSION
        .try_lock()
        .map_err(|_| err("OFFICE_BUSY", "已有文档正在生成原排版预览，请稍后重试"))?;
    let kind = kind(&f.extension).ok_or_else(|| err("INVALID_INPUT", "此格式不支持原排版预览"))?;
    if !matches!(engine, "microsoft" | "wps" | "libreoffice") {
        return Err(err("INVALID_INPUT", "请选择可用预览引擎"));
    }
    if f.status != "available" || f.bytes > 64 * 1024 * 1024 {
        return Err(err("OFFICE_LIMIT", "文件不可用或超过 64 MiB"));
    }
    let input = fs::File::open(&f.path).map_err(|e| err("ACCESS_DENIED", e))?;
    let meta = input.metadata().map_err(|e| err("ACCESS_DENIED", e))?;
    if fsops::revision(&meta) != f.revision
        || (!f.identity.is_empty() && fsops::identity(&input) != f.identity)
    {
        return Err(err("FILE_CHANGED", "文件已变化，请刷新预览"));
    }
    drop(input);
    let key = format!(
        "{:x}",
        Sha256::digest(
            format!(
                "v4:{}:{}:{}:{}:{}",
                f.id, f.revision, f.identity, engine, format
            )
            .as_bytes()
        )
    );
    let base = root.join("cache/office-layout");
    let cache = base.join(format!("{key}.{format}"));
    if !cache.is_file() {
        let script = helper(root)?;
        let job = base.join(uuid::Uuid::new_v4().to_string());
        fs::create_dir(&job).map_err(|e| err("CACHE_ERROR", e))?;
        let pdf = job.join(format!("preview.{format}"));
        let log = fs::File::create(job.join("engine.log")).map_err(|e| err("CACHE_ERROR", e))?;
        let mut cmd = if engine == "libreoffice" {
            let mut c = Command::new(
                libreoffice().ok_or_else(|| err("OFFICE_ENGINE", "未检测到 LibreOffice"))?,
            );
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                c.creation_flags(0x08000000);
            }
            let user = job.join("profile/user");
            fs::create_dir_all(&user).map_err(|e| err("CACHE_ERROR", e))?;
            fs::write(user.join("registrymodifications.xcu"),r#"<?xml version="1.0"?><oor:items xmlns:oor="http://openoffice.org/2001/registry"><item oor:path="/org.openoffice.Office.Common/Security/Scripting"><prop oor:name="MacroSecurityLevel" oor:op="fuse"><value>3</value></prop></item></oor:items>"#).map_err(|e|err("CACHE_ERROR",e))?;
            let profile = reqwest::Url::from_directory_path(job.join("profile"))
                .map_err(|_| err("OFFICE_ENGINE", "组件配置目录无效"))?;
            c.arg(format!("-env:UserInstallation={profile}"))
                .args(["--headless", "--convert-to", format, "--outdir"])
                .arg(&job)
                .arg(&f.path);
            c
        } else {
            let mut c = conversion_command(root, engine, kind)?;
            c.arg("-File")
                .arg(script)
                .args([
                    "-Mode",
                    "convert",
                    "-Engine",
                    engine,
                    "-Kind",
                    kind,
                    "-InputFile",
                ])
                .arg(&f.path)
                .arg("-OutputFile")
                .arg(&pdf);
            c
        };
        let mut child = cmd
            .stdout(Stdio::null())
            .stderr(Stdio::from(log))
            .spawn()
            .map_err(|e| err("OFFICE_ENGINE", e))?;
        let start = Instant::now();
        loop {
            if let Some(status) = child.try_wait().map_err(|e| err("OFFICE_ENGINE", e))? {
                if !status.success() {
                    let detail = fs::read_to_string(job.join("engine.log")).unwrap_or_default();
                    return Err(err(
                        "OFFICE_ENGINE",
                        detail.chars().take(1000).collect::<String>(),
                    ));
                }
                break;
            }
            if start.elapsed() > Duration::from_secs(90) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(err(
                    "OFFICE_TIMEOUT",
                    "预览生成超时；请检查办公软件是否有等待处理的对话框",
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let output = if engine == "libreoffice" {
            job.join(Path::new(&f.path).file_stem().unwrap_or_default())
                .with_extension(format)
        } else {
            pdf
        };
        let meta = fs::metadata(&f.path).map_err(|e| err("FILE_CHANGED", e))?;
        if fsops::revision(&meta) != f.revision
            || (!f.identity.is_empty()
                && fsops::identity(&fs::File::open(&f.path).map_err(|e| err("FILE_CHANGED", e))?)
                    != f.identity)
        {
            return Err(err("FILE_CHANGED", "文件已变化，请刷新预览"));
        }
        let size = fs::metadata(&output)
            .map_err(|e| err("OFFICE_ENGINE", e))?
            .len();
        if size > 64 * 1024 * 1024 {
            return Err(err("OFFICE_LIMIT", "生成的 PDF 超过 64 MiB"));
        }
        fs::rename(output, &cache).map_err(|e| err("CACHE_ERROR", e))?;
        let _ = fs::remove_dir_all(&job);
    }
    Ok(cache)
}
pub fn document(root: &Path, f: &FileRecord, engine: &str) -> Result<Value> {
    let cache = artifact(root, f, engine, "pdf")?;
    let data = fs::read(cache).map_err(|e| err("CACHE_ERROR", e))?;
    if data.len() > 64 * 1024 * 1024 || !data.starts_with(b"%PDF-") {
        return Err(err("OFFICE_ENGINE", "预览缓存不是有效 PDF"));
    }
    Ok(json!({"data":B64.encode(data),"engine":engine}))
}

pub fn normalized(root: &Path, f: &FileRecord, engine: &str, sheet: usize) -> Result<Value> {
    let format = match kind(&f.extension) {
        Some("word") => "docx",
        Some("excel") => "xlsx",
        Some("powerpoint") => "pptx",
        _ => return Err(err("INVALID_INPUT", "不支持此格式")),
    };
    let cache = artifact(root, f, engine, format)?;
    let meta = fsops::inspect(&cache)?;
    let mut converted = f.clone();
    converted.path = meta.path;
    converted.extension = format.into();
    converted.kind = "office".into();
    converted.revision = meta.revision;
    converted.identity = meta.identity;
    converted.bytes = meta.bytes;
    crate::office::document(&converted, sheet)
}
