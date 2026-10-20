//! Bounded, content-only readers. Never extract archives or resolve external resources.
use crate::{fsops, model::*};
use quick_xml::{events::Event, Reader};
use serde_json::{json, Value};
use std::{collections::HashMap, fs::File, io::Read};

#[path = "office_index.rs"]
mod index;
#[path = "office_ppt.rs"]
mod ppt;
#[path = "office_word.rs"]
mod word;
pub use index::indexed;

const ENTRY: usize = 8 * 1024 * 1024;
const ROWS: usize = 10_000;
const COLS: usize = 100;

#[derive(Default, Debug)]
struct Node {
    name: String,
    attrs: HashMap<String, String>,
    text: String,
    children: Vec<Node>,
}
impl Node {
    fn attr(&self, k: &str) -> &str {
        self.attrs.get(k).map(String::as_str).unwrap_or("")
    }
    fn child(&self, k: &str) -> Option<&Node> {
        self.children.iter().find(|n| n.name == k)
    }
    fn all<'a>(&'a self, k: &str, out: &mut Vec<&'a Node>) {
        if self.name == k {
            out.push(self);
        }
        for n in &self.children {
            n.all(k, out);
        }
    }
    fn texts(&self, k: &str) -> String {
        let mut out = vec![];
        self.all(k, &mut out);
        out.iter().map(|n| n.text.as_str()).collect()
    }
}
fn invalid() -> String {
    err(
        "OFFICE_INVALID",
        "文档损坏、加密或格式不受支持，请打开原文件",
    )
}
fn limit() -> String {
    err("PREVIEW_TOO_LARGE", "文档内容超过预览预算，请打开原文件")
}
fn xml(bytes: &[u8]) -> Result<Node> {
    let mut reader = Reader::from_reader(bytes);
    let mut stack = vec![Node::default()];
    let mut count = 0;
    loop {
        let event = reader.read_event().map_err(|_| invalid())?;
        let empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                count += 1;
                if count > 200_000 || stack.len() > 64 {
                    return Err(limit());
                }
                let mut n = Node {
                    name: e.local_name().as_ref().to_string(),
                    ..Default::default()
                };
                for a in e.attributes() {
                    let a = a.map_err(|_| invalid())?;
                    n.attrs.insert(
                        a.key.as_ref().to_owned(),
                        a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .map_err(|_| invalid())?
                            .into_owned(),
                    );
                    n.attrs.insert(
                        a.key.local_name().as_ref().to_string(),
                        a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .map_err(|_| invalid())?
                            .into_owned(),
                    );
                }
                if empty {
                    stack.last_mut().unwrap().children.push(n);
                } else {
                    stack.push(n);
                }
            }
            Event::End(_) => {
                if stack.len() < 2 {
                    return Err(invalid());
                }
                let n = stack.pop().unwrap();
                stack.last_mut().unwrap().children.push(n);
            }
            Event::Text(t) => stack
                .last_mut()
                .unwrap()
                .text
                .push_str(&t.xml_content(quick_xml::XmlVersion::Implicit1_0)),
            Event::GeneralRef(r) => {
                let ch = r
                    .resolve_char_ref()
                    .map_err(|_| invalid())?
                    .or_else(|| match r.as_ref() {
                        "amp" => Some('&'),
                        "lt" => Some('<'),
                        "gt" => Some('>'),
                        "quot" => Some('"'),
                        "apos" => Some('\''),
                        _ => None,
                    })
                    .ok_or_else(invalid)?;
                stack.last_mut().unwrap().text.push(ch);
            }
            Event::DocType(_) => return Err(invalid()),
            Event::Eof => break,
            _ => {}
        }
    }
    if stack.len() != 1 {
        return Err(invalid());
    }
    Ok(stack.pop().unwrap())
}
fn entry(z: &mut zip::ZipArchive<File>, path: &str, budget: &mut usize) -> Result<Node> {
    let f = z.by_name(path).map_err(|_| invalid())?;
    if f.size() > ENTRY as u64 || f.size() > *budget as u64 {
        return Err(limit());
    }
    let mut bytes = vec![];
    f.take(ENTRY as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    if bytes.len() > ENTRY || bytes.len() > *budget {
        return Err(limit());
    }
    *budget -= bytes.len();
    xml(&bytes)
}
fn paragraph(n: &Node) -> Value {
    let mut runs = vec![];
    n.all("r", &mut runs);
    let runs: Vec<_> = runs
        .iter()
        .map(|r| {
            let mut text = String::new();
            for c in &r.children {
                match c.name.as_str() {
                    "t" => text.push_str(&c.text),
                    "tab" => text.push('\t'),
                    "br" | "cr" => text.push('\n'),
                    _ => {}
                }
            }
            let enabled = |k| {
                r.child("rPr")
                    .and_then(|p| p.child(k))
                    .map(|x| !matches!(x.attr("val"), "0" | "false" | "off"))
                    .unwrap_or(false)
            };
            json!({"text":text,"bold":enabled("b"),"italic":enabled("i")})
        })
        .collect();
    let style = n
        .child("pPr")
        .and_then(|p| p.child("pStyle"))
        .map(|s| s.attr("val"))
        .unwrap_or("")
        .to_lowercase();
    let heading = style
        .strip_prefix("heading")
        .and_then(|s| s.parse::<u8>().ok())
        .filter(|n| (1..=6).contains(n))
        .unwrap_or(0);
    json!({"type":"paragraph","runs":runs,"heading":heading})
}
fn docx(z: &mut zip::ZipArchive<File>, budget: &mut usize) -> Result<Value> {
    docx_mode(z, budget, false)
}
fn docx_mode(z: &mut zip::ZipArchive<File>, budget: &mut usize, summary: bool) -> Result<Value> {
    let tree = entry(z, "word/document.xml", budget)?;
    let mut bodies = vec![];
    tree.all("body", &mut bodies);
    let body = bodies.first().ok_or_else(invalid)?;
    let mut context = word::Context::load(z, budget)?;
    let mut blocks = vec![];
    for n in &body.children {
        match n.name.as_str() {
            "p" => blocks.push(if summary {
                paragraph(n)
            } else {
                context.paragraph(n, z, budget)
            }),
            "tbl" => {
                let mut rows = vec![];
                if n.children.iter().filter(|n| n.name == "tr").count() > 1000 {
                    return Err(limit());
                }
                for tr in n.children.iter().filter(|n| n.name == "tr").take(1000) {
                    let mut cells = vec![];
                    if tr.children.iter().filter(|n| n.name == "tc").count() > COLS {
                        return Err(limit());
                    }
                    for tc in tr.children.iter().filter(|n| n.name == "tc").take(COLS) {
                        let mut ps = vec![];
                        tc.all("p", &mut ps);
                        cells.push(
                            ps.into_iter()
                                .map(|p| {
                                    if summary {
                                        paragraph(p)
                                    } else {
                                        context.paragraph(p, z, budget)
                                    }
                                })
                                .collect::<Vec<_>>(),
                        );
                    }
                    rows.push(cells);
                }
                blocks.push(json!({"type":"table","rows":rows}));
            }
            _ => {}
        }
        if blocks.len() > 5000 {
            return Err(limit());
        }
    }
    context.warnings.sort();
    context.warnings.dedup();
    Ok(json!({"kind":"docx","blocks":blocks,"images":context.images,"warnings":context.warnings}))
}
fn column(s: &str) -> Option<usize> {
    let mut col = 0usize;
    for b in s.bytes().take_while(u8::is_ascii_alphabetic) {
        col = col
            .checked_mul(26)?
            .checked_add((b.to_ascii_uppercase() - b'A' + 1) as usize)?;
    }
    col.checked_sub(1).filter(|n| *n < COLS)
}
struct Workbook {
    names: Vec<String>,
    targets: Vec<Option<String>>,
    formats: HashMap<String, String>,
    xfs: Vec<String>,
    date1904: bool,
    strings: Vec<String>,
}
fn workbook(z: &mut zip::ZipArchive<File>, budget: &mut usize) -> Result<Workbook> {
    let book = entry(z, "xl/workbook.xml", budget)?;
    let rels = entry(z, "xl/_rels/workbook.xml.rels", budget)?;
    let mut sheets = vec![];
    book.all("sheet", &mut sheets);
    if sheets.is_empty() || sheets.len() > 100 {
        return Err(limit());
    }
    let names = sheets.iter().map(|s| s.attr("name").to_owned()).collect();
    let mut links = vec![];
    rels.all("Relationship", &mut links);
    let targets = sheets
        .iter()
        .map(|sheet| {
            links
                .iter()
                .find(|r| r.attr("Id") == sheet.attr("id") && r.attr("TargetMode") != "External")
                .map(|link| {
                    let t = link.attr("Target").trim_start_matches('/');
                    if t.starts_with("xl/") {
                        t.to_owned()
                    } else {
                        format!("xl/{t}")
                    }
                })
        })
        .collect();
    let styles = if z.file_names().any(|n| n == "xl/styles.xml") {
        Some(entry(z, "xl/styles.xml", budget)?)
    } else {
        None
    };
    let mut formats = HashMap::new();
    let mut xfs = vec![];
    if let Some(tree) = &styles {
        let mut ns = vec![];
        tree.all("numFmt", &mut ns);
        if ns.len() > 5000 || ns.iter().any(|n| n.attr("formatCode").len() > 256) {
            return Err(limit());
        }
        for n in ns {
            formats.insert(
                n.attr("numFmtId").to_owned(),
                n.attr("formatCode").to_owned(),
            );
        }
        let mut groups = vec![];
        tree.all("cellXfs", &mut groups);
        if let Some(group) = groups.first() {
            for xf in &group.children {
                if xf.name == "xf" {
                    xfs.push(xf.attr("numFmtId").to_owned());
                }
            }
        }
    }
    let mut props = vec![];
    book.all("workbookPr", &mut props);
    let date1904 = props
        .first()
        .map(|p| matches!(p.attr("date1904"), "1" | "true"))
        .unwrap_or(false);
    let mut strings = vec![];
    if z.file_names().any(|n| n == "xl/sharedStrings.xml") {
        let shared = entry(z, "xl/sharedStrings.xml", budget)?;
        let mut items = vec![];
        shared.all("si", &mut items);
        strings = items.iter().map(|s| s.texts("t")).collect();
    }
    Ok(Workbook {
        names,
        targets,
        formats,
        xfs,
        date1904,
        strings,
    })
}
fn xlsx(z: &mut zip::ZipArchive<File>, budget: &mut usize, selected: usize) -> Result<Value> {
    let book = workbook(z, budget)?;
    xlsx_sheet(z, budget, &book, selected)
}
fn xlsx_sheet(
    z: &mut zip::ZipArchive<File>,
    budget: &mut usize,
    book: &Workbook,
    selected: usize,
) -> Result<Value> {
    let target = book
        .targets
        .get(selected)
        .and_then(Option::as_ref)
        .ok_or_else(invalid)?;
    if !target.starts_with("xl/worksheets/") || target.contains("..") || target.contains('\\') {
        return Err(invalid());
    }
    let Workbook {
        names,
        formats,
        xfs,
        date1904,
        strings,
        ..
    } = book;
    let tree = entry(z, &target, budget)?;
    let mut raw = vec![];
    tree.all("row", &mut raw);
    let mut rows = vec![];
    let mut columns = 1;
    let mut cells = 0;
    let mut text_bytes = 0usize;
    let mut truncated = false;
    for row in raw {
        if rows.len() >= ROWS {
            truncated = true;
            break;
        }
        let mut values = vec![];
        for c in row.children.iter().filter(|n| n.name == "c") {
            cells += 1;
            if cells > 100_000 {
                truncated = true;
                break;
            }
            let Some(col) = column(c.attr("r")) else {
                truncated = true;
                continue;
            };
            if values.len() <= col {
                values.resize(col + 1, json!(""));
            }
            let v = c.child("v").map(|v| v.text.clone()).unwrap_or_default();
            let value = match c.attr("t") {
                "s" => strings
                    .get(v.parse::<usize>().map_err(|_| invalid())?)
                    .ok_or_else(invalid)?
                    .chars()
                    .take(4097)
                    .collect(),
                "inlineStr" => c.texts("t"),
                "b" => if v == "1" { "TRUE" } else { "FALSE" }.into(),
                _ => v,
            };
            let mut value: String = value;
            if value.chars().take(4097).count() > 4096 {
                value = value.chars().take(4096).collect();
                truncated = true;
            }
            text_bytes += value.len();
            if text_bytes > 2 * 1024 * 1024 {
                truncated = true;
                break;
            }
            let style = c.attr("s").parse::<usize>().ok().and_then(|i| xfs.get(i));
            let format_id = style
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(if c.attr("s").is_empty() { 0 } else { u32::MAX });
            let format_code = style.and_then(|s| formats.get(s));
            values[col] = json!({"value":value,"formula":c.child("f").map(|n|n.text.as_str()),
                "numeric":matches!(c.attr("t"), "" | "n"), "formatId":format_id,"formatCode":format_code});
            columns = columns.max(col + 1);
        }
        rows.push(
            json!({"number":row.attr("r").parse::<usize>().unwrap_or(rows.len()+1),"cells":values}),
        );
        if cells > 100_000 || text_bytes > 2 * 1024 * 1024 {
            break;
        }
    }
    let mut merged = vec![];
    tree.all("mergeCell", &mut merged);
    let merged: Vec<_> = merged.iter().take(1000).map(|n| n.attr("ref")).collect();
    Ok(
        json!({"mergedRanges":merged,"kind":"sheet","sheets":names,"selected":selected,"rows":rows,"columns":columns,"truncated":truncated,"date1904":date1904}),
    )
}
fn csv(bytes: &[u8], delimiter: char) -> Result<Value> {
    let text = if bytes.starts_with(&[255, 254]) {
        let (text, _, bad) = encoding_rs::UTF_16LE.decode(&bytes[2..]);
        if bad {
            return Err(invalid());
        }
        text.into_owned()
    } else if bytes.starts_with(&[254, 255]) {
        let (text, _, bad) = encoding_rs::UTF_16BE.decode(&bytes[2..]);
        if bad {
            return Err(invalid());
        }
        text.into_owned()
    } else if let Ok(s) = std::str::from_utf8(bytes) {
        s.trim_start_matches('\u{feff}').into()
    } else {
        let (s, _, bad) = encoding_rs::GB18030.decode(bytes);
        if bad {
            return Err(invalid());
        }
        s.into_owned()
    };
    if text.contains('\0') {
        return Err(invalid());
    }
    let mut chars = text.chars().peekable();
    let mut quoted = false;
    let mut closed = false;
    let mut field = String::new();
    let mut row = vec![];
    let mut rows = vec![];
    let mut columns = 1;
    let mut cell_count = 0;
    let mut truncated = false;
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' && field.is_empty() && !closed {
            quoted = true;
        } else if c == delimiter || c == '\r' || c == '\n' {
            if row.len() < COLS && cell_count + row.len() < 100_000 {
                row.push(json!({"value":std::mem::take(&mut field)}));
            } else {
                field.clear();
                truncated = true;
            }
            closed = false;
            if c != delimiter {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                columns = columns.max(row.len());
                cell_count += row.len();
                rows.push(json!({"number":rows.len()+1,"cells":std::mem::take(&mut row)}));
                if rows.len() >= ROWS || cell_count >= 100_000 {
                    truncated |= chars.peek().is_some();
                    break;
                }
            }
        } else {
            if closed {
                return Err(invalid());
            }
            field.push(c);
        }
    }
    if quoted {
        return Err(invalid());
    }
    if !field.is_empty() || !row.is_empty() || closed {
        if row.len() < COLS && cell_count + row.len() < 100_000 {
            row.push(json!({"value":field}));
        } else {
            truncated = true;
        }
        columns = columns.max(row.len());
        rows.push(json!({"number":rows.len()+1,"cells":row}));
    }
    Ok(
        json!({"kind":"sheet","sheets":["数据"],"selected":0,"rows":rows,"columns":columns,"truncated":truncated}),
    )
}
pub fn document(f: &FileRecord, selected: usize) -> Result<Value> {
    document_mode(f, selected, false)
}
fn document_mode(f: &FileRecord, selected: usize, summary: bool) -> Result<Value> {
    if !matches!(
        f.extension.to_lowercase().as_str(),
        "docx" | "xlsx" | "csv" | "tsv" | "pptx"
    ) {
        return Err(err(
            "PREVIEW_UNSUPPORTED",
            "此格式暂不支持内置预览，请打开原文件",
        ));
    }
    if f.status != "available" {
        return Err(err("FILE_UNAVAILABLE", "文件不可用，请刷新文件状态"));
    }
    let file = File::open(&f.path).map_err(|_| invalid())?;
    let meta = file.metadata().map_err(|_| invalid())?;
    if fsops::revision(&meta) != f.revision || fsops::identity(&file) != f.identity {
        return Err(err("FILE_CHANGED", "文件已变化，请刷新"));
    }
    let ext = f.extension.to_lowercase();
    let max = if matches!(ext.as_str(), "csv" | "tsv") {
        8
    } else {
        32
    } * 1024
        * 1024;
    if meta.len() > max {
        return Err(limit());
    }
    let mut result = if matches!(ext.as_str(), "csv" | "tsv") {
        let mut bytes = vec![];
        file.take(max + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid())?;
        if bytes.len() as u64 > max {
            return Err(limit());
        }
        csv(&bytes, if ext == "tsv" { '\t' } else { ',' })?
    } else {
        let mut z = zip::ZipArchive::new(file).map_err(|_| invalid())?;
        if z.len() > 5000 {
            return Err(limit());
        }
        let mut budget = 24 * 1024 * 1024;
        if ext == "pptx" {
            ppt::document(&mut z, &mut budget)?
        } else if ext == "docx" {
            if summary {
                docx_mode(&mut z, &mut budget, true)?
            } else {
                docx(&mut z, &mut budget)?
            }
        } else {
            xlsx(&mut z, &mut budget, selected)?
        }
    };
    if std::fs::metadata(&f.path)
        .map(|m| fsops::revision(&m) != f.revision)
        .unwrap_or(true)
    {
        return Err(err("FILE_CHANGED", "预览期间文件已变化，请刷新"));
    }
    result["revision"] = json!(f.revision);
    Ok(result)
}

/// Bounded revision/identity/parser-version cache. Never stores whole Office content.
pub fn summary(root: &std::path::Path, f: &FileRecord) -> Result<Value> {
    let file = File::open(&f.path).map_err(|_| invalid())?;
    let meta = file.metadata().map_err(|_| invalid())?;
    if f.status != "available"
        || fsops::revision(&meta) != f.revision
        || fsops::identity(&file) != f.identity
    {
        return Err(err("FILE_CHANGED", "文件已变化，请刷新"));
    }
    let cache = fsops::cache_path(root, f, false).with_extension("office-v044.json");
    if let Ok(file) = File::open(&cache) {
        let mut bytes = vec![];
        if file.take(64 * 1024 + 1).read_to_end(&mut bytes).is_ok() && bytes.len() <= 64 * 1024 {
            if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
                if v["revision"] == f.revision
                    && v["kind"] == "office-summary"
                    && v["text"]
                        .as_str()
                        .map(|s| s.chars().count() <= 450)
                        .unwrap_or(false)
                {
                    return Ok(v);
                }
            }
        }
    }
    let d = document_mode(f, 0, true)?;
    let mut lines = vec![];
    if d["kind"] == "docx" {
        for b in d["blocks"].as_array().into_iter().flatten().take(8) {
            let paragraphs: Vec<&Value> = if b["type"] == "table" {
                b["rows"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(2)
                    .flat_map(|r| r.as_array().into_iter().flatten().take(4))
                    .flat_map(|c| c.as_array().into_iter().flatten())
                    .collect()
            } else {
                vec![b]
            };
            for p in paragraphs {
                let text: String = p["runs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|r| r["text"].as_str())
                    .collect();
                if !text.trim().is_empty() {
                    lines.push(text);
                }
            }
        }
    } else {
        lines.push(format!(
            "{} · {} 行 · {} 列",
            d["sheets"][0].as_str().unwrap_or("数据"),
            d["rows"].as_array().map(Vec::len).unwrap_or(0),
            d["columns"]
        ));
        for r in d["rows"].as_array().into_iter().flatten().take(1) {
            lines.push(
                r["cells"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(4)
                    .filter(|c| c["numeric"].as_bool() != Some(true))
                    .filter_map(|c| c["value"].as_str())
                    .collect::<Vec<_>>()
                    .join(" · "),
            );
        }
    }
    let text: String = lines.join("\n").chars().take(450).collect();
    let v = json!({"kind":"office-summary","text":text,"revision":f.revision});
    if let Some(parent) = cache.parent() {
        if std::fs::create_dir_all(parent).is_ok() {
            if let Ok(mut temp) = tempfile::NamedTempFile::new_in(parent) {
                use std::io::Write;
                if temp.write_all(v.to_string().as_bytes()).is_ok() {
                    let _ = temp.persist(&cache);
                }
            }
        }
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn csv_quotes_encoding_and_bounds() {
        let v = csv(
            "名称,备注\r\n中文,\"a,b\n\"\"引号\"\"\"\r\n".as_bytes(),
            ',',
        )
        .unwrap();
        assert_eq!(v["rows"][1]["cells"][1]["value"], "a,b\n\"引号\"");
        assert!(csv(b"\"unfinished", ',').is_err());
        let wide = format!("{}\n", vec!["x"; 101].join(",")).repeat(1000);
        assert_eq!(csv(wide.as_bytes(), ',').unwrap()["truncated"], true);
        assert_eq!(csv(b"\xff\xfeA\0\t\0B\0", '\t').unwrap()["columns"], 2);
        assert_eq!(
            csv("x\n".repeat(10001).as_bytes(), ',').unwrap()["truncated"],
            true
        );
    }
    #[test]
    fn xml_entities_styles_and_hostile_input() {
        let n=xml(b"<p><pPr><pStyle val='Heading1'/></pPr><r><rPr><b/></rPr><t>A &amp; B &#60;</t></r></p>").unwrap();
        let p = paragraph(&n.children[0]);
        assert_eq!(p["heading"], 1);
        assert_eq!(p["runs"][0]["bold"], true);
        assert_eq!(p["runs"][0]["text"], "A & B <");
        assert!(xml(b"<!DOCTYPE x [<!ENTITY x SYSTEM 'file:///secret'>]><x>&x;</x>").is_err());
        assert!(xml(format!("{}{}", "<x>".repeat(70), "</x>".repeat(70)).as_bytes()).is_err());
        assert_eq!(column("CW1"), None);
        assert_eq!(column("AA7"), Some(26));
    }
    pub(super) fn archive(entries: &[(&str, &str)]) -> (crate::fsops::TmpDir, zip::ZipArchive<File>) {
        use std::io::Write;
        let dir = crate::fsops::TmpDir::new();
        let path = dir.path().join("office.zip");
        let mut z = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, text) in entries {
            z.start_file(
                *name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
            z.write_all(text.as_bytes()).unwrap();
        }
        z.finish().unwrap();
        let z = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
        (dir, z)
    }
    #[test]
    fn workbook_relationships_values_and_budgets() {
        let book = "<workbook><sheets><sheet name='预算' r:id='r1'/></sheets></workbook>";
        let rels =
            "<Relationships><Relationship Id='r1' Target='worksheets/sheet1.xml'/></Relationships>";
        let (_,mut z)=archive(&[("xl/workbook.xml",book),("xl/_rels/workbook.xml.rels",rels),("xl/sharedStrings.xml","<sst><si><t>中文 &amp; 标签</t></si></sst>"),("xl/worksheets/sheet1.xml","<worksheet><sheetData><row r='5'><c r='A5' t='s'><v>0</v></c><c r='C5'><f>1+2</f><v>3</v></c></row></sheetData></worksheet>")]);
        let v = xlsx(&mut z, &mut (24 * 1024 * 1024), 0).unwrap();
        assert_eq!(v["sheets"][0], "预算");
        assert_eq!(v["rows"][0]["number"], 5);
        assert_eq!(v["rows"][0]["cells"][0]["value"], "中文 & 标签");
        assert_eq!(v["rows"][0]["cells"][2]["formula"], "1+2");
        assert!(xlsx(&mut z, &mut (24 * 1024 * 1024), 9).is_err());
        let (_,mut z)=archive(&[("xl/workbook.xml",book),("xl/_rels/workbook.xml.rels","<Relationships><Relationship Id='r1' Target='https://example.com/secret' TargetMode='External'/></Relationships>")]);
        assert!(xlsx(&mut z, &mut (24 * 1024 * 1024), 0).is_err());
        let oversized = " ".repeat(ENTRY + 1);
        let (_, mut z) = archive(&[("word/document.xml", &oversized)]);
        assert!(docx(&mut z, &mut (24 * 1024 * 1024))
            .unwrap_err()
            .starts_with("PREVIEW_TOO_LARGE"));
    }
    #[test]
    fn word_table_and_invalid_xml() {
        let (_,mut z)=archive(&[("word/document.xml","<document><body><p><r><t>说明</t></r></p><tbl><tr><tc><p><r><t>表格</t></r></p></tc></tr></tbl></body></document>")]);
        let mut budget = ENTRY;
        let v = docx(&mut z, &mut budget).unwrap();
        assert_eq!(v["blocks"][1]["rows"][0][0][0]["runs"][0]["text"], "表格");
        assert!(xml(b"<document><body></document>").is_err());
    }
    #[test]
    fn numbering_styles_overrides_and_nested_restart() {
        let numbering = "<numbering><abstractNum abstractNumId='1'><lvl ilvl='0'><start val='1'/><numFmt val='decimal'/><lvlText val='%1.'/></lvl><lvl ilvl='1'><start val='1'/><numFmt val='lowerLetter'/><lvlText val='%1.%2)'/></lvl></abstractNum><num numId='2'><abstractNumId val='1'/><lvlOverride ilvl='0'><startOverride val='3'/></lvlOverride></num></numbering>";
        let styles="<styles><style styleId='List'><pPr><numPr><numId val='2'/></numPr></pPr></style><style styleId='Inherited'><basedOn val='List'/></style></styles>";
        let body="<document><body><p><pPr><pStyle val='Inherited'/></pPr><r><t>one</t></r></p><p><pPr><numPr><numId val='2'/><ilvl val='1'/></numPr></pPr><r><t>child</t></r></p><p><pPr><pStyle val='List'/></pPr></p><p><pPr><numPr><numId val='2'/><ilvl val='1'/></numPr></pPr></p></body></document>";
        let (_, mut z) = archive(&[
            ("word/document.xml", body),
            ("word/numbering.xml", numbering),
            ("word/styles.xml", styles),
        ]);
        let d = docx(&mut z, &mut (24 * 1024 * 1024)).unwrap();
        assert_eq!(d["blocks"][0]["label"], "3.");
        assert_eq!(d["blocks"][1]["label"], "3.a)");
        assert_eq!(d["blocks"][2]["label"], "4.");
        assert_eq!(d["blocks"][3]["label"], "4.a)");
    }
    #[test]
    fn embedded_images_reused_and_external_blocked() {
        use std::io::{Cursor, Write};
        let dir = crate::fsops::TmpDir::new();
        let path = dir.path().join("images.docx");
        let mut png = Cursor::new(vec![]);
        image::DynamicImage::new_rgb8(64, 32)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let mut z = zip::ZipWriter::new(File::create(&path).unwrap());
        let body=b"<document><body><p><r><t>before</t><drawing><blip embed='r1'/></drawing><t>after</t><drawing><blip embed='r1'/></drawing><drawing><blip link='external'/></drawing></r></p></body></document>";
        let rels=b"<Relationships><Relationship Id='r1' Type='http://schemas/image' Target='media/a.png'/><Relationship Id='external' Type='http://schemas/image' TargetMode='External' Target='https://example.com/a.png'/></Relationships>";
        for (name, bytes) in [
            ("word/document.xml", body.as_slice()),
            ("word/_rels/document.xml.rels", rels.as_slice()),
            ("word/media/a.png", png.get_ref().as_slice()),
        ] {
            z.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            z.write_all(bytes).unwrap();
        }
        z.finish().unwrap();
        let mut z = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
        let d = docx(&mut z, &mut (24 * 1024 * 1024)).unwrap();
        assert_eq!(d["images"].as_object().unwrap().len(), 1);
        assert_eq!(d["images"]["image0"]["width"], 64);
        let runs = &d["blocks"][0]["runs"];
        assert_eq!(runs[0]["text"], "before");
        assert_eq!(runs[1]["image"], "image0");
        assert_eq!(runs[2]["text"], "after");
        assert_eq!(runs[3]["image"], "image0");
        assert_eq!(runs[4]["imageError"], "图片未显示");
        assert!(!d["warnings"].as_array().unwrap().is_empty());
        let mut context = word::Context::load(&mut z, &mut (24 * 1024 * 1024)).unwrap();
        // A tiny archive-read budget produces a placeholder, retaining text.
        let n = xml(body).unwrap();
        let mut ps = vec![];
        n.all("p", &mut ps);
        let p = context.paragraph(ps[0], &mut z, &mut 1);
        assert_eq!(p["runs"][1]["imageError"], "图片未显示");
    }
    #[test]
    fn spreadsheet_style_indices_epoch_and_merge_metadata() {
        let (_,mut z)=archive(&[("xl/workbook.xml","<workbook><workbookPr date1904='1'/><sheets><sheet name='格式' id='r1'/></sheets></workbook>"),
            ("xl/_rels/workbook.xml.rels","<Relationships><Relationship Id='r1' Target='worksheets/sheet1.xml'/></Relationships>"),
            ("xl/styles.xml","<styleSheet><numFmts><numFmt numFmtId='164' formatCode='yyyy-mm-dd'/></numFmts><cellStyleXfs><xf numFmtId='99'/></cellStyleXfs><cellXfs><xf numFmtId='0'/><xf numFmtId='164'/><xf numFmtId='10'/></cellXfs></styleSheet>"),
            ("xl/worksheets/sheet1.xml","<worksheet><sheetData><row r='1'><c r='A1' s='1'><v>1</v></c><c r='B1' s='2'><f>1/4</f><v>0.25</v></c><c r='C1' s='1' t='inlineStr'><is><t>123</t></is></c><c r='D1' s='99'><v>42</v></c></row></sheetData><mergeCells><mergeCell ref='A3:C3'/></mergeCells></worksheet>")]);
        let d = xlsx(&mut z, &mut (24 * 1024 * 1024), 0).unwrap();
        assert_eq!(d["date1904"], true);
        assert_eq!(d["rows"][0]["cells"][0]["formatCode"], "yyyy-mm-dd");
        assert_eq!(d["rows"][0]["cells"][1]["formatId"], 10);
        assert_eq!(d["rows"][0]["cells"][1]["value"], "0.25");
        assert_eq!(d["rows"][0]["cells"][2]["numeric"], false);
        assert_eq!(d["mergedRanges"][0], "A3:C3");
        assert_eq!(d["rows"][0]["cells"][3]["formatId"], u32::MAX);
    }
    #[test]
    fn spreadsheet_repeated_text_and_format_codes_are_bounded() {
        let book = "<workbook><sheets><sheet name='数据' id='r1'/></sheets></workbook>";
        let rels =
            "<Relationships><Relationship Id='r1' Target='worksheets/sheet1.xml'/></Relationships>";
        let shared = format!("<sst><si><t>{}</t></si></sst>", "文".repeat(4500));
        let rows = (1..=600)
            .map(|n| format!("<row r='{n}'><c r='A{n}' t='s'><v>0</v></c></row>"))
            .collect::<String>();
        let sheet = format!("<worksheet><sheetData>{rows}</sheetData></worksheet>");
        let (_, mut z) = archive(&[
            ("xl/workbook.xml", book),
            ("xl/_rels/workbook.xml.rels", rels),
            ("xl/sharedStrings.xml", &shared),
            ("xl/worksheets/sheet1.xml", &sheet),
        ]);
        let d = xlsx(&mut z, &mut (24 * 1024 * 1024), 0).unwrap();
        assert_eq!(d["truncated"], true);
        assert!(d["rows"].as_array().unwrap().len() < 600);
        assert_eq!(
            d["rows"][0]["cells"][0]["value"]
                .as_str()
                .unwrap()
                .chars()
                .count(),
            4096
        );
        let styles = format!(
            "<styleSheet><numFmts><numFmt numFmtId='164' formatCode='{}'/></numFmts></styleSheet>",
            "0".repeat(257)
        );
        let (_, mut z) = archive(&[
            ("xl/workbook.xml", book),
            ("xl/_rels/workbook.xml.rels", rels),
            ("xl/styles.xml", &styles),
            ("xl/worksheets/sheet1.xml", &sheet),
        ]);
        assert!(xlsx(&mut z, &mut (24 * 1024 * 1024), 0)
            .unwrap_err()
            .starts_with("PREVIEW_TOO_LARGE"));
    }
}
