//! Query-bound evidence; never infer a visual match from a picture's appearance.
use crate::model::{key, FileRecord, Query};
use serde_json::{json, Value};

pub fn explanation(file: &FileRecord, query: &Query, terms: &[String]) -> Option<Value> {
    if query.text.trim().is_empty() || terms.is_empty() {
        return None;
    }
    let mut sources = vec![];
    let exact = key(query.text.trim());
    if query.search_mode != "content" && query.search_scope != "body" {
        let name = key(&file.name);
        if terms.iter().any(|t| name.contains(t)) {
            let stem = name
                .strip_suffix(&format!(".{}", key(&file.extension)))
                .unwrap_or(&name);
            sources.push(
                json!({"kind":"filename","value":file.name,"exact":name==exact || stem==exact}),
            );
        }
        if query.search_scope != "name" {
            if file
                .web_source
                .as_ref()
                .is_some_and(|s| terms.iter().any(|t| key(&s.text()).contains(t)))
            {
                sources.push(json!({"kind":"web"}));
            }
            for tag in &file.tags {
                if terms.iter().any(|t| key(&tag.name).contains(t)) {
                    sources
                        .push(json!({"kind":"tag","value":tag.name,"exact":key(&tag.name)==exact}));
                }
                for alias in &tag.aliases {
                    if terms.iter().any(|t| key(alias).contains(t)) {
                        sources
                            .push(json!({"kind":"alias","value":alias,"exact":key(alias)==exact}));
                    }
                }
            }
            if terms.iter().any(|t| key(&file.note).contains(t)) {
                sources.push(json!({"kind":"note"}));
            }
        }
    }
    if let Some(body) = &file.content_match {
        if body["semantic"].as_bool() != Some(true) {
            sources.push(json!({"kind":"body","value":body["source"]}));
        }
    }
    if sources.is_empty() {
        return None;
    }
    Some(json!({"queryText":query.text,"sources":sources}))
}
