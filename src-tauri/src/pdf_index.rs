//! Page-scoped PDF text extraction using the dependency already used for AI.
//! Bounds stored text and page work, not the parser's total decompression memory.
use crate::{
    fsops,
    model::*,
    search::{Text, READ_LIMIT},
};
use serde_json::json;
use std::{
    fs::File,
    io::{Read, Write},
    time::{Duration, Instant},
};

pub const SOURCE_LIMIT: u64 = 32 * 1024 * 1024;
pub const PAGE_LIMIT: usize = 500;
const PAGE_TEXT_LIMIT: usize = 256 * 1024;

struct PageText {
    value: Vec<u8>,
    limit: usize,
    full: bool,
}
impl Write for PageText {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.value.len() + bytes.len() > self.limit {
            self.full = true;
            return Err(std::io::Error::other("PDF page text budget reached"));
        }
        self.value.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub fn indexed(f: &FileRecord, mut current: impl FnMut() -> Result<bool>) -> Result<Text> {
    // Some malformed PDFs panic inside the upstream parser. Record a failed
    // task instead of losing the background worker and all subsequent work.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| extract(f, &mut current)))
        .unwrap_or_else(|panic| {
            let reason = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("");
            if reason.contains("unsupported encoding") || reason.contains("unsupported ToUnicode") {
                Err(err(
                    "PDF_FONT_UNSUPPORTED",
                    "PDF 字体编码暂不支持；可用默认程序阅读或另存为带 Unicode 字体映射的 PDF",
                ))
            } else {
                Err(err(
                    "PDF_INVALID",
                    "PDF 结构或字体无法解析，可用默认程序查看",
                ))
            }
        })
}
fn extract(f: &FileRecord, current: &mut impl FnMut() -> Result<bool>) -> Result<Text> {
    if !current()? {
        return Err(err("INDEX_CANCELLED", "索引任务已停止"));
    }
    let mut file = File::open(&f.path).map_err(|e| err("ACCESS_DENIED", e))?;
    let before = file.metadata().map_err(|e| err("ACCESS_DENIED", e))?;
    if f.status != "available"
        || fsops::revision(&before) != f.revision
        || fsops::identity(&file) != f.identity
    {
        return Err(err("FILE_CHANGED", "文件已改变，请刷新"));
    }
    if before.len() > SOURCE_LIMIT {
        return Err(err(
            "INDEX_TOO_LARGE",
            "PDF 正文索引源文件上限 32 MiB；仍可在 64 MiB 范围内预览",
        ));
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(SOURCE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| err("ACCESS_DENIED", e))?;
    if bytes.len() as u64 > SOURCE_LIMIT {
        return Err(err("INDEX_TOO_LARGE", "PDF 超过 32 MiB 索引范围"));
    }
    let started = Instant::now();
    let doc = pdf_extract::Document::load_mem(&bytes)
        .map_err(|_| err("PDF_INVALID", "PDF 损坏或无法解析，可用默认程序查看"))?;
    if doc.is_encrypted() || doc.encryption_state.is_some() {
        return Err(err(
            "PDF_ENCRYPTED",
            "加密 PDF 不建立正文索引；可在预览中输入密码阅读",
        ));
    }
    let pages = doc.get_pages();
    if pages.is_empty() {
        return Err(err("PDF_INVALID", "PDF 没有可读取的页面"));
    }
    let mut out = Text {
        body: String::new(),
        normalized: String::new(),
        encoding: "PDF 文本".into(),
        truncated: pages.len() > PAGE_LIMIT,
        locations: vec![],
        coverage: String::new(),
    };
    let mut chars = 0;
    let mut key_chars = 0;
    let mut processed = 0;
    let mut empty = 0;
    let mut limited_pages = 0;
    for page in pages.keys().take(PAGE_LIMIT) {
        if !current()? {
            return Err(err("INDEX_CANCELLED", "索引任务已停止"));
        }
        if started.elapsed() > Duration::from_secs(15) || out.body.len() + 1 >= READ_LIMIT {
            out.truncated = true;
            break;
        }
        let mut text = PageText {
            value: vec![],
            limit: PAGE_TEXT_LIMIT.min(READ_LIMIT - out.body.len() - 1),
            full: false,
        };
        let result = {
            let mut writer = pdf_extract::PlainTextOutput::new(&mut text as &mut dyn Write);
            pdf_extract::output_doc_page(&doc, &mut writer, *page)
        };
        if result.is_err() && !text.full {
            return Err(err(
                "PDF_INVALID",
                format!("PDF 第 {page} 页提取失败；未保存不完整的索引"),
            ));
        }
        processed += 1;
        if text.full {
            out.truncated = true;
            limited_pages += 1;
        }
        let value = String::from_utf8(text.value)
            .map_err(|_| err("PDF_INVALID", "PDF 字体文字编码无法转换"))?;
        if value.trim().is_empty() {
            empty += 1;
            continue;
        }
        let normalized = key(&value);
        out.locations.push(json!({"kind":"pdf","page":page,"label":format!("第 {page} 页"),"start":chars+1,"length":value.chars().count(),"keyStart":key_chars+1,"keyLength":normalized.chars().count()}));
        chars += value.chars().count() + 1;
        key_chars += normalized.chars().count() + 1;
        out.body.push_str(&value);
        out.body.push('\n');
        out.normalized.push_str(&normalized);
        out.normalized.push('\n');
    }
    if out.body.trim().is_empty() {
        return Err(err(
            "PDF_NO_TEXT",
            format!(
                "已检查 {processed}/{} 页，未提取到文字；可能为扫描件或字体无法解码，暂不支持 OCR",
                pages.len()
            ),
        ));
    }
    out.coverage = format!("已检查 {processed}/{} 页；{empty} 页无可提取文字，{limited_pages} 页文字截取。最多前 500 页、单页 256 KiB、正文合计 2 MiB；不含 OCR，按页内容流顺序提取", pages.len());
    Ok(out)
}

#[cfg(test)]
pub(crate) fn fixture(pages: &[&str]) -> Vec<u8> {
    let mut doc = pdf_extract::Document::with_version("1.4");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(
        pdf_extract::dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"},
    );
    let resources =
        doc.add_object(pdf_extract::dictionary! {"Font"=>pdf_extract::dictionary!{"F1"=>font}});
    let mut kids = vec![];
    for text in pages {
        let content = pdf_extract::content::Content {
            operations: vec![
                pdf_extract::content::Operation::new("BT", vec![]),
                pdf_extract::content::Operation::new("Tf", vec!["F1".into(), 12.into()]),
                pdf_extract::content::Operation::new("Td", vec![50.into(), 150.into()]),
                pdf_extract::content::Operation::new(
                    "Tj",
                    vec![pdf_extract::Object::string_literal(*text)],
                ),
                pdf_extract::content::Operation::new("ET", vec![]),
            ],
        }
        .encode()
        .unwrap();
        let stream = doc.add_object(pdf_extract::Stream::new(
            pdf_extract::dictionary! {},
            content,
        ));
        let page = doc.add_object(pdf_extract::dictionary!{"Type"=>"Page","Parent"=>pages_id,"MediaBox"=>vec![0.into(),0.into(),300.into(),200.into()],"Resources"=>resources,"Contents"=>stream});
        kids.push(page.into());
    }
    doc.objects.insert(
        pages_id,
        pdf_extract::dictionary! {"Type"=>"Pages","Kids"=>kids,"Count"=>pages.len() as i64}.into(),
    );
    let catalog = doc.add_object(pdf_extract::dictionary! {"Type"=>"Catalog","Pages"=>pages_id});
    doc.trailer.set("Root", catalog);
    let mut bytes = vec![];
    doc.save_to(&mut bytes).unwrap();
    bytes
}
