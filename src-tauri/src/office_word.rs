//! Word relationships, bounded embedded raster images and common list numbering.
use super::*;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use std::io::Cursor;

fn optional(z: &mut zip::ZipArchive<File>, path: &str, budget: &mut usize) -> Result<Node> {
    if z.file_names().any(|n| n == path) {
        entry(z, path, budget)
    } else {
        Ok(Node::default())
    }
}
fn val(n: &Node, key: &str) -> String {
    n.child(key)
        .map(|n| n.attr("val").to_owned())
        .unwrap_or_default()
}
#[derive(Clone)]
struct Level {
    start: u32,
    format: String,
    text: String,
}
pub(super) struct Context {
    links: HashMap<String, String>,
    levels: HashMap<(String, usize), Level>,
    styles: HashMap<String, (String, Option<String>, Option<usize>)>,
    counters: HashMap<String, [Option<u32>; 9]>,
    image_refs: HashMap<String, Value>,
    pixels: u64,
    encoded: usize,
    pub images: HashMap<String, Value>,
    pub warnings: Vec<String>,
}
impl Context {
    pub fn load(z: &mut zip::ZipArchive<File>, budget: &mut usize) -> Result<Self> {
        let rels = optional(z, "word/_rels/document.xml.rels", budget)?;
        let mut nodes = vec![];
        rels.all("Relationship", &mut nodes);
        if nodes.len() > 5000 {
            return Err(limit());
        }
        let links = nodes
            .iter()
            .filter(|r| r.attr("TargetMode") != "External" && r.attr("Type").ends_with("/image"))
            .map(|r| (r.attr("Id").to_owned(), r.attr("Target").to_owned()))
            .collect();
        let numbering = optional(z, "word/numbering.xml", budget)?;
        let mut abstracts = vec![];
        numbering.all("abstractNum", &mut abstracts);
        let mut nums = vec![];
        numbering.all("num", &mut nums);
        if abstracts.len() > 1000 || nums.len() > 1000 {
            return Err(limit());
        }
        let mut levels = HashMap::new();
        for num in nums {
            if let Some(a) = abstracts
                .iter()
                .find(|a| a.attr("abstractNumId") == val(num, "abstractNumId"))
            {
                for l in a.children.iter().filter(|l| l.name == "lvl") {
                    let Some(i) = l.attr("ilvl").parse::<usize>().ok().filter(|i| *i < 9) else {
                        continue;
                    };
                    let override_node = num
                        .children
                        .iter()
                        .find(|n| n.name == "lvlOverride" && n.attr("ilvl") == l.attr("ilvl"));
                    let effective = override_node.and_then(|n| n.child("lvl")).unwrap_or(l);
                    let start = override_node
                        .map(|n| val(n, "startOverride"))
                        .unwrap_or_default()
                        .parse()
                        .ok()
                        .or_else(|| val(effective, "start").parse().ok())
                        .unwrap_or(1);
                    if levels.len() >= 9000 {
                        return Err(limit());
                    }
                    levels.insert(
                        (num.attr("numId").to_owned(), i),
                        Level {
                            start,
                            format: val(effective, "numFmt"),
                            text: val(effective, "lvlText").chars().take(128).collect(),
                        },
                    );
                }
            }
        }
        let tree = optional(z, "word/styles.xml", budget)?;
        let mut nodes = vec![];
        tree.all("style", &mut nodes);
        if nodes.len() > 5000 {
            return Err(limit());
        }
        let styles = nodes
            .into_iter()
            .map(|s| {
                let num = s.child("pPr").and_then(|p| p.child("numPr"));
                (
                    s.attr("styleId").to_owned(),
                    (
                        val(s, "basedOn"),
                        num.and_then(|n| n.child("numId"))
                            .map(|n| n.attr("val").to_owned()),
                        num.and_then(|n| n.child("ilvl"))
                            .and_then(|n| n.attr("val").parse().ok()),
                    ),
                )
            })
            .collect();
        Ok(Self {
            links,
            levels,
            styles,
            counters: HashMap::new(),
            image_refs: HashMap::new(),
            pixels: 0,
            encoded: 0,
            images: HashMap::new(),
            warnings: vec![],
        })
    }
    fn warn(&mut self, text: &str) {
        if !self.warnings.iter().any(|s| s == text) {
            self.warnings.push(text.to_owned());
        }
    }
    fn image(&mut self, id: &str, z: &mut zip::ZipArchive<File>, budget: &mut usize) -> Value {
        if let Some(value) = self.image_refs.get(id) {
            return value.clone();
        }
        let result = (|| -> Result<Value> {
            if self.images.len() >= 20 {
                return Err(limit());
            }
            let target = self.links.get(id).ok_or_else(invalid)?;
            let path = if let Some(p) = target.strip_prefix("/word/") {
                format!("word/{p}")
            } else {
                format!("word/{target}")
            };
            if !path.starts_with("word/media/")
                || path.contains("..")
                || path.contains('\\')
                || path.contains(':')
            {
                return Err(invalid());
            }
            let f = z.by_name(&path).map_err(|_| invalid())?;
            if f.size() > 4 * 1024 * 1024 || f.size() > *budget as u64 {
                return Err(limit());
            }
            let mut bytes = vec![];
            f.take(4 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| invalid())?;
            if bytes.len() > 4 * 1024 * 1024 || bytes.len() > *budget {
                return Err(limit());
            }
            *budget -= bytes.len();
            let reader = image::ImageReader::new(Cursor::new(&bytes))
                .with_guessed_format()
                .map_err(|_| invalid())?;
            let (w, h) = reader.into_dimensions().map_err(|_| invalid())?;
            let pixels = w as u64 * h as u64;
            if pixels > 8_000_000 || self.pixels + pixels > 24_000_000 || w == 0 || h == 0 {
                return Err(limit());
            }
            self.pixels += pixels;
            let mut reader = image::ImageReader::new(Cursor::new(bytes))
                .with_guessed_format()
                .map_err(|_| invalid())?;
            let mut limits = image::Limits::default();
            limits.max_alloc = Some(64 * 1024 * 1024);
            reader.limits(limits);
            let image = reader.decode().map_err(|_| invalid())?;
            let image = if w > 1280 || h > 1280 {
                image.thumbnail(1280, 1280)
            } else {
                image
            };
            let (width, height) = (image.width(), image.height());
            let mut out = Cursor::new(vec![]);
            image
                .write_to(&mut out, image::ImageFormat::Png)
                .map_err(|_| invalid())?;
            let data = out.into_inner();
            if self.encoded + data.len() > 8 * 1024 * 1024 {
                return Err(limit());
            }
            self.encoded += data.len();
            let key = format!("image{}", self.images.len());
            self.images.insert(
                key.clone(),
                json!({"data":B64.encode(data),"width":width,"height":height}),
            );
            Ok(json!({"image":key}))
        })();
        let value = result.unwrap_or_else(|_| {
            self.warn("部分图片因格式、外部引用或资源预算未显示");
            json!({"imageError":"图片未显示"})
        });
        self.image_refs.insert(id.to_owned(), value.clone());
        value
    }
    pub fn paragraph(
        &mut self,
        n: &Node,
        z: &mut zip::ZipArchive<File>,
        budget: &mut usize,
    ) -> Value {
        let mut p = super::paragraph(n);
        let mut nodes = vec![];
        n.all("r", &mut nodes);
        let mut runs = vec![];
        for r in nodes {
            // Keep text and drawings in their document order within each run.
            for c in &r.children {
                if matches!(c.name.as_str(), "t" | "tab" | "br" | "cr") {
                    let mut single = Node {
                        name: "p".into(),
                        ..Default::default()
                    };
                    let mut run = Node {
                        name: "r".into(),
                        ..Default::default()
                    };
                    let text = match c.name.as_str() {
                        "tab" => "\t",
                        "br" | "cr" => "\n",
                        _ => &c.text,
                    };
                    run.children.push(Node {
                        name: "t".into(),
                        text: text.into(),
                        ..Default::default()
                    });
                    if let Some(pr) = r.child("rPr") {
                        for key in ["b", "i"] {
                            if let Some(prop) = pr.child(key) {
                                let rp = run.children.iter().position(|n| n.name == "rPr");
                                let idx = rp.unwrap_or_else(|| {
                                    run.children.push(Node {
                                        name: "rPr".into(),
                                        ..Default::default()
                                    });
                                    run.children.len() - 1
                                });
                                run.children[idx].children.push(Node {
                                    name: key.into(),
                                    attrs: prop.attrs.clone(),
                                    ..Default::default()
                                });
                            }
                        }
                    }
                    single.children.push(run);
                    runs.push(super::paragraph(&single)["runs"][0].clone());
                } else {
                    let mut drawings = vec![];
                    c.all("blip", &mut drawings);
                    for drawing in drawings {
                        runs.push(self.image(drawing.attr("embed"), z, budget));
                    }
                }
            }
        }
        p["runs"] = json!(runs);
        let pr = n.child("pPr");
        let direct = pr.and_then(|p| p.child("numPr"));
        let mut num = direct
            .and_then(|n| n.child("numId"))
            .map(|n| n.attr("val").to_owned());
        let mut level = direct
            .and_then(|n| n.child("ilvl"))
            .and_then(|n| n.attr("val").parse::<usize>().ok());
        let mut style = pr.map(|p| val(p, "pStyle")).unwrap_or_default();
        for _ in 0..16 {
            let Some((base, id, i)) = self.styles.get(&style) else {
                break;
            };
            if num.is_none() {
                num = id.clone();
            }
            if level.is_none() {
                level = *i;
            }
            style = base.clone();
        }
        if let Some(id) = num.filter(|id| id != "0") {
            let i = level.unwrap_or(0).min(8);
            if let Some(l) = self.levels.get(&(id.clone(), i)).cloned() {
                let counts = self.counters.entry(id.clone()).or_insert([None; 9]);
                counts[i] = Some(counts[i].map(|n| n.saturating_add(1)).unwrap_or(l.start));
                for n in &mut counts[i + 1..] {
                    *n = None;
                }
                let mut label = l.text.clone();
                if l.format == "bullet" {
                    label = "•".into();
                } else {
                    for j in 0..9 {
                        if label.contains(&format!("%{}", j + 1)) {
                            let other = self.levels.get(&(id.clone(), j));
                            let value =
                                counts[j].unwrap_or_else(|| other.map(|n| n.start).unwrap_or(1));
                            let format = other.map(|n| n.format.as_str()).unwrap_or("decimal");
                            if !matches!(
                                format,
                                "decimal"
                                    | "lowerLetter"
                                    | "upperLetter"
                                    | "lowerRoman"
                                    | "upperRoman"
                            ) {
                                self.warnings.push("部分列表编号使用数字近似显示".into());
                            }
                            label = label.replace(&format!("%{}", j + 1), &number(value, format));
                        }
                    }
                }
                p["label"] = json!(label);
                p["level"] = json!(i);
            } else {
                self.warn("部分列表编号定义不受支持");
            }
        }
        p
    }
}
fn number(mut n: u32, format: &str) -> String {
    if matches!(format, "lowerLetter" | "upperLetter") && n > 0 {
        let mut out = String::new();
        while n > 0 {
            n -= 1;
            out.insert(0, (b'A' + (n % 26) as u8) as char);
            n /= 26;
        }
        return if format == "lowerLetter" {
            out.to_lowercase()
        } else {
            out
        };
    }
    if matches!(format, "lowerRoman" | "upperRoman") && n > 0 && n < 4000 {
        let mut out = String::new();
        for (value, s) in [
            (1000, "M"),
            (900, "CM"),
            (500, "D"),
            (400, "CD"),
            (100, "C"),
            (90, "XC"),
            (50, "L"),
            (40, "XL"),
            (10, "X"),
            (9, "IX"),
            (5, "V"),
            (4, "IV"),
            (1, "I"),
        ] {
            while n >= value {
                out.push_str(s);
                n -= value;
            }
        }
        return if format == "lowerRoman" {
            out.to_lowercase()
        } else {
            out
        };
    }
    n.to_string()
}
