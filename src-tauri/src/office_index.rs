//! Office search reuses the preview parser's bounds, without loading Word images.
use super::*;
use crate::search::{Text, READ_LIMIT};

struct Extractor {
    text: Text,
    chars: usize,
    key_chars: usize,
}
impl Extractor {
    fn new(encoding: &str, coverage: &str) -> Self {
        Self {
            text: Text {
                body: String::new(),
                normalized: String::new(),
                encoding: encoding.into(),
                truncated: false,
                locations: vec![],
                coverage: coverage.into(),
            },
            chars: 0,
            key_chars: 0,
        }
    }
    fn push(&mut self, value: &str, mut location: Value) -> bool {
        if value.trim().is_empty() {
            return true;
        }
        // Include separators in the global UTF-8 budget. Bound location overhead too.
        if self.text.locations.len() >= 20_000 || self.text.body.len() + 1 >= READ_LIMIT {
            self.text.truncated = true;
            return false;
        }
        let remaining = READ_LIMIT - self.text.body.len() - 1;
        let original_len = value.len();
        let mut end = original_len.min(remaining);
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        let value = &value[..end];
        let normalized = key(value);
        location["start"] = json!(self.chars + 1);
        location["length"] = json!(value.chars().count());
        location["keyStart"] = json!(self.key_chars + 1);
        location["keyLength"] = json!(normalized.chars().count());
        self.chars += value.chars().count() + 1;
        self.key_chars += normalized.chars().count() + 1;
        self.text.body.push_str(value);
        self.text.body.push('\n');
        self.text.normalized.push_str(&normalized);
        self.text.normalized.push('\n');
        self.text.locations.push(location);
        if end < original_len {
            self.text.truncated = true;
            false
        } else {
            true
        }
    }
    fn sheet(&mut self, data: &Value, selected: usize) -> bool {
        self.text.truncated |= data["truncated"].as_bool().unwrap_or(false);
        let name = data["sheets"][selected].as_str().unwrap_or("数据");
        for row in data["rows"].as_array().into_iter().flatten() {
            let number = row["number"].as_u64().unwrap_or(1);
            for (col, cell) in row["cells"].as_array().into_iter().flatten().enumerate() {
                let value = cell["value"].as_str().unwrap_or("");
                let mut n = col + 1;
                let mut address = String::new();
                while n > 0 {
                    address.insert(0, (b'A' + ((n - 1) % 26) as u8) as char);
                    n = (n - 1) / 26;
                }
                address.push_str(&number.to_string());
                if !self.push(value,json!({"kind":"sheet","sheet":selected,"sheetName":name,"row":number,"column":col,"cell":address,"label":format!("{name} · {address}")})) {return false;}
            }
        }
        true
    }
}
fn plain(p: &Node) -> String {
    fn visit(n: &Node, out: &mut String) {
        if matches!(n.name.as_str(), "drawing" | "pict" | "txbxContent" | "del") {
            return;
        }
        if n.name == "r" {
            for c in &n.children {
                match c.name.as_str() {
                    "t" => out.push_str(&c.text),
                    "tab" => out.push('\t'),
                    "br" | "cr" => out.push('\n'),
                    _ => {}
                }
            }
        } else {
            for c in &n.children {
                visit(c, out);
            }
        }
    }
    let mut text = String::new();
    visit(p, &mut text);
    text
}
pub fn indexed(f: &FileRecord) -> Result<Text> {
    let file = File::open(&f.path).map_err(|e| err("ACCESS_DENIED", e))?;
    let meta = file.metadata().map_err(|e| err("ACCESS_DENIED", e))?;
    if f.status != "available"
        || fsops::revision(&meta) != f.revision
        || fsops::identity(&file) != f.identity
    {
        return Err(err("FILE_CHANGED", "文件已改变，请刷新"));
    }
    let ext = f.extension.to_lowercase();
    let delimited = matches!(ext.as_str(), "csv" | "tsv");
    let max = if delimited { 8 } else { 32 } * 1024 * 1024;
    if meta.len() > max {
        return Err(err(
            "INDEX_TOO_LARGE",
            "Office 源文件上限 32 MiB，CSV／TSV 上限 8 MiB",
        ));
    }
    let mut out = Extractor::new(
        if delimited { "文本表格" } else { "OOXML" },
        if ext == "docx" {
            "主文档段落与表格；不含页眉、页脚、批注、文本框和图片文字"
        } else {
            "单元格原始值与已保存公式结果；不计算公式、不索引公式表达式或图表"
        },
    );
    if delimited {
        let mut bytes = vec![];
        file.take(max + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| err("ACCESS_DENIED", e))?;
        if bytes.len() as u64 > max {
            return Err(limit());
        }
        let data = csv(&bytes, if ext == "tsv" { '\t' } else { ',' })?;
        out.sheet(&data, 0);
    } else {
        let mut z = zip::ZipArchive::new(file).map_err(|_| invalid())?;
        if z.len() > 5000 {
            return Err(limit());
        }
        let mut budget = 24 * 1024 * 1024;
        if ext == "docx" {
            let doc = entry(&mut z, "word/document.xml", &mut budget)?;
            let body = doc
                .child("document")
                .and_then(|d| d.child("body"))
                .ok_or_else(invalid)?;
            let mut block = 0;
            let mut para = 0;
            let mut table = 0;
            'blocks: for n in &body.children {
                if block >= 5000 {
                    out.text.truncated = true;
                    break;
                }
                match n.name.as_str() {
                    "p" => {
                        para += 1;
                        if !out.push(&plain(n),json!({"kind":"word","block":block,"paragraph":para,"label":format!("Word 第 {para} 段")})) {break;}
                        block += 1;
                    }
                    "tbl" => {
                        table += 1;
                        for (r, row) in n.children.iter().filter(|n| n.name == "tr").enumerate() {
                            if r >= 1000 {
                                out.text.truncated = true;
                                break;
                            }
                            for (c, cell) in
                                row.children.iter().filter(|n| n.name == "tc").enumerate()
                            {
                                if c >= COLS {
                                    out.text.truncated = true;
                                    break;
                                }
                                let mut ps = vec![];
                                cell.all("p", &mut ps);
                                for (p, node) in ps.iter().enumerate() {
                                    if !out.push(&plain(node),json!({"kind":"word","block":block,"tableRow":r,"tableColumn":c,"cellParagraph":p,"label":format!("Word 表格 {table} · 第 {} 行第 {} 列",r+1,c+1)})) {break 'blocks;}
                                }
                            }
                        }
                        block += 1;
                    }
                    "sectPr" => {}
                    _ => {
                        out.text.truncated = true;
                    }
                }
            }
        } else if ext == "xlsx" {
            let book = workbook(&mut z, &mut budget)?;
            for selected in 0..book.names.len() {
                match xlsx_sheet(&mut z, &mut budget, &book, selected) {
                    Ok(data) => {
                        if !out.sheet(&data, selected) {
                            break;
                        }
                    }
                    Err(e)
                        if e.starts_with("PREVIEW_TOO_LARGE") && !out.text.locations.is_empty() =>
                    {
                        out.text.truncated = true;
                        break;
                    }
                    Err(e) => return Err(e),
                }
            }
        } else {
            return Err(invalid());
        }
    }
    let actual = fsops::inspect(std::path::Path::new(&f.path))?;
    if actual.revision != f.revision || actual.identity != f.identity {
        return Err(err("FILE_CHANGED", "读取期间文件改变"));
    }
    if out.text.truncated {
        out.text.coverage.push_str("；已达提取范围，仅搜索部分内容");
    }
    Ok(out.text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db::Store, search};
    use std::io::Write;
    fn archive(path: &std::path::Path, parts: &[(&str, String)]) {
        let mut z = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, body) in parts {
            z.start_file(
                *name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
            z.write_all(body.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }
    fn doc(body: &str) -> String {
        format!("<w:document xmlns:w='urn:w'><w:body>{body}</w:body></w:document>")
    }
    fn p(value: &str) -> String {
        format!("<w:p><w:r><w:t>{value}</w:t></w:r></w:p>")
    }
    fn book() -> Vec<(&'static str, String)> {
        vec![("xl/workbook.xml","<workbook><sheets><sheet name='预算' id='r1'/><sheet name='明细' id='r2'/></sheets></workbook>".into()),
            ("xl/_rels/workbook.xml.rels","<Relationships><Relationship Id='r1' Target='worksheets/sheet1.xml'/><Relationship Id='r2' Target='worksheets/sheet2.xml'/></Relationships>".into()),
            ("xl/sharedStrings.xml","<sst><si><t>共享中文</t></si></sst>".into()),
            ("xl/worksheets/sheet1.xml","<worksheet><sheetData><row r='4'><c r='A4' t='s'><v>0</v></c><c r='B4'><f>SUM(B2:B3)</f><v>3600</v></c><c r='C4'><f>SecretFormula()</f></c></row></sheetData></worksheet>".into()),
            ("xl/worksheets/sheet2.xml","<worksheet><sheetData><row r='5000'><c r='B5000' t='inlineStr'><is><t>蜂鸟命中</t></is></c></row></sheetData></worksheet>".into())]
    }
    fn record(path: &std::path::Path) -> FileRecord {
        let dir = crate::fsops::TmpDir::new();
        let mut store = Store::open(dir.path()).unwrap();
        let meta = fsops::inspect(path).unwrap();
        store.add_file_with_ai(&meta, false).unwrap();
        let id: String = store
            .conn
            .query_row("SELECT id FROM files WHERE path=?", [&meta.path], |r| {
                r.get(0)
            })
            .unwrap();
        store.file_metadata(&id).unwrap()
    }
    fn drain(store: &Store) {
        while let Some(w) = store.search_next().unwrap() {
            store.search_commit(&w, search::read(&w)).unwrap();
        }
    }
    fn q(store: &Store, text: &str) -> Value {
        store
            .query(&Query {
                text: text.into(),
                search_mode: "content".into(),
                ..Default::default()
            })
            .unwrap()
    }
    #[test]
    fn office_full_body_and_word_table_locations() {
        let dir = crate::fsops::TmpDir::new();
        let path = dir.path().join("word.docx");
        archive(
            &path,
            &[(
                "word/document.xml",
                doc(&(p(&"开头".repeat(600))
                    + &p("尾部蜂鸟")
                    + "<w:tbl><w:tr><w:tc>"
                    + &p("表格命中")
                    + "</w:tc></w:tr></w:tbl>")),
            )],
        );
        let f = fsops::inspect(&path).unwrap();
        let text = indexed(&record(&path)).unwrap();
        assert!(text.body.contains("尾部蜂鸟"));
        assert_eq!(text.locations[1]["block"], 1);
        assert_eq!(text.locations[2]["tableRow"], 0);
        assert_eq!(text.locations[2]["tableColumn"], 0);
        assert!(!text.truncated);
        let mut store = Store::open(&dir.path().join("library")).unwrap();
        store.add_file_with_ai(&f, false).unwrap();
        store.search_control("enable").unwrap();
        drain(&store);
        let result = q(&store, "表格命中");
        assert_eq!(result["total"], 1);
        assert_eq!(result["files"][0]["contentMatch"]["location"]["block"], 2);
        assert!(result["files"][0]["contentMatch"]["snippet"]
            .as_str()
            .unwrap()
            .contains("表格命中"));
        assert_eq!(q(&store, "开头 蜂鸟")["total"], 1);
    }
    #[test]
    fn all_sheets_shared_strings_cached_values_and_coordinates() {
        let dir = crate::fsops::TmpDir::new();
        let path = dir.path().join("book.xlsx");
        archive(&path, &book());
        let f = fsops::inspect(&path).unwrap();
        let text = indexed(&record(&path)).unwrap();
        assert!(text.body.contains("共享中文"));
        assert!(text.body.contains("3600"));
        assert!(!text.body.contains("SecretFormula"));
        assert_eq!(text.locations.last().unwrap()["cell"], "B5000");
        assert_eq!(text.locations.last().unwrap()["sheet"], 1);
        let mut store = Store::open(&dir.path().join("library")).unwrap();
        store.add_file_with_ai(&f, false).unwrap();
        store.search_control("enable").unwrap();
        drain(&store);
        let result = q(&store, "蜂鸟");
        assert_eq!(result["total"], 1);
        assert_eq!(result["files"][0]["contentMatch"]["source"], "明细 · B5000");
        assert_eq!(q(&store, "共享 3600")["total"], 1);
        assert_eq!(q(&store, "SecretFormula")["total"], 0);
    }
    #[test]
    fn shared_resources_loaded_once_and_non_body_word_text_excluded() {
        let dir = crate::fsops::TmpDir::new();
        let path = dir.path().join("many.xlsx");
        let mut parts = book();
        parts[2].1 = format!(
            "<sst><si><t>共享中文</t></si><si><t>{}</t></si></sst>",
            "x".repeat(3 * 1024 * 1024)
        );
        parts[0].1 = format!(
            "<workbook><sheets>{}</sheets></workbook>",
            (1..=10)
                .map(|i| format!("<sheet name='表{i}' id='r{i}'/>"))
                .collect::<String>()
        );
        parts[1].1 = format!(
            "<Relationships>{}</Relationships>",
            (1..=10)
                .map(|i| format!("<Relationship Id='r{i}' Target='worksheets/sheet1.xml'/>"))
                .collect::<String>()
        );
        archive(&path, &parts);
        let text = indexed(&record(&path)).unwrap();
        assert!(!text.truncated);
        assert_eq!(
            text.locations.iter().filter(|l| l["cell"] == "A4").count(),
            10
        );
        let path = dir.path().join("body.docx");
        archive(
            &path,
            &[(
                "word/document.xml",
                doc(&(p("可见正文")
                    + "<w:p><w:r><w:drawing><w:txbxContent>"
                    + &p("文本框不索引")
                    + "</w:txbxContent></w:drawing></w:r></w:p>")),
            )],
        );
        let text = indexed(&record(&path)).unwrap();
        assert!(text.body.contains("可见正文"));
        assert!(!text.body.contains("文本框不索引"));
    }
    #[test]
    fn csv_tsv_quotes_short_terms_and_encodings() {
        let dir = crate::fsops::TmpDir::new();
        for (name, bytes) in [
            (
                "csv.csv",
                b"name,note\nhello,\"quoted, line\nnext\"\n".to_vec(),
            ),
            (
                "utf16.tsv",
                [
                    vec![255, 254],
                    "名称\t蜂鸟\n"
                        .encode_utf16()
                        .flat_map(u16::to_le_bytes)
                        .collect(),
                ]
                .concat(),
            ),
            (
                "gb.csv",
                encoding_rs::GB18030.encode("名称,蜂鸟\n").0.into_owned(),
            ),
        ] {
            let path = dir.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            let t = indexed(&record(&path)).unwrap();
            assert!(!t.locations.is_empty());
            assert!(!t.truncated);
            if name == "csv.csv" {
                assert_eq!(t.locations.last().unwrap()["cell"], "B2");
                assert!(t.body.contains("quoted, line\nnext"));
            } else {
                assert!(t.body.contains("蜂鸟"));
            }
        }
        let bad = dir.path().join("bad.tsv");
        std::fs::write(&bad, [255, 254, 0]).unwrap();
        assert!(indexed(&record(&bad)).is_err());
    }
    #[test]
    fn bounded_unicode_text_locations_and_partial_rows() {
        let mut out = Extractor::new("OOXML", "");
        assert!(!out.push(&"蜂鸟".repeat(READ_LIMIT), json!({"label":"large"})));
        assert!(out.text.truncated);
        assert!(out.text.body.len() <= READ_LIMIT);
        assert!(out.text.body.ends_with('\n'));
        let dir = crate::fsops::TmpDir::new();
        let path = dir.path().join("rows.csv");
        std::fs::write(&path, "蜂鸟\n".repeat(10001)).unwrap();
        let text = indexed(&record(&path)).unwrap();
        assert!(text.truncated);
        assert_eq!(text.locations.len(), 10000);
    }
    #[test]
    fn external_zip_traversal_corruption_and_changed_source_rejected() {
        let dir = crate::fsops::TmpDir::new();
        let path = dir.path().join("bad.xlsx");
        for target in ["../../secret.xml", "https://example.com/data.xml"] {
            let mut parts = book();
            parts[1].1 = parts[1].1.replace("worksheets/sheet2.xml", target);
            archive(&path, &parts);
            assert!(indexed(&record(&path)).is_err());
        }
        let mut parts = book();
        parts[1].1 = parts[1]
            .1
            .replace("Id='r2'", "Id='r2' TargetMode='External'");
        archive(&path, &parts);
        assert!(indexed(&record(&path)).is_err());
        std::fs::write(&path, b"broken").unwrap();
        assert!(indexed(&record(&path)).is_err());
        archive(&path, &book());
        let f = record(&path);
        std::fs::write(&path, b"changed").unwrap();
        assert!(indexed(&f).err().unwrap().starts_with("FILE_CHANGED"));
    }
    #[test]
    fn old_derived_schema_migrates_and_requeues_without_changing_annotations() {
        let dir = crate::fsops::TmpDir::new();
        let root = dir.path().join("library");
        let path = dir.path().join("old.md");
        std::fs::write(&path, "旧版正文").unwrap();
        let mut store = Store::open(&root).unwrap();
        let f = fsops::inspect(&path).unwrap();
        store.add_file_with_ai(&f, false).unwrap();
        store.search_control("enable").unwrap();
        drain(&store);
        store.conn.execute_batch("UPDATE search.docs SET parser=1; ALTER TABLE search.docs DROP COLUMN locations; ALTER TABLE search.docs DROP COLUMN coverage;").unwrap();
        drop(store);
        let store = Store::open(&root).unwrap();
        assert_eq!(q(&store, "旧版")["total"], 0);
        assert_eq!(store.search_status().unwrap()["counts"]["stale"], 1);
        drain(&store);
        assert_eq!(q(&store, "旧版")["total"], 1);
        assert_eq!(store.search_status().unwrap()["counts"]["done"], 1);
    }
}
