//! Offline, reviewable rule interpretation; never writes files or invokes a model.
use crate::{db::Store, model::*};
use serde_json::{json, Value};
impl Store {
    pub fn natural_preview(&self, v: &Value) -> Result<Value> {
        let text = str_arg(v, "text")?.trim();
        if text.is_empty() || text.chars().count() > 256 {
            return Err(err("FILTER_TEXT", "请输入 1～256 字符的筛选描述"));
        }
        let anchor = v["anchor"].as_i64().unwrap_or_else(now);
        if (now() - anchor).abs() > 300_000 {
            return Err(err("FILTER_EXPIRED", "筛选预览已过期，请重新解析"));
        }
        let mut outside = String::new();
        let mut close = None;
        for ch in text.chars() {
            if let Some(end) = close {
                if ch == end {
                    close = None;
                }
                outside.push(' ');
            } else {
                match ch {
                    '「' => close = Some('」'),
                    '“' => close = Some('”'),
                    '"' => close = Some('"'),
                    _ => outside.push(ch),
                }
            }
        }
        let mut q = Query {
            scope: "all".into(),
            search_mode: "metadata".into(),
            mode: "all".into(),
            sort: "added".into(),
            direction: "desc".into(),
            limit: 100,
            ..Default::default()
        };
        let mut rules = vec![];
        if outside.contains("不是")
            || outside.contains("非图片")
            || outside.contains("不要PDF")
            || outside.contains("不含PDF")
        {
            return Err(err(
                "FILTER_UNSUPPORTED",
                "类型的否定描述暂不支持，请使用工作台筛选；标签排除请写 排除「标签名」",
            ));
        }
        if outside.contains("收藏") {
            q.scope = "starred".into();
            rules.push("我的收藏".to_string());
        } else if outside.contains("待整理") {
            q.scope = "inbox".into();
            rules.push("待整理".into());
        } else if outside.contains("需要关注") || outside.contains("丢失") {
            q.scope = "unavailable".into();
            rules.push("需要关注".into());
        }
        let lower = outside.to_lowercase();
        for (words, kind, label) in [
            (vec!["图片", "照片"], "image", "图片"),
            (vec!["pdf"], "pdf", "PDF"),
            (
                vec!["office", "word", "excel", "ppt", "docx", "xlsx"],
                "office",
                "Office",
            ),
            (vec!["markdown", "文本", "html"], "text", "文本 / HTML"),
        ] {
            if words.iter().any(|w| lower.contains(w)) {
                q.kinds.push(kind.into());
                rules.push(format!("类型：{label}"));
            }
        }
        let mut days = None;
        for (w, n) in [
            ("最近一周", 7),
            ("最近一个月", 30),
            ("最近7天", 7),
            ("最近30天", 30),
            ("今天", 1),
        ] {
            if outside.contains(w) {
                days = Some(n);
                break;
            }
        }
        if let Some(n) = days {
            q.from = Some(anchor - n * 86_400_000);
            q.to = Some(anchor + 1);
            rules.push(format!("修改时间：过去 {n} 天（以本次解析时间为准）"));
        }
        let tags = self.tag_pool()?;
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let closing = match chars[i] {
                '「' => '」',
                '“' => '”',
                '"' => '"',
                _ => {
                    i += 1;
                    continue;
                }
            };
            let begin = i + 1;
            let Some(end) = (begin..chars.len()).find(|j| chars[*j] == closing) else {
                return Err(err("FILTER_TEXT", "标签名称的引号未闭合"));
            };
            let name: String = chars[begin..end].iter().collect();
            let prefix: String = chars[..i]
                .iter()
                .rev()
                .take(4)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let exclude =
                prefix.contains("排除") || prefix.contains("不含") || prefix.contains("没有");
            let matches: Vec<_> = tags
                .iter()
                .filter(|t| crate::tag_aliases::matches(t, &name))
                .collect();
            if matches.len() != 1 {
                return Err(err(
                    "FILTER_TAG",
                    format!("「{name}」不存在或名称有歧义，请使用完整名称"),
                ));
            }
            let tag = matches[0];
            let target = if exclude {
                &mut q.exclude
            } else {
                &mut q.include
            };
            if !target.contains(&tag.id) {
                target.push(tag.id.clone());
                rules.push(format!(
                    "{}标签：{}",
                    if exclude { "排除" } else { "包含" },
                    tag.name
                ));
            }
            i = end + 1;
        }
        if q.include.iter().any(|id| q.exclude.contains(id)) {
            return Err(err(
                "FILTER_CONFLICT",
                "同一标签不能同时包含和排除，请修改描述",
            ));
        }
        if rules.is_empty() {
            return Err(err(
                "FILTER_UNSUPPORTED",
                "未识别条件。示例：最近7天的 PDF，包含「参考素材」，排除「已完成」",
            ));
        }
        Ok(
            json!({"query":q,"rules":rules,"anchor":anchor,"warnings":["仅应用下列条件；未列出的描述不会被解释。多个类型匹配任一类型，包含标签需全部满足。日期按修改时间筛选。应用后将替换当前筛选。"]}),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn review_alias_dates() {
        let d = tempfile::tempdir().unwrap();
        let mut s = Store::open(d.path()).unwrap();
        let t = s.create_tag("参考素材").unwrap();
        let r = s
            .natural_preview(&json!({"text":"最近7天的 PDF，包含「参考素材」"}))
            .unwrap();
        assert_eq!(r["query"]["include"][0], t.id);
        assert_eq!(r["query"]["kinds"][0], "pdf");
        assert!(r["query"]["from"].is_number());
        assert!(!r["warnings"].as_array().unwrap().is_empty());
    }
    #[test]
    fn unknown_and_stale_rejected() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::open(d.path()).unwrap();
        assert!(s.natural_preview(&json!({"text":"漂亮的东西"})).is_err());
        assert!(s.natural_preview(&json!({"text":"不是图片"})).is_err());
        assert!(s
            .natural_preview(&json!({"text":"PDF","anchor":1}))
            .is_err());
        assert!(s
            .natural_preview(&json!({"text":"PDF 包含「不存在」"}))
            .is_err());
    }
}
