//! PowerPoint content reader: presentation order, paragraphs and bounded raster media.
use super::*;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use std::io::Cursor;
fn path(base: &str, target: &str, prefix: &str) -> Result<String> {
    if target.contains(['\\', ':']) {
        return Err(invalid());
    }
    let mut parts: Vec<&str> = if target.starts_with('/') {
        vec![]
    } else {
        base.split('/').filter(|p| !p.is_empty()).collect()
    };
    for p in target.split('/') {
        match p {
            "" | "." => (),
            ".." => {
                parts.pop().ok_or_else(invalid)?;
            }
            _ => parts.push(p),
        }
    }
    let out = parts.join("/");
    if !out.starts_with(prefix) {
        return Err(invalid());
    }
    Ok(out)
}
pub(super) fn document(z: &mut zip::ZipArchive<File>, budget: &mut usize) -> Result<Value> {
    let book = entry(z, "ppt/presentation.xml", budget)?;
    let rels = entry(z, "ppt/_rels/presentation.xml.rels", budget)?;
    let mut links = vec![];
    rels.all("Relationship", &mut links);
    let mut ids = vec![];
    book.all("sldId", &mut ids);
    if ids.len() > 2000 {
        return Err(limit());
    }
    let total = ids.len();
    let mut slides = vec![];
    let mut images: HashMap<String, String> = HashMap::new();
    let mut pixels = 0u64;
    let mut encoded = 0usize;
    for (number, id) in ids.into_iter().take(200).enumerate() {
        let rid = if id.attr("r:id").is_empty() {
            id.attr("id")
        } else {
            id.attr("r:id")
        };
        let relation = links
            .iter()
            .find(|r| r.attr("Id") == rid && r.attr("TargetMode") != "External")
            .ok_or_else(invalid)?;
        let slide_path = path("ppt", relation.attr("Target"), "ppt/slides/")?;
        let tree = entry(z, &slide_path, budget)?;
        let mut ps = vec![];
        tree.all("p", &mut ps);
        let paragraphs: Vec<String> = ps
            .into_iter()
            .map(|p| p.texts("t"))
            .filter(|t| !t.is_empty())
            .collect();
        let name = slide_path.rsplit('/').next().ok_or_else(invalid)?;
        let rel_path = format!("ppt/slides/_rels/{name}.rels");
        let mut pictures = vec![];
        let mut omitted = 0;
        if z.file_names().any(|n| n == rel_path) {
            let rels = entry(z, &rel_path, budget)?;
            let mut media = vec![];
            rels.all("Relationship", &mut media);
            let mut refs = vec![];
            tree.all("blip", &mut refs);
            for r in refs.into_iter().take(100) {
                let result = (|| -> Result<String> {
                    let link = media
                        .iter()
                        .find(|m| {
                            m.attr("Id") == r.attr("embed")
                                && m.attr("TargetMode") != "External"
                                && m.attr("Type").ends_with("/image")
                        })
                        .ok_or_else(invalid)?;
                    let p = path("ppt/slides", link.attr("Target"), "ppt/media/")?;
                    if images.contains_key(&p) {
                        return Ok(p);
                    }
                    let item = z.by_name(&p).map_err(|_| invalid())?;
                    if item.size() > 4 * 1024 * 1024 || item.size() > *budget as u64 {
                        return Err(limit());
                    }
                    let mut bytes = vec![];
                    item.take(4 * 1024 * 1024 + 1)
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
                    let count = w as u64 * h as u64;
                    if w == 0 || h == 0 || count > 8_000_000 || pixels + count > 24_000_000 {
                        return Err(limit());
                    }
                    pixels += count;
                    let mut reader = image::ImageReader::new(Cursor::new(bytes))
                        .with_guessed_format()
                        .map_err(|_| invalid())?;
                    let mut limits = image::Limits::default();
                    limits.max_alloc = Some(64 * 1024 * 1024);
                    reader.limits(limits);
                    let image = reader
                        .decode()
                        .map_err(|_| invalid())?
                        .thumbnail(1280, 1280);
                    let mut png = Cursor::new(vec![]);
                    image
                        .write_to(&mut png, image::ImageFormat::Png)
                        .map_err(|_| invalid())?;
                    let bytes = png.into_inner();
                    if encoded + bytes.len() > 4 * 1024 * 1024 {
                        return Err(limit());
                    }
                    encoded += bytes.len();
                    let data = B64.encode(bytes);
                    images.insert(p.clone(), data);
                    Ok(p)
                })();
                match result {
                    Ok(data) => pictures.push(data),
                    Err(_) => omitted += 1,
                }
            }
        }
        slides.push(json!({"number":number+1,"paragraphs":paragraphs,"images":pictures,"omittedImages":omitted}));
    }
    Ok(json!({"kind":"pptx","slides":slides,"images":images,"total":total,"truncated":total>200}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn media_paths_stay_inside_package_and_slide_order_is_not_lexical() {
        assert_eq!(
            path("ppt/slides", "../media/photo.png", "ppt/media/").unwrap(),
            "ppt/media/photo.png"
        );
        for s in [
            "../../../../file.png",
            "https://example.com/image.png",
            "..\\media\\file.png",
            "/outside/file.png",
        ] {
            assert!(path("ppt/slides", s, "ppt/media/").is_err());
        }
        let (_,mut z)=super::super::tests::archive(&[
            ("ppt/presentation.xml","<p:presentation xmlns:p='p' xmlns:r='r'><p:sldIdLst><p:sldId r:id='r2' id='99'/><p:sldId id='100' r:id='r1'/></p:sldIdLst></p:presentation>"),
            ("ppt/_rels/presentation.xml.rels","<Relationships><Relationship Id='r1' Target='slides/slide1.xml'/><Relationship Id='r2' Target='slides/slide2.xml'/></Relationships>"),
            ("ppt/slides/slide1.xml","<sld><p><r><t>第二页</t></r></p></sld>"),
            ("ppt/slides/slide2.xml","<sld><p><r><t>第一页</t></r></p></sld>"),
        ]);
        let value = document(&mut z, &mut (24 * 1024 * 1024)).unwrap();
        assert_eq!(value["slides"][0]["paragraphs"][0], "第一页");
        assert_eq!(value["slides"][1]["paragraphs"][0], "第二页");
    }
}
