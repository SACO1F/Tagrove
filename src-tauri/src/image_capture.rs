//! Explicit web-image download and user-picked original. No browser credentials or arbitrary local targets.
use crate::{
    capture::{checked_url, ImageSource, Source},
    db::Store,
    fsops,
    model::*,
};
use image::{ImageFormat, ImageReader};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Read, Write},
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
pub const MAX_BYTES: u64 = 50 * 1024 * 1024;
pub const MAX_PIXELS: u64 = 100_000_000;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub request_id: String,
    pub kind: String,
    pub title: String,
    pub url: String,
    pub image_url: String,
    #[serde(default)]
    pub excerpt: String,
    #[serde(default)]
    pub tag_ids: Vec<String>,
}
impl Request {
    pub fn parse(value: &Value) -> Result<Self> {
        let mut r: Self = serde_json::from_value(value.clone())
            .map_err(|_| err("IMAGE_INVALID", "图片采集请求无效"))?;
        r.title = r.title.trim().into();
        r.url = checked_url(&r.url)?;
        r.image_url = checked_url(&r.image_url)?;
        if uuid::Uuid::parse_str(&r.request_id).is_err()
            || r.kind != "web_image"
            || !r.excerpt.is_empty()
            || r.title.is_empty()
            || r.title.chars().count() > 300
            || r.title.chars().any(char::is_control)
        {
            return Err(err("IMAGE_INVALID", "标题或图片采集类型无效"));
        }
        if r.tag_ids.len() > 50
            || r.tag_ids
                .iter()
                .any(|id| uuid::Uuid::parse_str(id).is_err())
        {
            return Err(err("IMAGE_INVALID", "标签选择无效"));
        }
        r.tag_ids.sort();
        r.tag_ids.dedup();
        Ok(r)
    }
    fn payload(&self) -> String {
        let original = hash(
            &serde_json::to_vec(&json!([self.kind, self.title, self.url, self.image_url])).unwrap(),
        );
        if self.tag_ids.is_empty() {
            original
        } else {
            hash(&serde_json::to_vec(&json!([original, self.tag_ids])).unwrap())
        }
    }
}
fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
pub fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let [a, b, c, _] = v.octets();
            !v.is_private()
                && !v.is_loopback()
                && !v.is_link_local()
                && !v.is_unspecified()
                && !v.is_multicast()
                && a != 0
                && a < 224
                && !(a == 100 && (64..=127).contains(&b))
                && !(a == 192 && b == 0)
                && !(a == 198 && (b == 18 || b == 19 || b == 51 && c == 100))
                && !(a == 203 && b == 0 && c == 113)
        }
        IpAddr::V6(v) => {
            if let Some(v4) = v.to_ipv4_mapped() {
                return public_ip(IpAddr::V4(v4));
            }
            let s = v.segments();
            s[0] & 0xe000 == 0x2000
                && s[0] != 0x2002
                && s[0] != 0x3fff
                && !(s[0] == 0x2001
                    && (s[1] == 0
                        || s[1] == 2
                        || s[1] == 0xdb8
                        || s[1] & 0xfff0 == 0x10
                        || s[1] & 0xfff0 == 0x20))
        }
    }
}
static DNS_ACTIVE: AtomicUsize = AtomicUsize::new(0);
fn addresses(url: &reqwest::Url, deadline: Instant) -> Result<Vec<SocketAddr>> {
    let host = url
        .host_str()
        .ok_or_else(|| err("IMAGE_URL", "图片地址无效"))?
        .trim_matches(['[', ']'])
        .to_owned();
    let port = url
        .port_or_known_default()
        .ok_or_else(|| err("IMAGE_URL", "图片端口无效"))?;
    let ips = if let Ok(ip) = host.parse::<IpAddr>() {
        vec![SocketAddr::new(ip, port)]
    } else {
        if DNS_ACTIVE
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                (n < 2).then_some(n + 1)
            })
            .is_err()
        {
            return Err(err("IMAGE_DOWNLOAD", "域名解析忙，请稍后重试"));
        }
        let (tx, rx) = std::sync::mpsc::channel();
        if std::thread::Builder::new()
            .name("image-dns".into())
            .spawn(move || {
                let result = (host.as_str(), port)
                    .to_socket_addrs()
                    .map(|i| i.take(16).collect::<Vec<_>>());
                DNS_ACTIVE.fetch_sub(1, Ordering::SeqCst);
                let _ = tx.send(result);
            })
            .is_err()
        {
            DNS_ACTIVE.fetch_sub(1, Ordering::SeqCst);
            return Err(err("IMAGE_DOWNLOAD", "无法解析图片域名"));
        }
        rx.recv_timeout(
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_secs(3)),
        )
        .map_err(|_| err("IMAGE_DOWNLOAD", "图片域名解析超时"))?
        .map_err(|_| err("IMAGE_DOWNLOAD", "图片域名解析失败"))?
    };
    if ips.is_empty() || ips.iter().any(|a| !public_ip(a.ip())) {
        return Err(err(
            "IMAGE_URL",
            "首版仅下载公网 HTTP／HTTPS 图片，不访问本机或内网地址",
        ));
    }
    Ok(ips)
}
pub struct Prepared {
    pub bytes: Vec<u8>,
    pub image: ImageSource,
}
pub fn inspect_bytes(
    bytes: Vec<u8>,
    original: &str,
    final_url: &str,
    content_type: Option<&str>,
) -> Result<Prepared> {
    if bytes.is_empty() || bytes.len() as u64 > MAX_BYTES {
        return Err(err("IMAGE_LIMIT", "图片为空或超过 50 MiB"));
    }
    let format = image::guess_format(&bytes).map_err(|_| {
        err(
            "IMAGE_TYPE",
            "图片内容不是受支持的 PNG、JPEG、WebP、GIF、BMP",
        )
    })?;
    let mime = match format {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::WebP => "image/webp",
        ImageFormat::Gif => "image/gif",
        ImageFormat::Bmp => "image/bmp",
        _ => {
            return Err(err(
                "IMAGE_TYPE",
                "仅支持 PNG、JPEG、WebP、GIF、BMP；不支持 SVG 或网页内容",
            ))
        }
    };
    if let Some(header) = content_type {
        let header = header
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        let header = match header.as_str() {
            "image/jpg" | "image/pjpeg" => "image/jpeg",
            "image/x-png" => "image/png",
            "image/x-ms-bmp" | "image/x-bmp" => "image/bmp",
            other => other,
        };
        if !header.is_empty() && header != "application/octet-stream" && header != mime {
            return Err(err("IMAGE_TYPE", "服务器图片类型与实际内容不一致"));
        }
    }
    let mut reader = ImageReader::with_format(Cursor::new(&bytes), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(512 * 1024 * 1024);
    limits.max_image_width = Some(40000);
    limits.max_image_height = Some(40000);
    reader.limits(limits);
    let (width, height) = reader
        .into_dimensions()
        .map_err(|_| err("IMAGE_INVALID", "无法读取图片尺寸"))?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(err("IMAGE_LIMIT", "图片超过 1 亿像素"));
    }
    let mut reader = ImageReader::with_format(Cursor::new(&bytes), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|_| err("IMAGE_INVALID", "图片损坏或解码资源超限"))?;
    Ok(Prepared {
        image: ImageSource {
            url: checked_url(original)?,
            final_url: checked_url(final_url)?,
            sha256: hash(&bytes),
            mime: mime.into(),
            width,
            height,
        },
        bytes,
    })
}
fn response_bytes(
    mut response: reqwest::blocking::Response,
    original: &str,
    final_url: &str,
) -> Result<Prepared> {
    if response.content_length().is_some_and(|n| n > MAX_BYTES) {
        return Err(err("IMAGE_LIMIT", "图片超过 50 MiB"));
    }
    let mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|s| s.to_str().ok())
        .map(str::to_owned);
    if mime.as_deref().is_some_and(|m| {
        !matches!(
            m.split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase()
                .as_str(),
            "image/png"
                | "image/jpeg"
                | "image/webp"
                | "image/gif"
                | "image/bmp"
                | "image/x-ms-bmp"
                | "image/x-bmp"
                | "image/jpg"
                | "image/pjpeg"
                | "image/x-png"
                | "application/octet-stream"
        )
    }) {
        return Err(err("IMAGE_TYPE", "服务器返回的不是受支持的图片"));
    }
    let mut bytes = Vec::new();
    (&mut response)
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| err("IMAGE_DOWNLOAD", "图片下载中断或超时，请重试"))?;
    inspect_bytes(bytes, original, final_url, mime.as_deref())
}
// Retry with origin only: never expose a page path, query, fragment or credentials.
fn source_referer(source: &str, target: &reqwest::Url) -> Option<String> {
    let source = reqwest::Url::parse(&checked_url(source).ok()?).ok()?;
    if source.scheme() == "https" && target.scheme() == "http" {
        return None;
    }
    if source.host_str().is_some_and(|host| {
        host.trim_matches(['[', ']'])
            .parse::<IpAddr>()
            .is_ok_and(|ip| !public_ip(ip))
    }) {
        return None;
    }
    Some(format!("{}/", source.origin().ascii_serialization()))
}
fn compatible_response(
    client: &reqwest::blocking::Client,
    url: &reqwest::Url,
    source: &str,
    deadline: Instant,
) -> Result<reqwest::blocking::Response> {
    let send = |referer: Option<String>| -> Result<reqwest::blocking::Response> {
        if Instant::now() >= deadline {
            return Err(err("IMAGE_DOWNLOAD", "图片下载总时限已到，请稍后重试"));
        }
        let mut request = client
            .get(url.clone())
            .timeout(deadline.saturating_duration_since(Instant::now()));
        if let Some(value) = referer {
            request = request.header(reqwest::header::REFERER, value);
        }
        request
            .send()
            .map_err(|_| err("IMAGE_DOWNLOAD", "图片下载失败或超时；不携带浏览器登录信息"))
    };
    let first = send(None)?;
    if first.status() != reqwest::StatusCode::FORBIDDEN {
        return Ok(first);
    }
    if let Some(referer) = source_referer(source, url) {
        drop(first);
        send(Some(referer))
    } else {
        Ok(first)
    }
}
pub fn download(request: &Request) -> Result<Prepared> {
    let mut url =
        reqwest::Url::parse(&request.image_url).map_err(|_| err("IMAGE_URL", "图片地址无效"))?;
    let deadline = Instant::now() + Duration::from_secs(20);
    for hop in 0..=5 {
        checked_url(url.as_str())?;
        let ips = addresses(&url, deadline)?;
        let host = url.host_str().unwrap().trim_matches(['[', ']']);
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(deadline.saturating_duration_since(Instant::now()))
            .connect_timeout(Duration::from_secs(5))
            .resolve_to_addrs(host, &ips)
            .user_agent(concat!("Shiqian/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| err("IMAGE_DOWNLOAD", "无法初始化图片下载"))?;
        let response = compatible_response(&client, &url, &request.url, deadline)?;
        if response.status().is_redirection() {
            if hop == 5 {
                return Err(err("IMAGE_DOWNLOAD", "图片重定向次数过多"));
            }
            let target = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|s| s.to_str().ok())
                .ok_or_else(|| err("IMAGE_DOWNLOAD", "图片重定向缺少有效地址"))?;
            url = url
                .join(target)
                .map_err(|_| err("IMAGE_URL", "图片重定向地址无效"))?;
            continue;
        }
        if !response.status().is_success() {
            return Err(err(
                if response.status().is_client_error()
                    && !matches!(response.status().as_u16(), 408 | 429)
                {
                    "IMAGE_ACCESS"
                } else {
                    "IMAGE_DOWNLOAD"
                },
                format!(
                    "图片服务返回 HTTP {}{}；可从浏览器下载原图后，在采集队列选择已下载原图",
                    response.status().as_u16(),
                    if response.status().as_u16() == 403 {
                        "，来源站点兼容重试后仍被拒绝或无法安全发送来源信息"
                    } else {
                        ""
                    }
                ),
            ));
        }
        return response_bytes(response, &request.image_url, url.as_str());
    }
    Err(err("IMAGE_DOWNLOAD", "图片未下载完成"))
}
// Called only with a path chosen by the native file dialog, never a path from the extension.
pub fn prepare_local(path: &std::path::Path, request: &Request) -> Result<Prepared> {
    let meta = fs::symlink_metadata(path).map_err(|_| {
        err(
            "IMAGE_LOCAL_READ",
            "无法读取所选原图，请检查文件是否存在及读取权限",
        )
    })?;
    if !meta.is_file() || fsops::is_link(&meta) {
        return Err(err(
            "IMAGE_LOCAL_READ",
            "请选择普通图片文件，不支持文件夹或链接",
        ));
    }
    if meta.len() > MAX_BYTES {
        return Err(err("IMAGE_LIMIT", "所选原图超过 50 MiB"));
    }
    let file = fs::File::open(path)
        .map_err(|_| err("IMAGE_LOCAL_READ", "无法打开所选原图，请检查读取权限"))?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| err("IMAGE_LOCAL_READ", "读取所选原图失败"))?;
    inspect_bytes(bytes, &request.image_url, &request.image_url, None)
}
#[derive(Default)]
pub struct LocalTasks {
    entries: std::collections::HashMap<String, (String, Value)>,
}
impl LocalTasks {
    pub fn start(&mut self, request: &Request) -> Result<(bool, Value)> {
        if let Some((payload, value)) = self.entries.get(&request.request_id) {
            if payload != &request.payload() {
                return Err(err(
                    "CAPTURE_CONFLICT",
                    "本地原图任务内容已变化，请重新编辑",
                ));
            }
            if value["status"] == "pending" {
                return Ok((false, value.clone()));
            }
        }
        if self.entries.values().any(|(_, v)| v["status"] == "pending") {
            return Err(err("BRIDGE_BUSY", "已有原图选择窗口，请先完成或取消"));
        }
        if self.entries.len() >= 20 {
            self.entries.clear();
        }
        let state = json!({"status":"pending"});
        self.entries.insert(
            request.request_id.clone(),
            (request.payload(), state.clone()),
        );
        Ok((true, state))
    }
    pub fn finish(&mut self, id: &str, value: Value) {
        if let Some(entry) = self.entries.get_mut(id) {
            entry.1 = value;
        }
    }
    pub fn status(&self, id: &str) -> Result<Value> {
        self.entries.get(id).map(|(_, v)| v.clone()).ok_or_else(|| {
            err(
                "IMAGE_LOCAL_LOST",
                "原图选择任务已结束或桌面已重启；请重新点击选择已下载原图",
            )
        })
    }
}
pub fn extension(source: &ImageSource) -> &'static str {
    match source.mime.as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        _ => "webp",
    }
}
pub fn portable_name(source: &Source) -> String {
    let base = crate::capture::portable_name(&source.title);
    format!(
        "{}.{}",
        base.trim_end_matches(".sqweb"),
        extension(source.image.as_ref().expect("validated image"))
    )
}
impl Store {
    pub fn image_receipt(&self, r: &Request) -> Result<Option<Value>> {
        let receipt:Option<(String,String,Option<i64>)>=self.conn.query_row("SELECT c.payload_hash,c.file_id,f.removed_at FROM capture_receipts c JOIN files f ON f.id=c.file_id WHERE c.request_id=?",[&r.request_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional().map_err(sql_err)?;
        if let Some((payload, fid, removed)) = receipt {
            if payload != r.payload() {
                return Err(err(
                    "CAPTURE_CONFLICT",
                    "请求标识对应内容已变化，请重新采集",
                ));
            }
            if removed.is_some() {
                return Err(err("CAPTURE_REMOVED", "图片资料已移除，请重新采集"));
            }
            return Ok(Some(json!({"id":fid,"duplicate":true})));
        }
        Ok(None)
    }
    pub fn commit_image(&mut self, r: &Request, prepared: Prepared) -> Result<Value> {
        let tags = self.capture_tag_ids(&json!({"tagIds":r.tag_ids}))?;
        if let Some(receipt) = self.image_receipt(r)? {
            crate::capture_options::apply(
                &self.conn,
                receipt["id"].as_str().unwrap(),
                &tags,
                false,
            )?;
            return Ok(receipt);
        }
        let auto_ai = tags.is_empty() && self.ai_config()?.enabled;
        let source = Source {
            format: 2,
            kind: "web_image".into(),
            title: r.title.clone(),
            url: r.url.clone(),
            excerpt: String::new(),
            captured_at: now(),
            image: Some(prepared.image),
            reading: None,
        };
        source.validate()?;
        let dedupe = crate::capture::source_dedupe(&source);
        let existing:Option<String>=self.conn.query_row("SELECT c.file_id FROM capture_receipts c JOIN files f ON f.id=c.file_id WHERE c.dedupe_key=? AND f.removed_at IS NULL LIMIT 1",[&dedupe],|row|row.get(0)).optional().map_err(sql_err)?;
        if let Some(fid) = existing {
            self.conn
                .execute(
                    "INSERT INTO capture_receipts VALUES(?,?,?,?)",
                    params![r.request_id, r.payload(), dedupe, fid],
                )
                .map_err(sql_err)?;
            crate::capture_options::apply(&self.conn, &fid, &tags, false)?;
            return Ok(json!({"id":fid,"duplicate":true,"aiQueued":false}));
        }
        let folder = self.root.join("captured-images");
        fs::create_dir_all(&folder).map_err(|e| err("CAPTURE_STORAGE", e))?;
        if fsops::is_link(&fs::symlink_metadata(&folder).map_err(|e| err("CAPTURE_STORAGE", e))?) {
            return Err(err("CAPTURE_STORAGE", "图片采集目录不能是链接"));
        }
        let fid = id();
        let file = folder.join(format!(
            "{fid}.{}",
            extension(source.image.as_ref().unwrap())
        ));
        let mut temp =
            tempfile::NamedTempFile::new_in(&folder).map_err(|e| err("CAPTURE_STORAGE", e))?;
        temp.write_all(&prepared.bytes)
            .map_err(|e| err("CAPTURE_STORAGE", e))?;
        temp.as_file()
            .sync_all()
            .map_err(|e| err("CAPTURE_STORAGE", e))?;
        temp.persist_noclobber(&file)
            .map_err(|e| err("CAPTURE_STORAGE", e))?;
        let result = (|| {
            let meta = fsops::inspect(&file)?;
            let tx = self.conn.transaction().map_err(sql_err)?;
            tx.execute("INSERT INTO files(id,path,path_key,parent,name,name_key,extension,kind,bytes,modified,added,revision,identity,web_source,web_key) VALUES(?,?,?,?,?,?,?,'image',?,?,?,?,?,?,?)",params![fid,meta.path,meta.path,meta.parent,portable_name(&source),key(&portable_name(&source)),meta.extension,meta.bytes,meta.modified,source.captured_at,meta.revision,meta.identity,serde_json::to_string(&source).map_err(|e|err("IMAGE_INVALID",e))?,key(&source.text())]).map_err(sql_err)?;
            tx.execute(
                "INSERT INTO capture_receipts VALUES(?,?,?,?)",
                params![r.request_id, r.payload(), dedupe, fid],
            )
            .map_err(sql_err)?;
            crate::capture_options::apply(&tx, &fid, &tags, auto_ai)?;
            tx.commit().map_err(sql_err)?;
            Ok(json!({"id":fid,"duplicate":false,"aiQueued":auto_ai}))
        })();
        if result.is_err() {
            let _ = fs::remove_file(file);
        }
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn bytes(format: ImageFormat) -> Vec<u8> {
        let mut c = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(8, 6)
            .write_to(&mut c, format)
            .unwrap();
        c.into_inner()
    }
    fn req(title: &str) -> Request {
        Request::parse(&json!({"requestId":id(),"kind":"web_image","title":title,"url":"https://example.test/page","imageUrl":"https://images.example.test/a.png"})).unwrap()
    }
    fn prepared() -> Prepared {
        inspect_bytes(
            bytes(ImageFormat::Png),
            "https://images.example.test/a.png",
            "https://cdn.example.test/a.png",
            Some("image/png"),
        )
        .unwrap()
    }
    #[test]
    fn forbidden_response_retries_once_with_origin_only() {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let png = bytes(ImageFormat::Png);
        let server = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for attempt in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut raw = Vec::new();
                let mut byte = [0];
                while !raw.ends_with(b"\r\n\r\n") && raw.len() < 8192 {
                    stream.read_exact(&mut byte).unwrap();
                    raw.push(byte[0]);
                }
                seen.push(String::from_utf8(raw).unwrap().to_ascii_lowercase());
                if attempt == 0 {
                    stream.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                } else {
                    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",png.len()).as_bytes()).unwrap();
                    stream.write_all(&png).unwrap();
                }
            }
            seen
        });
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let url = reqwest::Url::parse(&format!("http://{address}/image")).unwrap();
        let response = compatible_response(
            &client,
            &url,
            "http://example.test/private?token=hidden#fragment",
            Instant::now() + Duration::from_secs(3),
        )
        .unwrap();
        assert!(response.status().is_success());
        assert!(
            response_bytes(response, "https://example.test/a", "https://example.test/a").is_ok()
        );
        let seen = server.join().unwrap();
        assert!(!seen[0].contains("referer:"));
        assert!(seen[1].contains("referer: http://example.test/\r\n"));
        for raw in seen {
            for secret in ["private", "hidden", "cookie:", "authorization:"] {
                assert!(!raw.contains(secret));
            }
        }
        assert!(source_referer("https://example.test/private", &url).is_none());
        assert!(source_referer("https://user:pass@example.test/", &url).is_none());
        assert!(source_referer("http://127.0.0.1/private", &url).is_none());
    }
    #[test]
    fn chosen_original_preserves_source_tags_and_receipt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("downloaded.png");
        fs::write(&path, bytes(ImageFormat::Png)).unwrap();
        let mut store = Store::open(&dir.path().join("library")).unwrap();
        let tag = store.create_tag("手动原图").unwrap();
        let mut request = req("原网页标题");
        request.tag_ids.push(tag.id.clone());
        let result = store
            .commit_image(&request, prepare_local(&path, &request).unwrap())
            .unwrap();
        let file = store.file(result["id"].as_str().unwrap()).unwrap();
        assert_eq!(file.tags[0].id, tag.id);
        let source = file.web_source.unwrap();
        assert_eq!(source.url, request.url);
        assert_eq!(source.image.unwrap().url, request.image_url);
        assert!(store.image_receipt(&request).unwrap().unwrap()["duplicate"]
            .as_bool()
            .unwrap());
        assert_eq!(fs::read(&path).unwrap(), bytes(ImageFormat::Png));
        fs::write(&path, b"<html>not an image</html>").unwrap();
        assert!(prepare_local(&path, &request)
            .err()
            .unwrap()
            .starts_with("IMAGE_TYPE"));
        assert!(prepare_local(dir.path(), &request)
            .err()
            .unwrap()
            .starts_with("IMAGE_LOCAL_READ"));
        let huge = dir.path().join("huge.png");
        fs::File::create(&huge)
            .unwrap()
            .set_len(MAX_BYTES + 1)
            .unwrap();
        assert!(prepare_local(&huge, &request)
            .err()
            .unwrap()
            .starts_with("IMAGE_LIMIT"));
    }
    #[test]
    fn local_picker_tasks_are_bounded_serialized_and_recoverable() {
        let mut tasks = LocalTasks::default();
        let request = req("原图任务");
        assert!(tasks.start(&request).unwrap().0);
        assert!(!tasks.start(&request).unwrap().0);
        assert!(tasks
            .start(&req("其他任务"))
            .unwrap_err()
            .starts_with("BRIDGE_BUSY"));
        let mut changed = request.clone();
        changed.title = "变化".into();
        assert!(tasks
            .start(&changed)
            .unwrap_err()
            .starts_with("CAPTURE_CONFLICT"));
        tasks.finish(&request.request_id, json!({"status":"cancelled"}));
        assert_eq!(
            tasks.status(&request.request_id).unwrap()["status"],
            "cancelled"
        );
        assert!(tasks.start(&request).unwrap().0);
        tasks.finish(
            &request.request_id,
            json!({"status":"done","result":{"id":"fixture","duplicate":false}}),
        );
        for _ in 0..25 {
            let r = req("后续");
            tasks.start(&r).unwrap();
            tasks.finish(&r.request_id, json!({"status":"cancelled"}));
        }
        assert!(tasks.entries.len() <= 20);
        assert!(tasks
            .status("missing")
            .unwrap_err()
            .starts_with("IMAGE_LOCAL_LOST"));
    }
    #[test]
    fn image_formats_limits_and_corruption_are_checked() {
        for f in [
            ImageFormat::Png,
            ImageFormat::Jpeg,
            ImageFormat::WebP,
            ImageFormat::Gif,
            ImageFormat::Bmp,
        ] {
            let p = inspect_bytes(
                bytes(f),
                "https://example.test/a",
                "https://example.test/a",
                None,
            )
            .unwrap();
            assert_eq!((p.image.width, p.image.height), (8, 6));
            assert_eq!(p.image.sha256.len(), 64);
        }
        assert!(inspect_bytes(
            vec![0; MAX_BYTES as usize + 1],
            "https://example.test/a",
            "https://example.test/a",
            None
        )
        .err()
        .unwrap()
        .starts_with("IMAGE_LIMIT"));
        for b in [
            b"<svg>unsafe</svg>".to_vec(),
            b"<html>not image</html>".to_vec(),
            bytes(ImageFormat::Png)[..30].to_vec(),
        ] {
            assert!(
                inspect_bytes(b, "https://example.test/a", "https://example.test/a", None).is_err()
            );
        }
        assert!(inspect_bytes(
            bytes(ImageFormat::Png),
            "https://example.test/a",
            "https://example.test/a",
            Some("image/jpeg")
        )
        .is_err());
        let mut p = bytes(ImageFormat::Png);
        p[16..20].copy_from_slice(&10000u32.to_be_bytes());
        p[20..24].copy_from_slice(&11000u32.to_be_bytes());
        let mut crc = 0xffffffffu32;
        for b in &p[12..29] {
            crc ^= u32::from(*b);
            for _ in 0..8 {
                crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb88320 } else { 0 };
            }
        }
        p[29..33].copy_from_slice(&(!crc).to_be_bytes());
        assert!(
            inspect_bytes(p, "https://example.test/a", "https://example.test/a", None)
                .err()
                .unwrap()
                .starts_with("IMAGE_LIMIT")
        );
    }
    #[test]
    fn added_formats_survive_refresh_preview_and_package() {
        for fmt in [ImageFormat::Gif, ImageFormat::Bmp] {
            let dir = tempfile::tempdir().unwrap();
            let mut store = Store::open(dir.path()).unwrap();
            let prepared = inspect_bytes(
                bytes(fmt),
                "https://example.test/a",
                "https://example.test/a",
                None,
            )
            .unwrap();
            let response = store.commit_image(&req("extra format"), prepared).unwrap();
            let id = response["id"].as_str().unwrap();
            let f = store.refresh_file(id).unwrap();
            assert_eq!(f.kind, "image");
            assert_eq!(
                fsops::preview(dir.path(), &f, false).unwrap()["kind"],
                "image"
            );
            let pack = dir.path().join("format.sqtagpack");
            let control = crate::transfer::Control::default();
            let plan = store
                .transfer_plan(&json!({"scope":"selected","ids":[id]}))
                .unwrap();
            let fingerprint = plan.info["fingerprint"].as_str().unwrap().to_owned();
            crate::transfer::export(plan, &pack, &fingerprint, &control).unwrap();
            assert!(crate::transfer::preview(&pack, &control).is_ok());
        }
    }
    #[test]
    fn local_reserved_and_mixed_address_targets_are_denied() {
        for text in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "192.168.1.1",
            "192.0.2.1",
            "198.18.0.1",
            "203.0.113.1",
            "224.0.0.1",
            "::1",
            "fe80::1",
            "fc00::1",
            "::ffff:127.0.0.1",
            "2001:db8::1",
            "2002:7f00:1::1",
        ] {
            assert!(!public_ip(text.parse().unwrap()), "{text}");
        }
        for text in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
            assert!(public_ip(text.parse().unwrap()));
        }
        for text in [
            "http://127.0.0.1/x",
            "http://[::1]/x",
            "http://169.254.169.254/x",
        ] {
            let mut r = req("拒绝本机");
            r.image_url = text.into();
            assert!(download(&r).err().unwrap().starts_with("IMAGE_URL"));
        }
        for text in [
            "data:image/png;base64,AAAA",
            "file:///C:/private",
            "https://user:password@example.test/a",
        ] {
            assert!(Request::parse(&json!({"requestId":id(),"kind":"web_image","title":"无效","url":"https://example.test/","imageUrl":text})).is_err());
        }
    }
    #[test]
    fn streamed_http_body_limits_types_and_truncation() {
        use std::net::TcpListener;
        fn fixture_response(header: String, body: Vec<u8>) -> reqwest::blocking::Response {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut incoming = [0; 4096];
                let _ = stream.read(&mut incoming);
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(&body);
            });
            reqwest::blocking::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap()
                .get(format!("http://{address}/"))
                .send()
                .unwrap()
        }
        let b = bytes(ImageFormat::Png);
        let response=fixture_response(format!("HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",b.len()),b);
        assert!(
            response_bytes(response, "https://example.test/a", "https://example.test/a").is_ok()
        );
        let response=fixture_response(format!("HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",MAX_BYTES+1),vec![]);
        assert!(
            response_bytes(response, "https://example.test/a", "https://example.test/a")
                .err()
                .unwrap()
                .starts_with("IMAGE_LIMIT")
        );
        let response=fixture_response("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 1\r\nConnection: close\r\n\r\n".into(),vec![0]);
        assert!(
            response_bytes(response, "https://example.test/a", "https://example.test/a")
                .err()
                .unwrap()
                .starts_with("IMAGE_TYPE")
        );
        let response=fixture_response("HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: 1000\r\nConnection: close\r\n\r\n".into(),vec![0]);
        assert!(
            response_bytes(response, "https://example.test/a", "https://example.test/a")
                .err()
                .unwrap()
                .starts_with("IMAGE_DOWNLOAD")
        );
        let response=fixture_response("HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n".into(),vec![0;MAX_BYTES as usize+1]);
        assert!(
            response_bytes(response, "https://example.test/a", "https://example.test/a")
                .err()
                .unwrap()
                .starts_with("IMAGE_LIMIT")
        );
    }
    #[test]
    fn receipts_content_dedupe_and_failed_storage_preserve_data() {
        let d = tempfile::tempdir().unwrap();
        let mut store = Store::open(d.path()).unwrap();
        let a = req("第一来源");
        let first = store.commit_image(&a, prepared()).unwrap();
        let fid = first["id"].as_str().unwrap();
        let file = store.file(fid).unwrap();
        assert_eq!(file.kind, "image");
        assert!(file.tags.is_empty());
        assert!(file.ai_task.is_none());
        assert_eq!(store.image_receipt(&a).unwrap().unwrap()["id"], first["id"]);
        let mut different = req("第二来源");
        different.image_url = "https://example.test/different-url.png".into();
        let duplicate = store.commit_image(&different, prepared()).unwrap();
        assert_eq!(duplicate["id"], first["id"]);
        assert!(duplicate["duplicate"].as_bool().unwrap());
        assert_eq!(
            store.file(fid).unwrap().web_source.unwrap().title,
            "第一来源"
        );
        let mut conflict = a.clone();
        conflict.title = "改变同一个请求".into();
        assert!(store
            .image_receipt(&conflict)
            .unwrap_err()
            .starts_with("CAPTURE_CONFLICT"));
        store
            .conn
            .execute("UPDATE files SET removed_at=1 WHERE id=?", [fid])
            .unwrap();
        assert!(store
            .image_receipt(&a)
            .unwrap_err()
            .starts_with("CAPTURE_REMOVED"));
        let other = tempfile::tempdir().unwrap();
        let mut blocked = Store::open(other.path()).unwrap();
        fs::write(other.path().join("captured-images"), b"obstruction").unwrap();
        let r = req("恢复");
        assert!(blocked
            .commit_image(&r, prepared())
            .unwrap_err()
            .starts_with("CAPTURE_STORAGE"));
        assert!(blocked.image_receipt(&r).unwrap().is_none());
        fs::remove_file(other.path().join("captured-images")).unwrap();
        assert_eq!(
            blocked.commit_image(&r, prepared()).unwrap()["duplicate"],
            false
        );
        blocked.validate().unwrap();
    }
    #[test]
    fn image_package_contains_original_and_source_backup_does_not_invent_originals() {
        let d = tempfile::tempdir().unwrap();
        let mut sender = Store::open(&d.path().join("sender")).unwrap();
        let request = req("图片 / : 标题");
        let result = sender.commit_image(&request, prepared()).unwrap();
        let fid = result["id"].as_str().unwrap();
        let original = sender.file(fid).unwrap();
        let tag = sender.create_tag("网页图片").unwrap();
        sender
            .conn
            .execute(
                "INSERT INTO file_tags(file_id,tag_id) VALUES(?,?)",
                params![fid, tag.id],
            )
            .unwrap();
        sender
            .conn
            .execute(
                "UPDATE files SET note='图片备注',favorite=1 WHERE id=?",
                [fid],
            )
            .unwrap();
        let backup = d.path().join("meta.sqtagbackup");
        sender.export_backup(&backup).unwrap();
        let plan = sender.transfer_plan(&json!({"ids":[fid]})).unwrap();
        let fingerprint = plan.info["fingerprint"].as_str().unwrap().to_owned();
        let pack = d.path().join("image.sqtagpack");
        let control = crate::transfer::Control::default();
        crate::transfer::export(plan, &pack, &fingerprint, &control).unwrap();
        let mut zip = zip::ZipArchive::new(fs::File::open(&pack).unwrap()).unwrap();
        let mut raw = String::new();
        zip.by_name("manifest.json")
            .unwrap()
            .read_to_string(&mut raw)
            .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&raw).unwrap()["format"], 7);
        drop(zip);
        let info = crate::transfer::preview(&pack, &control).unwrap();
        let mut receiver = Store::open(&d.path().join("receiver")).unwrap();
        fs::create_dir(d.path().join("received")).unwrap();
        receiver
            .import_package(
                &pack,
                &d.path().join("received"),
                info["fingerprint"].as_str().unwrap(),
                &control,
            )
            .unwrap();
        let rows = receiver
            .query(&Query {
                limit: 100,
                ..Default::default()
            })
            .unwrap();
        let file = &rows["files"][0];
        assert_eq!(file["kind"], "image");
        assert_eq!(file["note"], "图片备注");
        assert_eq!(file["favorite"], true);
        assert_eq!(file["tags"][0]["name"], "网页图片");
        assert_eq!(
            fs::read(file["path"].as_str().unwrap()).unwrap(),
            fs::read(&original.path).unwrap()
        );
        assert_eq!(
            file["webSource"]["image"]["sha256"],
            original
                .web_source
                .as_ref()
                .unwrap()
                .image
                .as_ref()
                .unwrap()
                .sha256
        );
        assert_eq!(
            receiver.commit_image(&req("再次采集"), prepared()).unwrap()["id"],
            file["id"]
        );
        fs::rename(&original.path, d.path().join("moved.png")).unwrap();
        let mut restored = Store::open(&d.path().join("restored")).unwrap();
        restored.restore_backup(&backup).unwrap();
        assert!(restored
            .file(fid)
            .unwrap()
            .web_source
            .unwrap()
            .image
            .is_some());
        assert_ne!(restored.refresh_file(fid).unwrap().status, "available");
        assert!(!restored.root.join("captured-images").exists());
        receiver.validate().unwrap();
        restored.validate().unwrap();
    }
    #[test]
    fn v4_upgrade_keeps_protective_backup() {
        let d = tempfile::tempdir().unwrap();
        let old = Store::open(d.path()).unwrap();
        old.conn
            .execute_batch("DELETE FROM schema_migrations WHERE version=5;PRAGMA user_version=4;")
            .unwrap();
        drop(old);
        let upgraded = Store::open(d.path()).unwrap();
        assert_eq!(
            upgraded
                .conn
                .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            7
        );
        assert!(d
            .path()
            .join("backups/before-image-capture-schema-v4.sqlite")
            .exists());
    }
}
