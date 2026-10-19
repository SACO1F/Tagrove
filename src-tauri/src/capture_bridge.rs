//! Chromium browser Native Messaging -> authenticated loopback transport -> one Store owner.
//! No arbitrary commands, paths or database queries are exposed to the extension.
use crate::{
    capture_browser::{
        allowed_origins, authorized_origin, connection_receipt, current_id_config, extension_ids,
        owned_id_config, owned_origins, registration_state, remember_origin, Browser,
        DEVELOPMENT_EXTENSION_ID, EDGE_STORE_EXTENSION_ID,
    },
    db::Store,
    model::*,
    transfer, watch,
};
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
const HOST: &str = "local.shiqian.capture";
const EXTENSION_ID: &str = DEVELOPMENT_EXTENSION_ID;
const MAX: usize = 256 * 1024;
fn read_frame(input: &mut impl Read) -> Result<Value> {
    let mut len = [0u8; 4];
    input
        .read_exact(&mut len)
        .map_err(|_| err("BRIDGE_OFFLINE", "本机连接已断开"))?;
    let n = u32::from_le_bytes(len) as usize;
    if n == 0 || n > MAX {
        return Err(err("BRIDGE_LIMIT", "采集请求过大"));
    }
    let mut body = vec![0; n];
    input
        .read_exact(&mut body)
        .map_err(|_| err("BRIDGE_OFFLINE", "消息不完整"))?;
    serde_json::from_slice(&body).map_err(|_| err("BRIDGE_PROTOCOL", "消息格式不兼容"))
}
fn write_frame(output: &mut impl Write, v: &Value) -> Result<()> {
    let bytes = serde_json::to_vec(v).map_err(|e| err("BRIDGE_PROTOCOL", e))?;
    output
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .and_then(|_| output.write_all(&bytes))
        .and_then(|_| output.flush())
        .map_err(|_| err("BRIDGE_OFFLINE", "连接已断开"))
}
fn config_path() -> PathBuf {
    std::env::var_os("SHIQIAN_BRIDGE_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default())
                .join("ShiqianCapture/host.json")
        })
}
fn enabled(s: &Store) -> bool {
    s.conn
        .query_row(
            "SELECT value='true' FROM settings WHERE key='browserCaptureEnabled'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(false)
}
fn manifest_path() -> Result<PathBuf> {
    Ok(config_path()
        .parent()
        .ok_or_else(|| err("BRIDGE_REGISTER", "连接配置不可用"))?
        .join("native-host.json"))
}
fn read_json(path: &std::path::Path) -> Option<Value> {
    let raw = fs::read(path).ok()?;
    if raw.len() > 32768 {
        return None;
    }
    serde_json::from_slice(&raw).ok()
}
fn owned_config(s: &Store, value: &Value) -> bool {
    value["root"]
        .as_str()
        .is_some_and(|p| PathBuf::from(p) == s.root)
        && owned_id_config(value)
}
fn owned_manifest(value: &Value) -> bool {
    value["name"].as_str() == Some(HOST)
        && value["type"].as_str() == Some("stdio")
        && owned_origins(&value["allowed_origins"])
}
fn record_connection(s: &Store, origin: &str) -> Result<()> {
    let path = config_path();
    let Some(mut config) = read_json(&path).filter(|v| owned_config(s, v)) else {
        return Ok(());
    };
    if !remember_origin(&mut config, origin, now()) {
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| err("BRIDGE_CONFIG", "本机配置无效"))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| err("BRIDGE_CONFIG", e))?;
    temp.write_all(config.to_string().as_bytes())
        .map_err(|e| err("BRIDGE_CONFIG", e))?;
    temp.persist(path).map_err(|e| err("BRIDGE_CONFIG", e))?;
    Ok(())
}
#[cfg(windows)]
mod registry {
    use super::*;
    use windows_sys::Win32::System::Registry::*;
    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }
    fn key(browser: Browser) -> Vec<u16> {
        wide(browser.registry_key())
    }
    pub fn get(browser: Browser) -> Result<Option<PathBuf>> {
        let mut size = 0;
        let code = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key(browser).as_ptr(),
                std::ptr::null(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if code == 2 {
            return Ok(None);
        }
        if code != 0 || size > 65536 {
            return Err(err("BRIDGE_REGISTER", "无法读取浏览器本机连接注册"));
        }
        let mut data = vec![0u16; (size as usize / 2) + 1];
        let code = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key(browser).as_ptr(),
                std::ptr::null(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                data.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if code != 0 {
            return Err(err("BRIDGE_REGISTER", "无法读取浏览器本机连接注册"));
        }
        let end = data.iter().position(|c| *c == 0).unwrap_or(data.len());
        use std::os::windows::ffi::OsStringExt;
        Ok(Some(PathBuf::from(std::ffi::OsString::from_wide(
            &data[..end],
        ))))
    }
    pub fn set(browser: Browser, path: &std::path::Path) -> Result<()> {
        use std::os::windows::ffi::OsStrExt;
        let data: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut handle = std::ptr::null_mut();
        if unsafe { RegCreateKeyW(HKEY_CURRENT_USER, key(browser).as_ptr(), &mut handle) } != 0 {
            return Err(err("BRIDGE_REGISTER", "无法注册浏览器本机连接"));
        }
        let code = unsafe {
            RegSetValueExW(
                handle,
                std::ptr::null(),
                0,
                REG_SZ,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            )
        };
        unsafe {
            RegCloseKey(handle);
        }
        if code != 0 {
            return Err(err(
                "BRIDGE_REGISTER",
                "浏览器本机连接注册失败，请检查当前用户权限",
            ));
        }
        Ok(())
    }
    pub fn remove(browser: Browser, expected: &std::path::Path) -> Result<()> {
        if let Some(current) = get(browser)? {
            if current != expected {
                return Err(err("BRIDGE_REGISTER", "连接由其他位置管理，未移除其注册"));
            }
            // Delete this one key only. Unexpected subkeys cause failure instead of recursive deletion.
            if unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, key(browser).as_ptr()) } != 0 {
                return Err(err("BRIDGE_REGISTER", "无法移除浏览器本机连接注册"));
            }
        }
        Ok(())
    }
}
fn qa_registry_path(browser: Browser) -> Result<PathBuf> {
    Ok(manifest_path()?.with_file_name(format!("qa-registry-{}.json", browser.id())))
}
fn registered_path(browser: Browser) -> Result<Option<PathBuf>> {
    if std::env::var_os("SHIQIAN_QA_BRIDGE_NO_REGISTRY").is_some() {
        return Ok(
            read_json(&qa_registry_path(browser)?).and_then(|v| v.as_str().map(PathBuf::from))
        );
    }
    #[cfg(windows)]
    {
        registry::get(browser)
    }
    #[cfg(not(windows))]
    {
        let _ = browser;
        Ok(None)
    }
}
fn register_path(browser: Browser, manifest: &std::path::Path) -> Result<()> {
    if std::env::var_os("SHIQIAN_QA_BRIDGE_NO_REGISTRY").is_some() {
        return fs::write(qa_registry_path(browser)?, json!(manifest).to_string())
            .map_err(|e| err("BRIDGE_REGISTER", e));
    }
    #[cfg(windows)]
    {
        registry::set(browser, manifest)?;
    }
    Ok(())
}
fn remove_path(browser: Browser, manifest: &std::path::Path) -> Result<()> {
    if std::env::var_os("SHIQIAN_QA_BRIDGE_NO_REGISTRY").is_some() {
        if registered_path(browser)?.as_deref() != Some(manifest) {
            return Err(err("BRIDGE_REGISTER", "连接由其他位置管理，未移除其注册"));
        }
        return fs::remove_file(qa_registry_path(browser)?).map_err(|e| err("BRIDGE_REGISTER", e));
    }
    #[cfg(windows)]
    {
        registry::remove(browser, manifest)?;
    }
    Ok(())
}
pub fn status(s: &Store, browser: Browser) -> Result<Value> {
    let manifest = manifest_path()?;
    let config = read_json(&config_path());
    let host = read_json(&manifest);
    let owns_config = config.as_ref().is_some_and(|v| owned_config(s, v));
    let owns_host = host.as_ref().is_some_and(owned_manifest);
    let current_exe = std::env::current_exe().map_err(|e| err("BRIDGE_REGISTER", e))?;
    let host_current = host
        .as_ref()
        .and_then(|v| v["path"].as_str())
        .is_some_and(|p| PathBuf::from(p) == current_exe)
        && host
            .as_ref()
            .is_some_and(|v| v["allowed_origins"] == allowed_origins())
        && config.as_ref().is_some_and(current_id_config);
    let mut browsers = Vec::new();
    for candidate in Browser::ALL {
        let registration = match registered_path(candidate) {
            Ok(value) => value,
            Err(error) => {
                browsers.push(json!({"id":candidate.id(),"name":candidate.name(),"registrationState":"error","registered":false,"removable":false,"error":error}));
                continue;
            }
        };
        let state = registration_state(
            registration.as_deref(),
            &manifest,
            config.is_some() && host.is_some(),
            owns_config && owns_host && host_current,
        );
        browsers.push(json!({"id":candidate.id(),"name":candidate.name(),"registrationState":state,"registered":state=="ready","removable":owns_config && owns_host && registration.as_deref()==Some(manifest.as_path())}));
    }
    let selected = browsers.iter().find(|v| v["id"] == browser.id()).unwrap();
    let (store_seen, development_seen) = if owns_config {
        config
            .as_ref()
            .map(|c| connection_receipt(c, browser))
            .unwrap_or_default()
    } else {
        (0, 0)
    };
    Ok(
        json!({"enabled":enabled(s),"extensionId":if browser == Browser::Edge {EDGE_STORE_EXTENSION_ID} else {EXTENSION_ID.trim()},"extensionIds":extension_ids(),"protocol":1,"browser":browser.name(),
        "storeLastSeen":store_seen,"developmentLastSeen":development_seen,
        "registrationState":selected["registrationState"],"registered":selected["registered"],"removable":selected["removable"],"registrationError":selected["error"],"browsers":browsers}),
    )
}
pub fn register(s: &Store, browser: Browser) -> Result<Value> {
    let config = config_path();
    let parent = config
        .parent()
        .ok_or_else(|| err("BRIDGE_REGISTER", "连接配置不可用"))?;
    fs::create_dir_all(parent).map_err(|e| err("BRIDGE_REGISTER", e))?;
    let manifest = manifest_path()?;
    let exe = std::env::current_exe().map_err(|e| err("BRIDGE_REGISTER", e))?;
    let receipts = read_json(&config)
        .filter(|v| owned_config(s, v))
        .map(|v| v["lastSeen"].clone())
        .unwrap_or(json!({}));
    // Replace each JSON atomically before switching the registry pointer.
    for (path, value) in [
        (
            &manifest,
            json!({"name":HOST,"description":"Shiqian local capture bridge","path":exe,"type":"stdio","allowed_origins":allowed_origins()}),
        ),
        (
            &config,
            json!({"root":s.root,"extensionId":EXTENSION_ID.trim(),"extensionIds":extension_ids(),"protocol":1,"lastSeen":receipts}),
        ),
    ] {
        let mut temp =
            tempfile::NamedTempFile::new_in(parent).map_err(|e| err("BRIDGE_REGISTER", e))?;
        temp.write_all(value.to_string().as_bytes())
            .map_err(|e| err("BRIDGE_REGISTER", e))?;
        temp.as_file()
            .sync_all()
            .map_err(|e| err("BRIDGE_REGISTER", e))?;
        temp.persist(path).map_err(|e| err("BRIDGE_REGISTER", e))?;
    }
    register_path(browser, &manifest)?;
    s.conn.execute("INSERT INTO settings(key,value) VALUES('browserCaptureEnabled','true') ON CONFLICT(key) DO UPDATE SET value='true'",[]).map_err(sql_err)?;
    status(s, browser)
}
pub fn unregister(s: &Store, browser: Browser) -> Result<Value> {
    let config = config_path();
    let manifest = manifest_path()?;
    if !read_json(&config)
        .as_ref()
        .is_some_and(|v| owned_config(s, v))
        || !read_json(&manifest).as_ref().is_some_and(owned_manifest)
    {
        return Err(err(
            "BRIDGE_REGISTER",
            "连接配置不属于当前资料库，请先修复连接",
        ));
    }
    if registered_path(browser)?.is_none() {
        return status(s, browser);
    }
    // Remove only the selected browser and never a foreign key.
    remove_path(browser, &manifest)?;
    let mut shared = false;
    for candidate in Browser::ALL {
        // Unknown ownership must preserve shared files.
        shared |= registered_path(candidate)
            .map(|value| value.as_deref() == Some(manifest.as_path()))
            .unwrap_or(true);
    }
    if !shared {
        fs::remove_file(&manifest).map_err(|e| err("BRIDGE_REGISTER", e))?;
        fs::remove_file(&config).map_err(|e| err("BRIDGE_REGISTER", e))?;
        s.conn
            .execute("DELETE FROM settings WHERE key='browserCaptureEnabled'", [])
            .map_err(sql_err)?;
    }
    status(s, browser)
}
pub fn start(
    store: Arc<Mutex<Store>>,
    watch: Arc<watch::Control>,
    transfer: Arc<transfer::Control>,
    app: tauri::AppHandle,
) {
    let Ok(listener) = TcpListener::bind(("127.0.0.1", 0)) else {
        return;
    };
    let Ok(address) = listener.local_addr() else {
        return;
    };
    let token = format!("{}{}", id(), id());
    let root = match store.lock() {
        Ok(s) => s.root.clone(),
        Err(_) => return,
    };
    let session = root.join("bridge-session.json");
    if fs::write(
        &session,
        json!({"port":address.port(),"token":token,"protocol":1}).to_string(),
    )
    .is_err()
    {
        return;
    }
    let _ = listener.set_nonblocking(true);
    let local_tasks = Arc::new(Mutex::new(crate::image_capture::LocalTasks::default()));
    std::thread::spawn(move || {
        while !watch.stopped.load(std::sync::atomic::Ordering::SeqCst) {
            let (mut socket, peer) = match listener.accept() {
                Ok(pair) => pair,
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(80));
                    continue;
                }
            };
            if !peer.ip().is_loopback() {
                continue;
            }
            let _ = socket.set_read_timeout(Some(Duration::from_secs(2)));
            let _ = socket.set_write_timeout(Some(Duration::from_secs(2)));
            let mut verified_origin = None;
            let result = (|| -> Result<Value> {
                let envelope = read_frame(&mut socket)?;
                if envelope["token"].as_str() != Some(&token) {
                    return Err(err("BRIDGE_AUTH", "连接身份无效"));
                }
                verified_origin = envelope["origin"].as_str().map(str::to_owned);
                let m = &envelope["message"];
                if m["protocol"].as_u64() != Some(1) {
                    return Err(err("BRIDGE_PROTOCOL", "请更新拾签和扩展"));
                }
                if m["action"].as_str() == Some("capture.image.local.status") {
                    let request = crate::image_capture::Request::parse(&m["capture"])?;
                    {
                        let s = store.lock().map_err(|_| err("BRIDGE_BUSY", "资料库忙"))?;
                        if !enabled(&s) {
                            return Err(err("BRIDGE_DISABLED", "浏览器采集已断开"));
                        }
                        if let Some(value) = s.image_receipt(&request)? {
                            return Ok(json!({"status":"done","result":value}));
                        }
                    }
                    return local_tasks
                        .lock()
                        .map_err(|_| err("BRIDGE_BUSY", "原图任务忙"))?
                        .status(&request.request_id);
                }
                if m["action"].as_str() == Some("capture.image.local.start") {
                    let request = crate::image_capture::Request::parse(&m["capture"])?;
                    {
                        let s = store.lock().map_err(|_| err("BRIDGE_BUSY", "资料库忙"))?;
                        if !enabled(&s) {
                            return Err(err("BRIDGE_DISABLED", "请先连接浏览器采集"));
                        }
                        if transfer.busy.load(std::sync::atomic::Ordering::SeqCst) {
                            return Err(err("BRIDGE_BUSY", "资料交接中，请稍后重试"));
                        }
                        s.capture_tag_ids(&json!({"tagIds":request.tag_ids}))?;
                        if let Some(value) = s.image_receipt(&request)? {
                            return Ok(json!({"status":"done","result":value}));
                        }
                    }
                    let (start, value) = local_tasks
                        .lock()
                        .map_err(|_| err("BRIDGE_BUSY", "原图任务忙"))?
                        .start(&request)?;
                    if !start {
                        return Ok(value);
                    }
                    let tasks = local_tasks.clone();
                    let owner = store.clone();
                    let control = transfer.clone();
                    let handle = app.clone();
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                    app.dialog()
                        .file()
                        .set_title("拾签 · 选择已从浏览器下载的原图")
                        .add_filter("图片", &["png", "jpg", "jpeg", "webp", "gif", "bmp"])
                        .pick_file(move |file| {
                            let result = (|| -> Result<Option<Value>> {
                                let Some(file) = file else {
                                    return Ok(None);
                                };
                                let path = file
                                    .into_path()
                                    .map_err(|_| err("IMAGE_LOCAL_READ", "所选原图不是本机文件"))?;
                                let prepared =
                                    crate::image_capture::prepare_local(&path, &request)?;
                                let mut s =
                                    owner.lock().map_err(|_| err("BRIDGE_BUSY", "资料库忙"))?;
                                if !enabled(&s) {
                                    return Err(err("BRIDGE_DISABLED", "采集已断开，原图未入库"));
                                }
                                if control.busy.load(std::sync::atomic::Ordering::SeqCst) {
                                    return Err(err("BRIDGE_BUSY", "资料交接中，原图未入库"));
                                }
                                let saved = s.commit_image(&request, prepared)?;
                                let _ = handle.emit("library-changed", ());
                                Ok(Some(saved))
                            })();
                            let state = match result {
                                Ok(Some(saved)) => json!({"status":"done","result":saved}),
                                Ok(None) => json!({"status":"cancelled"}),
                                Err(error) => json!({"status":"failed","error":error}),
                            };
                            if let Ok(mut tasks) = tasks.lock() {
                                tasks.finish(&request.request_id, state);
                            }
                        });
                    return Ok(value);
                }
                if m["action"].as_str() == Some("capture.image") {
                    let request = crate::image_capture::Request::parse(&m["capture"])?;
                    {
                        let s = store
                            .lock()
                            .map_err(|_| err("BRIDGE_BUSY", "资料库忙，请重试"))?;
                        if !enabled(&s) {
                            return Err(err("BRIDGE_DISABLED", "请在拾签偏好设置连接浏览器扩展"));
                        }
                        if transfer.busy.load(std::sync::atomic::Ordering::SeqCst) {
                            return Err(err("BRIDGE_BUSY", "资料交接中，请稍后重试"));
                        }
                        s.capture_tag_ids(&json!({"tagIds":request.tag_ids}))?;
                        if let Some(value) = s.image_receipt(&request)? {
                            crate::capture_options::apply(
                                &s.conn,
                                value["id"].as_str().unwrap(),
                                &request.tag_ids,
                                false,
                            )?;
                            let _ = app.emit("library-changed", ());
                            return Ok(value);
                        }
                    }
                    // Network/decode work never holds the Store mutex, keeping desktop operations available.
                    let prepared = crate::image_capture::download(&request)?;
                    let mut s = store
                        .lock()
                        .map_err(|_| err("BRIDGE_BUSY", "资料库忙，请重试"))?;
                    if !enabled(&s) {
                        return Err(err("BRIDGE_DISABLED", "采集已断开，图片未入库"));
                    }
                    if transfer.busy.load(std::sync::atomic::Ordering::SeqCst) {
                        return Err(err("BRIDGE_BUSY", "资料交接中，请稍后重试"));
                    }
                    let value = s.commit_image(&request, prepared)?;
                    let _ = app.emit("library-changed", ());
                    return Ok(value);
                }
                let mut s = store
                    .lock()
                    .map_err(|_| err("BRIDGE_BUSY", "资料库忙，请重试"))?;
                if !enabled(&s) {
                    return Err(err("BRIDGE_DISABLED", "请在拾签偏好设置连接浏览器扩展"));
                }
                match m["action"].as_str() {
                    Some("ping") => Ok(
                        json!({"protocol":1,"version":env!("CARGO_PKG_VERSION"),"connected":true,"capabilities":["imageCapture","captureTags","webAi","localImageRecovery","imageSourceRetry"]}),
                    ),
                    Some("tags") => {
                        s.browser_tag_catalog(m["capture"]["query"].as_str().unwrap_or(""))
                    }
                    Some("capture") => {
                        if transfer.busy.load(std::sync::atomic::Ordering::SeqCst) {
                            return Err(err("BRIDGE_BUSY", "资料交接中，请稍后重试"));
                        }
                        let value = s.capture(&m["capture"])?;
                        let _ = app.emit("library-changed", ());
                        Ok(value)
                    }
                    _ => Err(err("BRIDGE_PROTOCOL", "不支持的采集操作")),
                }
            })();
            if result.is_ok() {
                if let (Some(origin), Ok(s)) = (verified_origin.as_deref(), store.lock()) {
                    let _ = record_connection(&s, origin);
                }
            }
            let response = match result {
                Ok(v) => json!({"ok":true,"result":v}),
                Err(e) => json!({"ok":false,"error":e}),
            };
            let _ = write_frame(&mut socket, &response);
        }
        let _ = fs::remove_file(session);
    });
}
fn forward(message: &Value, origin: &str) -> Result<Value> {
    let raw = fs::read(config_path()).map_err(|_| {
        err(
            "BRIDGE_NOT_REGISTERED",
            "请在拾签偏好设置连接对应浏览器扩展",
        )
    })?;
    if raw.len() > 32768 {
        return Err(err("BRIDGE_CONFIG", "本机配置无效"));
    }
    let c: Value =
        serde_json::from_slice(&raw).map_err(|_| err("BRIDGE_CONFIG", "本机配置无效"))?;
    if !authorized_origin(&c, origin) {
        return Err(err("BRIDGE_AUTH", "未配对的扩展"));
    }
    let root = PathBuf::from(
        c["root"]
            .as_str()
            .ok_or_else(|| err("BRIDGE_CONFIG", "未找到拾签资料库"))?,
    );
    let data = fs::read(root.join("bridge-session.json"))
        .map_err(|_| err("BRIDGE_OFFLINE", "拾签未运行，资料已保留在浏览器队列"))?;
    let session: Value =
        serde_json::from_slice(&data).map_err(|_| err("BRIDGE_OFFLINE", "请重新启动拾签"))?;
    let port = session["port"]
        .as_u64()
        .filter(|p| *p > 1024 && *p <= 65535)
        .ok_or_else(|| err("BRIDGE_CONFIG", "本机端口无效"))?;
    let mut socket = TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port as u16)),
        Duration::from_secs(2),
    )
    .map_err(|_| err("BRIDGE_OFFLINE", "拾签未运行，资料仍在队列中"))?;
    socket
        .set_read_timeout(Some(Duration::from_secs(
            if message["action"].as_str() == Some("capture.image") {
                40
            } else {
                10
            },
        )))
        .map_err(|_| err("BRIDGE_OFFLINE", "连接失败"))?;
    let _ = socket.set_write_timeout(Some(Duration::from_secs(3)));
    write_frame(
        &mut socket,
        &json!({"token":session["token"],"message":message,"origin":origin}),
    )?;
    read_frame(&mut socket)
}
pub fn native_main() {
    let origin = std::env::args().nth(1).unwrap_or_default();
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    while let Ok(message) = read_frame(&mut input) {
        let response = forward(&message, &origin).unwrap_or_else(|e| json!({"ok":false,"error":e}));
        if write_frame(&mut output, &response).is_err() {
            break;
        }
    }
}
