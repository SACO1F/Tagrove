//! Shared browser capture options; no folder tags or unconfirmed AI suggestions.
use crate::{db::Store, model::*};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
impl Store {
    pub fn capture_tag_ids(&self, value: &Value) -> Result<Vec<String>> {
        let Some(raw) = value.get("tagIds") else {
            return Ok(vec![]);
        };
        let list = raw
            .as_array()
            .ok_or_else(|| err("CAPTURE_INVALID", "标签选择格式无效"))?;
        if list.len() > 50 {
            return Err(err("CAPTURE_INVALID", "每次最多选择 50 个标签"));
        }
        let pool = self.tag_pool()?;
        let mut ids = vec![];
        for v in list {
            let id = v
                .as_str()
                .ok_or_else(|| err("CAPTURE_INVALID", "标签标识无效"))?;
            if !pool.iter().any(|t| t.id == id && t.created_by != "folder") {
                return Err(err(
                    "CAPTURE_TAG_CHANGED",
                    "所选标签已删除或不再可用，请在采集队列重新编辑标签",
                ));
            }
            ids.push(id.to_owned());
        }
        ids.sort();
        ids.dedup();
        Ok(ids)
    }
    pub fn browser_tag_catalog(&self, query: &str) -> Result<Value> {
        if query.chars().count() > 80 {
            return Err(err("CAPTURE_INVALID", "标签搜索过长"));
        }
        let query = key(query);
        let tags: Vec<_> = self
            .tag_pool()?
            .into_iter()
            .filter(|t| {
                t.created_by != "folder"
                    && (query.is_empty()
                        || key(&t.name).contains(&query)
                        || t.aliases.iter().any(|a| key(a).contains(&query)))
            })
            .collect();
        Ok(
            json!({"tags":tags.iter().take(500).map(|t|json!({"id":t.id,"name":t.name})).collect::<Vec<_>>(),"truncated":tags.len()>500}),
        )
    }
}
pub fn apply(conn: &Connection, fid: &str, tags: &[String], queue_ai: bool) -> Result<()> {
    conn.execute_batch("SAVEPOINT capture_labels")
        .map_err(sql_err)?;
    let result = (|| {
        let mut changed = 0;
        for tid in tags {
            changed+=conn.execute("INSERT INTO file_tags(file_id,tag_id,source,ai_meta) VALUES(?,?,'manual','') ON CONFLICT(file_id,tag_id) DO UPDATE SET source='manual',ai_meta='' WHERE file_tags.source<>'manual' OR file_tags.ai_meta<>''",params![fid,tid]).map_err(sql_err)?;
        }
        if changed > 0 {
            conn.execute("UPDATE files SET version=version+1 WHERE id=?", [fid])
                .map_err(sql_err)?;
        }
        if !tags.is_empty() {
            conn.execute("UPDATE ai_jobs SET status='cancelled',error='',updated_at=? WHERE file_id=? AND status IN ('queued','running')",params![now(),fid]).map_err(sql_err)?;
        } else if queue_ai {
            conn.execute(
                "INSERT OR IGNORE INTO ai_jobs(file_id,status,updated_at) VALUES(?,'queued',?)",
                params![fid, now()],
            )
            .map_err(sql_err)?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => conn
            .execute_batch("RELEASE capture_labels")
            .map_err(sql_err),
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK TO capture_labels;RELEASE capture_labels");
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai;
    fn request()->Value{json!({"requestId":id(),"kind":"web_page","title":"正则学习资料","url":"https://example.test/tutorial#testing","excerpt":"","pageText":"程序设计课程与正则表达式练习"})}
    #[test]
    fn browser_options_queue_ai_preserve_manual_tags_and_retries(){
        let dir=tempfile::tempdir().unwrap();let mut s=Store::open(dir.path()).unwrap();
        s.conn.execute("INSERT INTO settings VALUES('ai',?)",[json!({"enabled":true,"endpoint":"http://localhost:11434/v1","model":"fixture","allowNewTags":true}).to_string()]).unwrap();
        let manual=s.create_tag("学习").unwrap();let mut r=request();r["tagIds"]=json!([manual.id]);
        let result=s.capture(&r).unwrap();let fid=result["id"].as_str().unwrap();let f=s.file(fid).unwrap();assert_eq!(f.tags[0].source,"manual");assert_eq!(f.web_source.as_ref().unwrap().reading.as_ref().unwrap().text,"程序设计课程与正则表达式练习");
        assert!(ai::content(dir.path(),&f).unwrap()["text"].as_str().unwrap().contains("正则表达式练习"));
        assert_eq!(result["aiQueued"],false);assert!(s.ai_task(fid).unwrap().is_none());
        s.queue_ai(&[fid.into()]).unwrap();let (job,token)=s.take_ai_job().unwrap().unwrap();
        s.apply_ai(&job,token,"fixture",&[(Some(manual.id.clone()),manual.name.clone(),"学习".into()),(None,"程序设计".into(),"课程内容".into())]).unwrap();
        let after=s.file(fid).unwrap();assert_eq!(after.tags.iter().find(|t|t.id==manual.id).unwrap().source,"manual");assert!(!s.tag_pool().unwrap().iter().any(|t|t.name=="程序设计"));
        let version=after.version;assert_eq!(s.capture(&r).unwrap()["duplicate"],true);assert_eq!(s.file(fid).unwrap().version,version);assert_eq!(s.ai_task(fid).unwrap().unwrap().status,"done");
        let other=s.create_tag("教程").unwrap();let mut duplicate=request();duplicate["tagIds"]=json!([other.id]);assert_eq!(s.capture(&duplicate).unwrap()["id"],fid);assert!(s.file(fid).unwrap().tags.iter().any(|t|t.id==other.id));assert_eq!(s.ai_task(fid).unwrap().unwrap().updated_at,token);
        r["tagIds"]=json!([other.id]);assert!(s.capture(&r).unwrap_err().starts_with("CAPTURE_CONFLICT"));
    }
    #[test]
    fn browser_catalog_excludes_folders_and_pending_ai_and_disabled_ai_stays_off(){
        let dir=tempfile::tempdir().unwrap();let mut s=Store::open(dir.path()).unwrap();let manual=s.create_tag("研究").unwrap();let folder=s.create_tag_with_source("文件夹","folder").unwrap();s.create_tag_with_source("待审","ai").unwrap();
        assert_eq!(s.browser_tag_catalog("研究").unwrap()["tags"][0]["id"],manual.id);
        assert_eq!(s.browser_tag_catalog("文件夹").unwrap()["tags"],json!([]));assert_eq!(s.browser_tag_catalog("待审").unwrap()["tags"],json!([]));
        let mut invalid=request();invalid["tagIds"]=json!([folder.id]);assert!(s.capture(&invalid).is_err());assert_eq!(s.query(&Query::default()).unwrap()["total"],0);
        let result=s.capture(&request()).unwrap();assert!(s.ai_task(result["id"].as_str().unwrap()).unwrap().is_none());
    }
    #[test]
    fn image_capture_options_queue_and_retry_are_idempotent(){
        use std::io::Cursor;
        let dir=tempfile::tempdir().unwrap();let mut s=Store::open(dir.path()).unwrap();s.conn.execute("INSERT INTO settings VALUES('ai',?)",[json!({"enabled":true,"endpoint":"http://localhost:11434/v1","model":"fixture","allowNewTags":true}).to_string()]).unwrap();let tag=s.create_tag("图片").unwrap();
        let r=crate::image_capture::Request::parse(&json!({"requestId":id(),"kind":"web_image","title":"风景","url":"https://example.test/photo","imageUrl":"https://example.test/a.png","tagIds":[tag.id]})).unwrap();
        let mut data=Cursor::new(vec![]);image::DynamicImage::new_rgb8(10,8).write_to(&mut data,image::ImageFormat::Png).unwrap();
        let make=||crate::image_capture::inspect_bytes(data.get_ref().clone(),&r.image_url,&r.image_url,None).unwrap();let result=s.commit_image(&r,make()).unwrap();let fid=result["id"].as_str().unwrap();let before=s.file(fid).unwrap();assert_eq!(before.tags[0].source,"manual");assert_eq!(result["aiQueued"],false);assert!(s.ai_task(fid).unwrap().is_none());assert_eq!(s.commit_image(&r,make()).unwrap()["duplicate"],true);assert_eq!(s.file(fid).unwrap().version,before.version);
    }
}

#[cfg(test)]
mod capture_policy_tests {
    use super::*;
    #[test]
    fn selected_tags_skip_auto_ai_for_links_excerpts_and_images_but_untagged_still_queue() {
        let dir=tempfile::tempdir().unwrap();let mut s=Store::open(dir.path()).unwrap();s.conn.execute("INSERT INTO settings VALUES('ai',?)",[json!({"enabled":true,"endpoint":"http://localhost:11434/v1","model":"fixture","allowNewTags":true}).to_string()]).unwrap();let tag=s.create_tag("手动选择").unwrap();
        for kind in ["web_page","web_excerpt"] {for tagged in [true,false] {
            let mut req=json!({"requestId":id(),"kind":kind,"title":"fixture","url":format!("https://example.test/{}/{}",kind,tagged),"excerpt":if kind=="web_excerpt"{"synthetic text"}else{""}});
            if tagged {req["tagIds"]=json!([tag.id]);}
            let result=s.capture(&req).unwrap();let fid=result["id"].as_str().unwrap();assert_eq!(result["aiQueued"],!tagged);assert_eq!(s.ai_task(fid).unwrap().is_some(),!tagged);
            if !tagged {req["requestId"]=json!(id());req["tagIds"]=json!([tag.id]);assert_eq!(s.capture(&req).unwrap()["aiQueued"],false);assert_eq!(s.ai_task(fid).unwrap().unwrap().status,"cancelled");}
        }}
        for tagged in [true,false] {
            let mut value=json!({"requestId":id(),"kind":"web_image","title":"fixture","url":"https://example.test/image","imageUrl":format!("https://example.test/{}.png",tagged)});if tagged {value["tagIds"]=json!([tag.id]);}
            let request=crate::image_capture::Request::parse(&value).unwrap();let mut data=std::io::Cursor::new(vec![]);image::DynamicImage::new_rgb8(if tagged {10}else{11},8).write_to(&mut data,image::ImageFormat::Png).unwrap();
            let prepared=crate::image_capture::inspect_bytes(data.into_inner(),&request.image_url,&request.image_url,None).unwrap();let result=s.commit_image(&request,prepared).unwrap();assert_eq!(result["aiQueued"],!tagged);assert_eq!(s.ai_task(result["id"].as_str().unwrap()).unwrap().is_some(),!tagged);
        }
    }
}
