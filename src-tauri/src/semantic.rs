//! Explicit, optional embeddings over bounded samples of the existing local text index.
use crate::{ai, db::Store, model::*};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::Read,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    },
    time::Duration,
};
pub const MAX_FILES: usize = 500;
pub const MAX_DIM: usize = 2048;
pub const CACHE_BUDGET: i64 = 32 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Config {
    pub enabled: bool,
    pub endpoint: String,
    pub model: String,
    pub max_requests: usize,
    pub min_similarity: f32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: String::new(),
            model: String::new(),
            max_requests: 100,
            min_similarity: 0.35,
        }
    }
}
impl Config {
    fn validate(&mut self) -> Result<()> {
        let mut c = ai::Config {
            enabled: self.enabled,
            endpoint: self.endpoint.clone(),
            model: self.model.clone(),
            allow_new_tags: false,
        };
        c.validate()
            .map_err(|e| e.replace("；图片识别需要支持视觉的模型", ""))?;
        self.endpoint = c.endpoint;
        self.model = c.model;
        if self.enabled
            && reqwest::Url::parse(&self.endpoint)
                .is_ok_and(|u| u.host_str() == Some("api.deepseek.com"))
        {
            return Err(err("SEMANTIC_CONFIG", "DeepSeek 官方当前未提供 Embeddings 接口；deepseek-flash 可用于 AI 标注，语义检索请配置支持 Embeddings 的向量服务和向量模型"));
        }
        if self.endpoint.ends_with("/chat/completions")
            || self.endpoint.ends_with("/responses")
            || self.endpoint.ends_with("/api/embed")
        {
            return Err(err(
                "SEMANTIC_CONFIG",
                "请填写 Embeddings 服务根地址，不能使用对话接口",
            ));
        }
        if !(1..=200).contains(&self.max_requests)
            || !self.min_similarity.is_finite()
            || !(0.0..=0.95).contains(&self.min_similarity)
        {
            return Err(err(
                "SEMANTIC_CONFIG",
                "请求上限为 1～200，相关度阈值为 0～0.95",
            ));
        }
        Ok(())
    }
    fn url(&self) -> String {
        if self.endpoint.ends_with("/embeddings") {
            self.endpoint.clone()
        } else {
            let root = reqwest::Url::parse(&self.endpoint)
                .is_ok_and(|u| u.path() == "/" || u.path().is_empty());
            format!(
                "{}{}/embeddings",
                self.endpoint,
                if root { "/v1" } else { "" }
            )
        }
    }
    fn stamp(&self) -> String {
        sha(&format!("samples-v1\n{}\n{}", self.url(), self.model))
    }
}
fn sha(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}
pub fn attach(s: &Store) -> Result<()> {
    match attach_once(s) {
        Ok(()) => Ok(()),
        Err(e) if e.contains("not a database") || e.contains("malformed") => {
            let _ = s.conn.execute_batch("DETACH DATABASE semantic");
            for n in ["index.sqlite", "index.sqlite-wal", "index.sqlite-shm"] {
                let source = s.root.join("cache/semantic").join(n);
                if source.exists() {
                    std::fs::rename(
                        &source,
                        source.with_file_name(format!("{n}.corrupt-{}", now())),
                    )
                    .map_err(|e| err("STORAGE_UNAVAILABLE", e))?;
                }
            }
            attach_once(s)
        }
        Err(e) => Err(e),
    }
}
fn attach_once(s: &Store) -> Result<()> {
    let dir = s.root.join("cache/semantic");
    std::fs::create_dir_all(&dir).map_err(|e| err("STORAGE_UNAVAILABLE", e))?;
    s.conn
        .execute(
            "ATTACH DATABASE ? AS semantic",
            [dir.join("index.sqlite").to_string_lossy().as_ref()],
        )
        .map_err(sql_err)?;
    s.conn.execute_batch("PRAGMA semantic.journal_mode=WAL;PRAGMA semantic.secure_delete=ON;CREATE TABLE IF NOT EXISTS semantic.samples(file_id TEXT NOT NULL,stamp TEXT NOT NULL,model_key TEXT NOT NULL,slot INTEGER NOT NULL,start INTEGER NOT NULL,text TEXT NOT NULL,location TEXT NOT NULL,vector TEXT NOT NULL,dim INTEGER NOT NULL,PRIMARY KEY(file_id,slot));CREATE TABLE IF NOT EXISTS semantic.control(id INTEGER PRIMARY KEY CHECK(id=1),generation INTEGER NOT NULL DEFAULT 0,state TEXT NOT NULL DEFAULT 'idle',requested INTEGER NOT NULL DEFAULT 0,completed INTEGER NOT NULL DEFAULT 0,failed INTEGER NOT NULL DEFAULT 0,error TEXT NOT NULL DEFAULT '');CREATE TABLE IF NOT EXISTS semantic.queue(file_id TEXT PRIMARY KEY,batch TEXT NOT NULL,created INTEGER NOT NULL,state TEXT NOT NULL DEFAULT 'pending',error TEXT NOT NULL DEFAULT '');UPDATE semantic.queue SET state='interrupted' WHERE state IN ('waiting','running');INSERT OR IGNORE INTO semantic.control(id) VALUES(1);UPDATE semantic.control SET state='interrupted',generation=generation+1 WHERE state='running';").map_err(sql_err)?;
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Sample {
    pub start: usize,
    pub text: String,
    pub location: Value,
}
#[derive(Clone)]
pub struct Work {
    pub id: String,
    pub name: String,
    pub stamp: String,
    pub samples: Vec<Sample>,
    pub reuse: Option<Vec<Vec<f32>>>,
}
#[cfg(test)]
fn sample(body: &str, locations: &[Value]) -> Vec<Sample> {
    let chars: Vec<char> = body.chars().collect();
    let len = chars.len();
    if len == 0 {
        return vec![];
    }
    let mut starts = vec![0];
    if len > 1000 {
        starts.push((len / 2).saturating_sub(500));
        starts.push(len.saturating_sub(1000));
    }
    starts.sort();
    starts.dedup();
    starts
        .into_iter()
        .filter_map(|start| {
            let text: String = chars[start..(start + 1000).min(len)].iter().collect();
            if text.trim().is_empty() {
                return None;
            }
            let location = locations
                .iter()
                .find(|l| {
                    let lo = l["start"].as_u64().unwrap_or(1) as usize - 1;
                    let n = l["length"].as_u64().unwrap_or(0) as usize;
                    lo <= start && start < lo + n
                })
                .cloned()
                .unwrap_or(Value::Null);
            Some(Sample {
                start,
                text,
                location,
            })
        })
        .collect()
}
fn doc_stamp(revision: &str, identity: &str, path: &str, body: &str, locations: &str) -> String {
    sha(&format!(
        "{revision}\n{identity}\n{path}\n{}\n{locations}",
        sha(body)
    ))
}
impl Store {
    pub fn semantic_file_current(&self, fid: &str) -> Result<bool> {
        let f = self.file_metadata(fid)?;
        let m = crate::fsops::inspect(std::path::Path::new(&f.path))?;
        Ok(m.revision == f.revision && m.identity == f.identity)
    }
    pub fn semantic_config(&self) -> Result<Config> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key='semanticConfig'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql_err)?;
        serde_json::from_str(raw.as_deref().unwrap_or("{}"))
            .map_err(|_| err("SEMANTIC_CONFIG", "语义设置损坏，请重新配置"))
    }
    pub fn semantic_save(&self, v: &Value) -> Result<Value> {
        let mut c: Config = serde_json::from_value(v["config"].clone())
            .map_err(|_| err("SEMANTIC_CONFIG", "语义设置无效"))?;
        c.validate()?;
        if c.enabled && v["consent"].as_bool() != Some(true) {
            return Err(err(
                "SEMANTIC_CONSENT",
                "请确认会向所配置的服务发送正文片段与查询",
            ));
        }
        let old = self.semantic_config()?;
        self.conn.execute("UPDATE semantic.queue SET state='interrupted' WHERE state IN ('waiting','running')",[]).map_err(sql_err)?;
        let secret = v["apiKey"].as_str().unwrap_or("").trim();
        if !secret.is_empty() {
            ai::write_secret(&self.root, "semantic-key.dpapi", secret)?;
        } else if v["clearKey"].as_bool() == Some(true) || old.endpoint != c.endpoint {
            ai::write_secret(&self.root, "semantic-key.dpapi", "")?;
        }
        self.conn.execute("INSERT INTO settings(key,value) VALUES('semanticConfig',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[json!(c).to_string()]).map_err(sql_err)?;
        self.conn
            .execute(
                "UPDATE semantic.control SET generation=generation+1,state='idle' WHERE id=1",
                [],
            )
            .map_err(sql_err)?;
        self.semantic_status()
    }
    pub fn semantic_clear(&self) -> Result<Value> {
        self.conn.execute_batch("UPDATE semantic.control SET generation=generation+1,state='idle',error='' WHERE id=1;DELETE FROM semantic.samples;DELETE FROM semantic.queue;PRAGMA semantic.wal_checkpoint(TRUNCATE);VACUUM semantic;").map_err(sql_err)?;
        self.semantic_status()
    }
    fn semantic_work(&self, ids: Option<&[String]>) -> Result<Vec<Work>> {
        if !self.search_enabled()? {
            return Err(err("INDEX_DISABLED", "请先启用正文索引并等待文件处理完成"));
        }
        let mut sql = "SELECT f.id,f.name,f.revision,f.identity,f.path,d.updated_at,d.locations,length(d.body),substr(d.body,1,1000),substr(d.body,max(1,length(d.body)/2-499),1000),substr(d.body,max(1,length(d.body)-999),1000) FROM files f JOIN search.docs d ON d.id=f.id WHERE f.removed_at IS NULL AND f.status='available' AND d.state='done' AND d.parser=3 AND d.revision=f.revision AND d.identity=f.identity AND d.path=f.path ORDER BY f.added DESC,f.id LIMIT 500".to_string();
        if let Some(ids) = ids {
            if ids.is_empty() || ids.len() > MAX_FILES {
                return Err(err("INVALID_INPUT", "请选择 1～500 个文件"));
            }
            sql = sql.replace(
                " ORDER BY",
                &format!(" AND f.id IN ({}) ORDER BY", vec!["?"; ids.len()].join(",")),
            );
        }
        let mut st = self.conn.prepare(&sql).map_err(sql_err)?;
        let rows = st
            .query_map(
                rusqlite::params_from_iter(ids.iter().flat_map(|v| v.iter())),
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, i64>(5)?,
                        r.get::<_, String>(6)?,
                        r.get::<_, usize>(7)?,
                        r.get::<_, String>(8)?,
                        r.get::<_, String>(9)?,
                        r.get::<_, String>(10)?,
                    ))
                },
            )
            .map_err(sql_err)?;
        let mut out = vec![];
        for row in rows {
            let (id, name, revision, identity, path, updated, locations, len, first, middle, last) =
                row.map_err(sql_err)?;
            let stamp = doc_stamp(
                &revision,
                &identity,
                &path,
                &format!("{updated}:{len}"),
                &locations,
            );
            let loc: Vec<Value> = serde_json::from_str(&locations).unwrap_or_default();
            let mut starts = vec![(0, first)];
            if len > 1000 {
                starts.push(((len / 2).saturating_sub(500), middle));
                starts.push((len.saturating_sub(1000), last));
            }
            starts.sort_by_key(|s| s.0);
            starts.dedup_by_key(|s| s.0);
            let samples = starts
                .into_iter()
                .filter_map(|(start, text)| {
                    if text.trim().is_empty() {
                        return None;
                    }
                    let location = loc
                        .iter()
                        .find(|l| {
                            let lo = l["start"].as_u64().unwrap_or(1).saturating_sub(1) as usize;
                            let n = l["length"].as_u64().unwrap_or(0) as usize;
                            lo <= start && start < lo + n
                        })
                        .cloned()
                        .unwrap_or(Value::Null);
                    let text = if let Some(n) = location["length"].as_u64() {
                        let lo = location["start"].as_u64().unwrap_or(1).saturating_sub(1) as usize;
                        text.chars()
                            .take((lo + n as usize).saturating_sub(start))
                            .collect()
                    } else {
                        text
                    };
                    Some(Sample {
                        start,
                        text,
                        location,
                    })
                })
                .collect::<Vec<_>>();
            if !samples.is_empty() {
                out.push(Work {
                    id,
                    name,
                    stamp,
                    samples,
                    reuse: None,
                });
            }
        }
        Ok(out)
    }
    fn semantic_stamps(&self) -> Result<HashMap<String, String>> {
        if !self.search_enabled()? {
            return Err(err("INDEX_DISABLED", "请先启用正文索引"));
        }
        let mut st=self.conn.prepare("SELECT f.id,f.revision,f.identity,f.path,d.updated_at,length(d.body),d.locations FROM files f JOIN search.docs d ON d.id=f.id WHERE f.removed_at IS NULL AND f.status='available' AND d.state='done' AND d.parser=3 AND d.revision=f.revision AND d.identity=f.identity AND d.path=f.path AND f.id IN (SELECT DISTINCT file_id FROM semantic.samples) ORDER BY f.added DESC,f.id LIMIT 500").map_err(sql_err)?;
        let rows = st
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, String>(6)?,
                ))
            })
            .map_err(sql_err)?;
        let mut out = HashMap::new();
        for row in rows {
            let (id, revision, identity, path, updated, len, locations) = row.map_err(sql_err)?;
            out.insert(
                id,
                doc_stamp(
                    &revision,
                    &identity,
                    &path,
                    &format!("{updated}:{len}"),
                    &locations,
                ),
            );
        }
        Ok(out)
    }
    pub fn semantic_ready(&self, c: &Config) -> Result<usize> {
        let current = self.semantic_stamps()?;
        let mut st = self
            .conn
            .prepare("SELECT DISTINCT file_id,stamp FROM semantic.samples WHERE model_key=?")
            .map_err(sql_err)?;
        let rows = st
            .query_map([c.stamp()], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(sql_err)?;
        let mut n = 0;
        for row in rows {
            let (id, stamp) = row.map_err(sql_err)?;
            if current.get(&id) == Some(&stamp) {
                n += 1;
            }
        }
        Ok(n)
    }
    pub fn semantic_plan(&self) -> Result<(Value, Vec<Work>, Config)> {
        self.semantic_plan_for(None)
    }
    pub fn semantic_plan_for(&self, ids: Option<&[String]>) -> Result<(Value, Vec<Work>, Config)> {
        self.semantic_plan_mode(ids, "update")
    }
    pub fn semantic_plan_mode(
        &self,
        ids: Option<&[String]>,
        mode: &str,
    ) -> Result<(Value, Vec<Work>, Config)> {
        if !["missing", "update", "rebuild"].contains(&mode) {
            return Err(err("INVALID_INPUT", "语义处理方式无效"));
        }
        let c = self.semantic_config()?;
        if !c.enabled {
            return Err(err("SEMANTIC_DISABLED", "请先配置并启用语义检索"));
        }
        let all = self.semantic_work(ids)?;
        let mk = c.stamp();
        let mut todo = vec![];
        let mut cached = 0;
        for mut w in all {
            let n:i64=self.conn.query_row("SELECT COUNT(*) FROM semantic.samples WHERE file_id=? AND stamp=? AND model_key=?",params![w.id,w.stamp,mk],|r|r.get(0)).map_err(sql_err)?;
            let exists: i64 = self
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM semantic.samples WHERE file_id=? AND model_key=?",
                    params![w.id, mk],
                    |r| r.get(0),
                )
                .map_err(sql_err)?;
            if mode != "rebuild"
                && (n == w.samples.len() as i64 || (mode == "missing" && exists > 0))
            {
                cached += 1;
            } else {
                if mode != "rebuild" {
                    w.reuse = self.semantic_reuse(&w, &c)?;
                }
                todo.push(w);
            }
        }
        let pending = todo.len();
        let mut requests = 0;
        todo.retain(|w| {
            if w.reuse.is_some() {
                true
            } else if requests < c.max_requests {
                requests += 1;
                true
            } else {
                false
            }
        });
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(DISTINCT file_id) FROM semantic.samples",
                [],
                |r| r.get(0),
            )
            .map_err(sql_err)?;
        if count as usize
            + todo
                .iter()
                .filter(|w| {
                    self.conn
                        .query_row(
                            "SELECT COUNT(*) FROM semantic.samples WHERE file_id=?",
                            [&w.id],
                            |r| r.get::<_, i64>(0),
                        )
                        .unwrap_or(0)
                        == 0
                })
                .count()
            > MAX_FILES
        {
            return Err(err(
                "SEMANTIC_LIMIT",
                "语义缓存最多 500 个文件，请清理旧缓存后重新建立",
            ));
        }
        let info = json!({"files":todo.iter().map(|w|json!({"id":w.id,"name":w.name,"stamp":w.stamp,"sampleFingerprint":sha(&json!(w.samples).to_string()),"characters":w.samples.iter().map(|s|s.text.chars().count()).sum::<usize>(),"samples":w.samples.len()})).collect::<Vec<_>>(),"mode":mode,"cached":cached,"pending":pending,"requests":requests,"reuse":todo.iter().filter(|w|w.reuse.is_some()).count(),"total":todo.len(),"model":c.model,"endpoint":c.endpoint,"modelKey":mk,"selection":ids});
        let mut p = info.clone();
        p["fingerprint"] = json!(sha(&info.to_string()));
        Ok((p, todo, c))
    }
    fn semantic_reuse(&self, w: &Work, c: &Config) -> Result<Option<Vec<Vec<f32>>>> {
        let mut st = self.conn.prepare("SELECT text,vector FROM semantic.samples WHERE file_id=? AND model_key=? ORDER BY slot").map_err(sql_err)?;
        let rows = st
            .query_map(params![w.id, c.stamp()], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(sql_err)?;
        let old = rows
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql_err)?;
        if old.len() != w.samples.len() {
            return Ok(None);
        }
        let mut vectors = vec![];
        for ((text, encoded), sample) in old.iter().zip(&w.samples) {
            if text != &sample.text {
                return Ok(None);
            }
            let vector: Vec<f32> = match serde_json::from_str(encoded) {
                Ok(v) => v,
                Err(_) => return Ok(None),
            };
            match normalized(&vector) {
                Ok(v) => vectors.push(v),
                Err(_) => return Ok(None),
            }
        }
        Ok(Some(vectors))
    }
    pub fn semantic_enqueue(&self, id: &str, batch: &str) -> Result<()> {
        self.conn.execute("INSERT OR IGNORE INTO semantic.queue(file_id,batch,created) VALUES(?,?,strftime('%s','now'))",params![id,batch]).map_err(sql_err)?;
        self.conn.execute("DELETE FROM semantic.queue WHERE file_id IN (SELECT file_id FROM semantic.queue WHERE state NOT IN ('waiting','running') ORDER BY created DESC,file_id LIMIT -1 OFFSET 2000)",[]).map_err(sql_err)?;
        Ok(())
    }
    pub fn semantic_queue_state(&self, id: &str, state: &str, error: &str) -> Result<()> {
        self.conn
            .execute(
                "UPDATE semantic.queue SET state=?,error=? WHERE file_id=?",
                params![state, error, id],
            )
            .map_err(sql_err)?;
        Ok(())
    }
    pub fn semantic_queue(&self) -> Result<Value> {
        let config = self.semantic_config()?;
        let mut st = self.conn.prepare("SELECT q.file_id,f.name,q.batch,q.created,q.state,q.error,f.status,CASE WHEN f.extension NOT IN ('txt','md','markdown','docx','xlsx','csv','tsv','pdf') THEN 'unsupported' WHEN d.state='done' AND trim(d.body)='' THEN 'empty' ELSE d.state END,COALESCE(d.revision=f.revision AND d.identity=f.identity AND d.path=f.path AND d.parser=3,0) FROM semantic.queue q JOIN files f ON f.id=q.file_id LEFT JOIN search.docs d ON d.id=f.id WHERE f.removed_at IS NULL ORDER BY q.created DESC,q.batch,q.file_id LIMIT 2000").map_err(sql_err)?;
        let rows = st
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, bool>(8)?,
                ))
            })
            .map_err(sql_err)?;
        let mut items = vec![];
        for row in rows {
            let (id, name, batch, created, task, error, available, indexed, fresh) =
                row.map_err(sql_err)?;
            let cached: Option<(String, String)> = self
                .conn
                .query_row(
                    "SELECT stamp,model_key FROM semantic.samples WHERE file_id=? LIMIT 1",
                    [&id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(sql_err)?;
            let state = if available != "available" {
                "unavailable"
            } else if indexed.as_deref() == Some("unsupported") {
                "unsupported"
            } else if indexed.as_deref() == Some("failed") {
                "index_failed"
            } else if indexed.as_deref() == Some("empty") {
                "empty"
            } else if indexed.as_deref() != Some("done") || !fresh {
                "indexing"
            } else if ["running", "waiting", "failed", "interrupted", "cancelled"]
                .contains(&task.as_str())
            {
                task.as_str()
            } else if let Some((stamp, model)) = &cached {
                if *model != config.stamp() {
                    "model_changed"
                } else if self.semantic_stamp_for(&id)?.as_ref() == Some(stamp) {
                    "cached"
                } else {
                    "changed"
                }
            } else {
                "pending"
            };
            items.push(json!({"id":id,"name":name,"batch":batch,"created":created,"state":state,"error":error}));
        }
        Ok(json!({"items":items,"limit":2000}))
    }
    pub fn semantic_queue_remove(&self, ids: &[String]) -> Result<Value> {
        for id in ids {
            self.conn.execute("DELETE FROM semantic.queue WHERE file_id=? AND state NOT IN ('running','waiting')",[id]).map_err(sql_err)?;
        }
        self.semantic_queue()
    }
    pub fn semantic_status(&self) -> Result<Value> {
        let config = self.semantic_config()?;
        let(count,bytes):(i64,i64)=self.conn.query_row("SELECT COUNT(DISTINCT file_id),COALESCE(SUM(length(vector)+length(text)*4),0) FROM semantic.samples",[],|r|Ok((r.get(0)?,r.get(1)?))).map_err(sql_err)?;
        let task:Value=self.conn.query_row("SELECT state,requested,completed,failed,error FROM semantic.control WHERE id=1",[],|r|Ok(json!({"state":r.get::<_,String>(0)?,"requested":r.get::<_,i64>(1)?,"completed":r.get::<_,i64>(2)?,"failed":r.get::<_,i64>(3)?,"error":r.get::<_,String>(4)?}))).map_err(sql_err)?;
        let eligible:i64=self.conn.query_row("SELECT COUNT(*) FROM search.docs d JOIN files f ON f.id=d.id WHERE f.removed_at IS NULL AND f.status='available' AND d.state='done' AND d.parser=3 AND d.revision=f.revision AND d.identity=f.identity AND d.path=f.path",[],|r|r.get(0)).map_err(sql_err)?;
        Ok(
            json!({"config":config,"hasKey":self.root.join("semantic-key.dpapi").is_file(),"files":count,"ready":if config.enabled {self.semantic_ready(&config).unwrap_or(0)} else{0},"bytes":bytes,"eligible":eligible,"maxFiles":MAX_FILES,"maxDimension":MAX_DIM,"budget":CACHE_BUDGET,"diskBytes":(["index.sqlite","index.sqlite-wal","index.sqlite-shm"].iter().filter_map(|n|std::fs::metadata(self.root.join("cache/semantic").join(n)).ok()).map(|m|m.len()).sum::<u64>()),"task":task}),
        )
    }
    pub fn semantic_begin(&self) -> Result<i64> {
        self.conn.execute("UPDATE semantic.control SET generation=generation+1,state='running',requested=0,completed=0,failed=0,error='' WHERE id=1",[]).map_err(sql_err)?;
        self.conn
            .query_row(
                "SELECT generation FROM semantic.control WHERE id=1",
                [],
                |r| r.get(0),
            )
            .map_err(sql_err)
    }
    fn semantic_stamp_for(&self, id: &str) -> Result<Option<String>> {
        let row:Option<(String,String,String,i64,i64,String)> = self.conn.query_row("SELECT f.revision,f.identity,f.path,d.updated_at,length(d.body),d.locations FROM files f JOIN search.docs d ON d.id=f.id WHERE f.id=? AND f.removed_at IS NULL AND f.status='available' AND d.state='done' AND d.parser=3 AND d.revision=f.revision AND d.identity=f.identity AND d.path=f.path",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional().map_err(sql_err)?;
        Ok(
            row.map(|(revision, identity, path, updated, len, locations)| {
                doc_stamp(
                    &revision,
                    &identity,
                    &path,
                    &format!("{updated}:{len}"),
                    &locations,
                )
            }),
        )
    }
    pub fn semantic_commit(
        &mut self,
        w: &Work,
        c: &Config,
        generation: i64,
        vectors: &[Vec<f32>],
    ) -> Result<bool> {
        let active: bool = self
            .conn
            .query_row(
                "SELECT generation=? AND state='running' FROM semantic.control WHERE id=1",
                [generation],
                |r| r.get(0),
            )
            .map_err(sql_err)?;
        if !active || self.semantic_config()? != *c {
            return Ok(false);
        }
        if self.semantic_stamp_for(&w.id)?.as_ref() != Some(&w.stamp)
            || !self.semantic_file_current(&w.id)?
        {
            return Ok(false);
        }
        if vectors.len() != w.samples.len() {
            return Err(err("SEMANTIC_RESPONSE", "向量数量不一致"));
        }
        let checked: Vec<Vec<f32>> = vectors
            .iter()
            .map(|v| normalized(v))
            .collect::<Result<_>>()?;
        if checked.iter().any(|v| v.len() != checked[0].len()) {
            return Err(err("SEMANTIC_DIMENSION", "向量维数不一致"));
        }
        let encoded: Vec<String> = checked.iter().map(|v| json!(v).to_string()).collect();
        let used:i64=self.conn.query_row("SELECT COALESCE(SUM(length(vector)+length(text)*4),0) FROM semantic.samples WHERE file_id<>?",[&w.id],|r|r.get(0)).map_err(sql_err)?;
        let additional: usize = encoded.iter().map(String::len).sum::<usize>()
            + w.samples
                .iter()
                .map(|s| s.text.chars().count() * 4)
                .sum::<usize>();
        if used + additional as i64 > CACHE_BUDGET {
            return Err(err(
                "SEMANTIC_LIMIT",
                "语义缓存达到 32 MiB 内容预算，请清理缓存",
            ));
        }
        let tx = self.conn.transaction().map_err(sql_err)?;
        tx.execute("DELETE FROM semantic.samples WHERE file_id=?", [&w.id])
            .map_err(sql_err)?;
        for (i, (sample, vector)) in w.samples.iter().zip(encoded).enumerate() {
            tx.execute(
                "INSERT INTO semantic.samples VALUES(?,?,?,?,?,?,?,?,?)",
                params![
                    w.id,
                    w.stamp,
                    c.stamp(),
                    i as i64,
                    sample.start as i64,
                    sample.text,
                    sample.location.to_string(),
                    vector,
                    vectors[i].len() as i64
                ],
            )
            .map_err(sql_err)?;
        }
        tx.execute(
            "UPDATE semantic.control SET completed=completed+1 WHERE id=1",
            [],
        )
        .map_err(sql_err)?;
        tx.commit().map_err(sql_err)?;
        Ok(true)
    }
    pub fn semantic_rank(&self, c: &Config, vector: &[f32]) -> Result<Vec<Hit>> {
        let current = self.semantic_stamps()?;
        let mut st=self.conn.prepare("SELECT file_id,stamp,start,text,location,vector FROM semantic.samples WHERE model_key=? LIMIT 1500").map_err(sql_err)?;
        let rows = st
            .query_map([c.stamp()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                ))
            })
            .map_err(sql_err)?;
        let mut best: HashMap<String, Hit> = HashMap::new();
        for row in rows {
            let (id, stamp, start, text, location, raw) = row.map_err(sql_err)?;
            if current.get(&id) != Some(&stamp) {
                continue;
            }
            let v: Vec<f32> = serde_json::from_str(&raw)
                .map_err(|_| err("SEMANTIC_CACHE", "语义缓存损坏，请清理后重建"))?;
            let v = normalized(&v)?;
            if v.len() != vector.len() {
                return Err(err(
                    "SEMANTIC_DIMENSION",
                    "模型向量维数变化，请清理缓存后重建",
                ));
            }
            let score: f32 = v.iter().zip(vector).map(|(a, b)| a * b).sum();
            if !score.is_finite() || score < c.min_similarity {
                continue;
            }
            let hit = Hit {
                id: id.clone(),
                stamp,
                score: score.clamp(-1., 1.),
                snippet: text.chars().take(180).collect(),
                location: serde_json::from_str(&location).unwrap_or(Value::Null),
                start,
            };
            if best.get(&id).is_none_or(|old| old.score < hit.score) {
                best.insert(id, hit);
            }
        }
        let mut hits: Vec<_> = best.into_values().collect();
        hits.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.id.cmp(&b.id)));
        Ok(hits)
    }
    pub fn semantic_result(&self, q: &Query, hits: &[Hit]) -> Result<Value> {
        let c = self.semantic_config()?;
        if !c.enabled {
            return Err(err("SEMANTIC_DISABLED", "语义检索已关闭"));
        }
        let current = self.semantic_stamps()?;
        let valid: Vec<_> = hits
            .iter()
            .filter(|h| {
                current.get(&h.id) == Some(&h.stamp)
                    && self.semantic_file_current(&h.id).unwrap_or(false)
            })
            .collect();
        let mut filtered = q.clone();
        if q.search_mode == "unified" {
            filtered.supplemental_ids = Some(valid.iter().map(|h| h.id.clone()).collect());
        } else {
            filtered.text.clear();
            filtered.search_mode = "metadata".into();
            filtered.ranked_ids = Some(valid.iter().map(|h| h.id.clone()).collect());
        }
        let mut value = self.query(&filtered)?;
        let map: HashMap<_, _> = valid.iter().map(|h| (&h.id, *h)).collect();
        for f in value["files"].as_array_mut().unwrap() {
            if f.get("contentMatch").is_some() {
                continue;
            }
            if let Some(h) = f["id"].as_str().and_then(|id| map.get(&id.to_owned())) {
                let fallback = format!("正文第 {} 字符起", h.start + 1);
                let label = h.location["label"].as_str().unwrap_or(&fallback);
                f["contentMatch"] = json!({"snippet":h.snippet,"queryText":q.text,"source":format!("语义相关 · {}",label),"location":h.location,"characterOffset":h.start,"coverage":"每文件最多 3 个 1000 字符样本；相似度不是确定命中","encoding":"本地正文索引","truncated":true,"semantic":true,"score":h.score});
                if !f["searchMatch"].is_object() {
                    f["searchMatch"] = json!({"queryText":q.text,"sources":[]});
                }
                if let Some(sources) = f["searchMatch"]["sources"].as_array_mut() {
                    sources.push(json!({"kind":"semantic"}));
                }
            }
        }
        value["semantic"] = json!({"sampledFiles":current.len(),"rankedFiles":hits.len(),"threshold":c.min_similarity});
        Ok(value)
    }
}
#[derive(Clone)]
pub struct Hit {
    pub id: String,
    pub stamp: String,
    pub score: f32,
    pub snippet: String,
    pub location: Value,
    pub start: i64,
}
#[derive(Clone)]
struct Session {
    created: i64,
    model_key: String,
    text: String,
    hits: Vec<Hit>,
}
#[derive(Default)]
pub struct Control {
    pub busy: AtomicBool,
    pub epoch: AtomicU64,
    sessions: Mutex<HashMap<String, Session>>,
}
impl Control {
    pub fn invalidate(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut s) = self.sessions.lock() {
            s.clear();
        }
    }
    pub fn remember(&self, c: &Config, text: &str, hits: Vec<Hit>) -> Result<String> {
        let mut all = self
            .sessions
            .lock()
            .map_err(|_| err("INTERNAL_ERROR", "语义结果锁异常"))?;
        all.retain(|_, s| now() - s.created < 600_000);
        if all.len() >= 8 {
            if let Some(old) = all
                .iter()
                .min_by_key(|(_, s)| s.created)
                .map(|(k, _)| k.clone())
            {
                all.remove(&old);
            }
        }
        let token = id();
        all.insert(
            token.clone(),
            Session {
                created: now(),
                model_key: c.stamp(),
                text: text.into(),
                hits,
            },
        );
        Ok(token)
    }
    pub fn resolve(&self, s: &Store, q: &Query) -> Result<Value> {
        let all = self
            .sessions
            .lock()
            .map_err(|_| err("INTERNAL_ERROR", "语义结果锁异常"))?;
        let session = all
            .get(&q.semantic_session)
            .filter(|x| {
                now() - x.created < 600_000
                    && x.text == q.text
                    && s.semantic_config().is_ok_and(|c| c.stamp() == x.model_key)
            })
            .ok_or_else(|| err("SEMANTIC_EXPIRED", "语义结果已过期，请重新点击语义查找"))?;
        s.semantic_result(q, &session.hits)
    }
}
pub fn normalized(v: &[f32]) -> Result<Vec<f32>> {
    if v.is_empty() || v.len() > MAX_DIM || v.iter().any(|x| !x.is_finite()) {
        return Err(err("SEMANTIC_RESPONSE", "向量为空、超限或含无效数字"));
    }
    let norm = v
        .iter()
        .map(|x| (*x as f64) * (*x as f64))
        .sum::<f64>()
        .sqrt();
    if !norm.is_finite() || norm < 1e-12 {
        return Err(err("SEMANTIC_RESPONSE", "模型返回零向量"));
    }
    Ok(v.iter().map(|x| (*x as f64 / norm) as f32).collect())
}
pub fn parse_vectors(v: &Value, count: usize) -> Result<Vec<Vec<f32>>> {
    let data = v["data"]
        .as_array()
        .filter(|a| a.len() == count)
        .ok_or_else(|| err("SEMANTIC_RESPONSE", "服务未返回全部向量"))?;
    let mut out = vec![None; count];
    let mut dim = 0;
    for row in data {
        let index = row["index"]
            .as_u64()
            .filter(|i| (*i as usize) < count)
            .ok_or_else(|| err("SEMANTIC_RESPONSE", "向量序号无效"))? as usize;
        if out[index].is_some() {
            return Err(err("SEMANTIC_RESPONSE", "向量序号重复"));
        }
        let vector: Vec<f32> = serde_json::from_value(row["embedding"].clone())
            .map_err(|_| err("SEMANTIC_RESPONSE", "服务需要返回 float 数组"))?;
        let vector = normalized(&vector)?;
        if dim != 0 && dim != vector.len() {
            return Err(err("SEMANTIC_DIMENSION", "返回向量维数不一致"));
        }
        dim = vector.len();
        out[index] = Some(vector);
    }
    Ok(out.into_iter().map(Option::unwrap).collect())
}
pub fn connection_test(c: &Config, key: &str) -> Result<Value> {
    let start = std::time::Instant::now();
    let sample = "拾签连接测试：这是合成文字，不包含本机文件内容。".to_string();
    let vectors = embed(c, key, &[sample])?;
    Ok(
        json!({"endpoint":c.url(),"model":c.model,"dimension":vectors[0].len(),"durationMs":start.elapsed().as_millis() as u64,"requests":1}),
    )
}
pub fn embed(c: &Config, key: &str, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
    let mut validated = c.clone();
    validated.validate()?;
    if !c.enabled {
        return Err(err("SEMANTIC_DISABLED", "语义检索未启用"));
    }
    if inputs.is_empty()
        || inputs.len() > 3
        || inputs
            .iter()
            .any(|s| s.trim().is_empty() || s.chars().count() > 1000)
    {
        return Err(err("SEMANTIC_LIMIT", "请求片段数量或文字超限"));
    }
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| err("SEMANTIC_NETWORK", "无法连接向量服务"))?;
    let mut req = client
        .post(c.url())
        .json(&json!({"model":c.model,"input":inputs,"encoding_format":"float"}));
    if !key.is_empty() {
        req = req.bearer_auth(key);
    }
    let response = req.send().map_err(|_| {
        err(
            "SEMANTIC_NETWORK",
            "向量服务连接失败或超时，精确检索仍可使用",
        )
    })?;
    if !response.status().is_success() {
        return Err(err(
            "SEMANTIC_SERVICE",
            match response.status().as_u16() {
                404 => "向量服务返回 HTTP 404：Embeddings 接口或向量模型未找到。请核对完整接口地址和已安装的向量模型；对话接口不能替代向量接口".to_string(),
                401 | 403 => "向量服务拒绝访问（HTTP 401/403），请核对此向量服务的密钥及访问权限".to_string(),
                code => format!("向量服务返回 HTTP {}，请核对服务状态及模型配置", code),
            },
        ));
    }
    let mut bytes = vec![];
    response
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| err("SEMANTIC_RESPONSE", "读取向量结果失败"))?;
    if bytes.len() > 1024 * 1024 {
        return Err(err("SEMANTIC_RESPONSE", "向量响应超过 1 MiB"));
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| err("SEMANTIC_RESPONSE", "服务返回格式无效"))?;
    parse_vectors(&value, inputs.len())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{fsops, search};
    use std::{fs, path::Path};
    struct F {
        d: tempfile::TempDir,
        s: Store,
    }
    impl F {
        fn new() -> Self {
            let d = tempfile::tempdir().unwrap();
            let s = Store::open(&d.path().join("library")).unwrap();
            s.search_control("enable").unwrap();
            Self { d, s }
        }
        fn add(&mut self, name: &str, body: &str) -> String {
            let p = self.d.path().join(name);
            fs::write(&p, body).unwrap();
            self.s.add_file(&fsops::inspect(&p).unwrap()).unwrap();
            let id: String = self
                .s
                .conn
                .query_row("SELECT id FROM files WHERE name=?", [name], |r| r.get(0))
                .unwrap();
            while let Some(w) = self.s.search_next().unwrap() {
                let t = search::read(&w);
                self.s.search_commit(&w, t).unwrap();
            }
            id
        }
        fn enable(&self) {
            self.s.semantic_save(&json!({"config":{"enabled":true,"endpoint":"http://127.0.0.1:9999/v1","model":"fixture","maxRequests":100,"minSimilarity":0.35},"consent":true})).unwrap();
        }
        fn cache(&mut self, id: &str, v: Vec<f32>) {
            let (_, works, c) = self.s.semantic_plan().unwrap();
            let w = works.into_iter().find(|w| w.id == id).unwrap();
            let g = self.s.semantic_begin().unwrap();
            self.s
                .semantic_commit(&w, &c, g, &vec![v; w.samples.len()])
                .unwrap();
        }
    }
    #[test]
    fn unchanged_sample_reuses_vectors_after_index_timestamp_changes() {
        let mut f = F::new();
        let id = f.add("same.txt", "学习资料");
        f.enable();
        f.cache(&id, vec![1., 0.]);
        f.s.conn
            .execute(
                "UPDATE search.docs SET updated_at=updated_at+100 WHERE id=?",
                [&id],
            )
            .unwrap();
        let (p, w, c) =
            f.s.semantic_plan_mode(Some(&[id.clone()]), "update")
                .unwrap();
        assert_eq!(p["requests"], 0);
        assert_eq!(p["reuse"], 1);
        assert_eq!(p["total"], 1);
        let g = f.s.semantic_begin().unwrap();
        assert!(f
            .s
            .semantic_commit(&w[0], &c, g, w[0].reuse.as_ref().unwrap())
            .unwrap());
        assert_eq!(f.s.semantic_ready(&c).unwrap(), 1);
    }
    #[test]
    fn content_changes_even_same_length_require_network_and_force_binds_preview() {
        let mut f = F::new();
        let id = f.add("change.txt", "学习资料");
        f.enable();
        f.cache(&id, vec![1., 0.]);
        f.s.conn
            .execute(
                "UPDATE search.docs SET body='室内风景',updated_at=updated_at+1 WHERE id=?",
                [&id],
            )
            .unwrap();
        let (p, w, _) =
            f.s.semantic_plan_mode(Some(&[id.clone()]), "update")
                .unwrap();
        assert_eq!(p["requests"], 1);
        assert!(w[0].reuse.is_none());
        let (missing, _, _) =
            f.s.semantic_plan_mode(Some(&[id.clone()]), "missing")
                .unwrap();
        assert_eq!(missing["total"], 0);
        let (force, w, _) = f.s.semantic_plan_mode(Some(&[id]), "rebuild").unwrap();
        assert_eq!(force["requests"], 1);
        assert!(w[0].reuse.is_none());
        assert_ne!(p["fingerprint"], force["fingerprint"]);
    }
    #[test]
    fn force_and_model_change_never_reuse_existing_vectors() {
        let mut f = F::new();
        let id = f.add("model.txt", "正文");
        f.enable();
        f.cache(&id, vec![1., 0.]);
        assert_eq!(
            f.s.semantic_plan_mode(Some(&[id.clone()]), "rebuild")
                .unwrap()
                .0["requests"],
            1
        );
        let mut c = f.s.semantic_config().unwrap();
        c.model = "another-model".into();
        f.s.semantic_save(&json!({"config":c,"consent":true}))
            .unwrap();
        let (p, w, _) = f.s.semantic_plan_for(Some(&[id])).unwrap();
        assert_eq!(p["requests"], 1);
        assert!(w[0].reuse.is_none());
    }
    #[test]
    fn pending_queue_survives_restart_and_running_is_interrupted() {
        let mut f = F::new();
        let id = f.add("queue.txt", "正文");
        f.enable();
        f.s.semantic_enqueue(&id, "import-1").unwrap();
        f.s.semantic_queue_state(&id, "running", "").unwrap();
        let root = f.s.root.clone();
        drop(f.s);
        f.s = Store::open(&root).unwrap();
        let q = f.s.semantic_queue().unwrap();
        assert_eq!(q["items"][0]["state"], "interrupted");
        assert_eq!(q["items"][0]["batch"], "import-1");
        assert_eq!(f.s.semantic_status().unwrap()["task"]["requested"], 0);
        f.s.semantic_clear().unwrap();
        assert_eq!(
            f.s.semantic_queue().unwrap()["items"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }
    #[test]
    fn queue_reports_unsupported_and_removed_files_do_not_remain_visible() {
        let mut f = F::new();
        let id = f.add("image.jpg", "synthetic");
        f.s.semantic_enqueue(&id, "import-2").unwrap();
        assert_eq!(
            f.s.semantic_queue().unwrap()["items"][0]["state"],
            "unsupported"
        );
        f.s.conn
            .execute("UPDATE files SET removed_at=1 WHERE id=?", [id])
            .unwrap();
        assert!(f.s.semantic_queue().unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty());
    }
    #[test]
    fn corrupt_cached_vector_is_not_reused_and_stale_reuse_commit_is_rejected() {
        let mut f = F::new();
        let id = f.add("stale.txt", "正文");
        f.enable();
        f.cache(&id, vec![1., 0.]);
        f.s.conn
            .execute(
                "UPDATE search.docs SET updated_at=updated_at+1 WHERE id=?",
                [&id],
            )
            .unwrap();
        let (_, w, c) = f.s.semantic_plan_for(Some(&[id.clone()])).unwrap();
        let g = f.s.semantic_begin().unwrap();
        f.s.conn
            .execute(
                "UPDATE search.docs SET updated_at=updated_at+1 WHERE id=?",
                [&id],
            )
            .unwrap();
        assert!(!f
            .s
            .semantic_commit(&w[0], &c, g, w[0].reuse.as_ref().unwrap())
            .unwrap());
        f.s.conn
            .execute(
                "UPDATE semantic.samples SET vector='[0,0]' WHERE file_id=?",
                [&id],
            )
            .unwrap();
        assert_eq!(f.s.semantic_plan_for(Some(&[id])).unwrap().0["requests"], 1);
    }
    #[test]
    fn disabled_connection_test_does_not_need_index_or_send() {
        assert!(connection_test(&Config::default(), "")
            .unwrap_err()
            .starts_with("SEMANTIC_DISABLED:"));
    }
    #[test]
    fn scoped_older_file_can_commit_and_remains_searchable_beyond_latest_500() {
        let mut f = F::new();
        let old = f.add("selected-old.txt", "选中的旧文件");
        for i in 0..501 {
            f.add(&format!("new-{i:03}.txt"), "较新的正文");
        }
        f.enable();
        let (_, work, c) = f.s.semantic_plan_for(Some(&[old.clone()])).unwrap();
        assert_eq!(work.len(), 1);
        let generation = f.s.semantic_begin().unwrap();
        assert!(f
            .s
            .semantic_commit(
                &work[0],
                &c,
                generation,
                &vec![vec![1., 0.]; work[0].samples.len()]
            )
            .unwrap());
        assert_eq!(f.s.semantic_ready(&c).unwrap(), 1);
        assert_eq!(f.s.semantic_rank(&c, &[1., 0.]).unwrap()[0].id, old);
    }
    #[test]
    fn selected_plan_never_includes_older_files_and_binds_scope() {
        let mut f = F::new();
        let old = f.add("older.txt", "旧文件正文");
        let new = f.add("new.txt", "新导入正文");
        f.enable();
        let (p, w, _) = f.s.semantic_plan_for(Some(&[new.clone()])).unwrap();
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].id, new);
        let (other, _, _) = f.s.semantic_plan_for(Some(&[old])).unwrap();
        assert_ne!(p["fingerprint"], other["fingerprint"]);
        assert!(f.s.semantic_plan_for(Some(&[])).is_err());
    }
    #[test]
    fn unified_semantic_fusion_retains_local_results_and_real_body_snippets() {
        let mut f = F::new();
        let meta = f.add("面试名称.txt", "普通内容");
        let body = f.add("正文.txt", "面试正文内容");
        let extra = f.add("语义补充.txt", "岗位问答准备");
        f.enable();
        f.cache(&body, vec![1., 0.]);
        f.cache(&extra, vec![1., 0.]);
        let hits =
            f.s.semantic_rank(&f.s.semantic_config().unwrap(), &[1., 0.])
                .unwrap();
        let q = Query {
            search_mode: "unified".into(),
            text: "面试".into(),
            sort: "relevance".into(),
            limit: 1,
            ..Default::default()
        };
        let a = f.s.semantic_result(&q, &hits).unwrap();
        assert_eq!(a["total"], 3);
        assert_eq!(a["files"][0]["id"], meta);
        let b =
            f.s.semantic_result(
                &Query {
                    offset: 1,
                    ..q.clone()
                },
                &hits,
            )
            .unwrap();
        assert_eq!(b["files"][0]["id"], body);
        assert_ne!(b["files"][0]["contentMatch"]["semantic"], true);
        assert_eq!(b["files"][0]["contentMatch"]["queryText"], "面试");
        let c =
            f.s.semantic_result(
                &Query {
                    offset: 2,
                    ..q.clone()
                },
                &hits,
            )
            .unwrap();
        assert_eq!(c["files"][0]["id"], extra);
        assert_eq!(c["files"][0]["contentMatch"]["semantic"], true);
        assert_eq!(
            f.s.semantic_result(
                &Query {
                    kinds: vec!["image".into()],
                    ..q.clone()
                },
                &hits
            )
            .unwrap()["total"],
            0
        );
        let control = Control::default();
        let session = control
            .remember(&f.s.semantic_config().unwrap(), "面试", hits)
            .unwrap();
        assert_eq!(
            control
                .resolve(
                    &f.s,
                    &Query {
                        semantic_session: session.clone(),
                        ..q.clone()
                    }
                )
                .unwrap()["total"],
            3
        );
        assert!(control
            .resolve(
                &f.s,
                &Query {
                    semantic_session: session,
                    text: "旅行".into(),
                    ..q
                }
            )
            .is_err());
    }
    #[test]
    fn endpoint_resolution_and_provider_validation() {
        for (base, expected) in [
            (
                "http://localhost:11434",
                "http://localhost:11434/v1/embeddings",
            ),
            ("https://example.com", "https://example.com/v1/embeddings"),
            (
                "https://example.com/v1/",
                "https://example.com/v1/embeddings",
            ),
            (
                "https://example.com/compatible-mode/v1",
                "https://example.com/compatible-mode/v1/embeddings",
            ),
            (
                "https://example.com/embeddings",
                "https://example.com/embeddings",
            ),
        ] {
            let mut c = Config {
                enabled: true,
                endpoint: base.into(),
                model: "fixture".into(),
                ..Default::default()
            };
            c.validate().unwrap();
            assert_eq!(c.url(), expected);
        }
        let mut c = Config {
            enabled: true,
            endpoint: "https://api.deepseek.com".into(),
            model: "deepseek-flash".into(),
            ..Default::default()
        };
        assert!(c.validate().unwrap_err().contains("DeepSeek"));
        c.enabled = false;
        assert!(c.validate().is_ok());
        for path in ["/chat/completions", "/responses", "/api/embed"] {
            c.endpoint = format!("https://example.com{path}");
            assert!(c.validate().is_err());
        }
    }
    #[test]
    fn disabled_is_offline_and_requires_explicit_consent() {
        let mut f = F::new();
        f.add("test.txt", "原有精确搜索");
        assert!(!f.s.semantic_config().unwrap().enabled);
        assert!(f.s.semantic_plan().is_err());
        let mut c = Config {
            enabled: true,
            endpoint: "http://example.com/v1".into(),
            model: "m".into(),
            ..Default::default()
        };
        assert!(c.validate().is_err());
        c.endpoint = "http://localhost:9999/v1".into();
        assert!(f.s.semantic_save(&json!({"config":c})).is_err());
        assert_eq!(
            f.s.query(&Query {
                text: "test".into(),
                limit: 100,
                ..Default::default()
            })
            .unwrap()["total"],
            1
        );
    }
    #[test]
    fn vectors_reject_zero_nonfinite_oversize() {
        assert!(normalized(&[]).is_err());
        assert!(normalized(&[0., 0.]).is_err());
        assert!(normalized(&[f32::NAN]).is_err());
        assert!(normalized(&vec![1.; 2049]).is_err());
        let v = normalized(&[3., 4.]).unwrap();
        assert!((v[0] - 0.6).abs() < 0.001);
    }
    #[test]
    fn response_orders_indices_and_rejects_duplicates() {
        let v = json!({"data":[{"index":1,"embedding":[0,1]},{"index":0,"embedding":[1,0]}]});
        assert_eq!(parse_vectors(&v, 2).unwrap()[0], vec![1., 0.]);
        assert!(parse_vectors(
            &json!({"data":[{"index":0,"embedding":[1]},{"index":0,"embedding":[1]}]}),
            2
        )
        .is_err());
        assert!(parse_vectors(
            &json!({"data":[{"index":0,"embedding":[1]},{"index":1,"embedding":[1,2]}]}),
            2
        )
        .is_err());
    }
    #[test]
    fn samples_preserve_unicode_positions_and_budget() {
        let body = "甲乙😀".repeat(1400);
        let samples = sample(&body, &[]);
        assert_eq!(samples.len(), 3);
        assert!(samples.iter().all(|s| s.text.chars().count() <= 1000));
        for s in samples {
            assert_eq!(
                body.chars().skip(s.start).take(1000).collect::<String>(),
                s.text
            );
        }
    }
    #[test]
    fn plan_hash_changes_and_has_explicit_bounded_range() {
        let mut f = F::new();
        f.add("论文.txt", "论文的总结");
        f.enable();
        let (p, _, _) = f.s.semantic_plan().unwrap();
        assert_eq!(p["requests"], 1);
        assert!(p["files"][0]["characters"].as_u64().unwrap() <= 3000);
        f.add("技术.txt", "搜索计划");
        assert_ne!(
            p["fingerprint"],
            f.s.semantic_plan().unwrap().0["fingerprint"]
        );
    }
    #[test]
    fn rank_reference_fixture_sources_and_reuses_session() {
        let mut f = F::new();
        let a = f.add("面试.txt", "Agent 岗位的面试准备和技术问题");
        let b = f.add("旅行.txt", "旅行攻略和交通住宿");
        f.enable();
        f.cache(&a, vec![1., 0.]);
        f.cache(&b, vec![0.6, 0.8]);
        let c = f.s.semantic_config().unwrap();
        let h = f.s.semantic_rank(&c, &[1., 0.]).unwrap();
        assert_eq!(h[0].id, a);
        let control = Control::default();
        let token = control.remember(&c, "岗位准备", h).unwrap();
        let mut q = Query {
            text: "岗位准备".into(),
            search_mode: "semantic".into(),
            semantic_session: token,
            limit: 1,
            ..Default::default()
        };
        let r = control.resolve(&f.s, &q).unwrap();
        assert_eq!(r["total"], 2);
        assert_eq!(r["files"][0]["id"], a);
        assert_eq!(r["files"][0]["contentMatch"]["semantic"], true);
        q.offset = 1;
        assert_eq!(control.resolve(&f.s, &q).unwrap()["files"][0]["id"], b);
        control.invalidate();
        assert!(control.resolve(&f.s, &q).is_err());
    }
    #[test]
    fn semantic_respects_filters_and_clear_does_not_touch_original() {
        let mut f = F::new();
        let a = f.add("a.txt", "参考内容");
        f.enable();
        f.cache(&a, vec![1., 0.]);
        let c = f.s.semantic_config().unwrap();
        let h = f.s.semantic_rank(&c, &[1., 0.]).unwrap();
        let q = Query {
            kinds: vec!["image".into()],
            limit: 100,
            ..Default::default()
        };
        assert_eq!(f.s.semantic_result(&q, &h).unwrap()["total"], 0);
        let before = json!(f.s.file(&a).unwrap());
        f.s.semantic_clear().unwrap();
        assert_eq!(json!(f.s.file(&a).unwrap()), before);
        assert!(f.s.search_enabled().unwrap());
        assert_eq!(
            fs::read_to_string(f.d.path().join("a.txt")).unwrap(),
            "参考内容"
        );
    }
    #[test]
    fn late_cancelled_and_modified_source_vectors_do_not_commit() {
        let mut f = F::new();
        let a = f.add("a.txt", "旧内容");
        f.enable();
        let (_, w, c) = f.s.semantic_plan().unwrap();
        let g = f.s.semantic_begin().unwrap();
        f.s.semantic_clear().unwrap();
        assert!(!f.s.semantic_commit(&w[0], &c, g, &[vec![1., 0.]]).unwrap());
        let g = f.s.semantic_begin().unwrap();
        fs::write(f.d.path().join("a.txt"), "新内容现在更长").unwrap();
        assert!(!f.s.semantic_commit(&w[0], &c, g, &[vec![1., 0.]]).unwrap());
        assert!(!f.s.semantic_file_current(&a).unwrap());
    }
    #[test]
    fn model_changes_invalidate_cache_and_index_revision_excludes_hits() {
        let mut f = F::new();
        let a = f.add("a.txt", "旧的文本");
        f.enable();
        f.cache(&a, vec![1., 0.]);
        let c = f.s.semantic_config().unwrap();
        assert_eq!(f.s.semantic_ready(&c).unwrap(), 1);
        let mut changed = c.clone();
        changed.model = "other".into();
        f.s.semantic_save(&json!({"config":changed,"consent":true}))
            .unwrap();
        assert_eq!(f.s.semantic_ready(&changed).unwrap(), 0);
        f.s.conn
            .execute("UPDATE files SET revision='changed' WHERE id=?", [a])
            .unwrap();
        assert!(f.s.semantic_rank(&c, &[1., 0.]).unwrap().is_empty());
    }
    #[test]
    fn restart_marks_running_interrupted_and_retains_completed() {
        let mut f = F::new();
        let a = f.add("a.txt", "已有内容");
        f.enable();
        f.cache(&a, vec![1., 0.]);
        let root = f.s.root.clone();
        drop(f.s);
        let s = Store::open(&root).unwrap();
        assert_eq!(s.semantic_status().unwrap()["task"]["state"], "interrupted");
        assert_eq!(s.semantic_status().unwrap()["files"], 1);
    }
    #[test]
    fn backup_excludes_credentials_config_and_vectors() {
        let mut f = F::new();
        let a = f.add("a.txt", "备份保留内容");
        f.enable();
        f.cache(&a, vec![1., 0.]);
        let backup = f.d.path().join("metadata.sqtagbackup");
        f.s.export_backup(&backup).unwrap();
        let target = f.d.path().join("restored");
        let mut s = Store::open(&target).unwrap();
        s.restore_backup(Path::new(&backup)).unwrap();
        assert!(!s.semantic_config().unwrap().enabled);
        assert_eq!(s.semantic_status().unwrap()["files"], 0);
        assert_eq!(
            s.query(&Query {
                limit: 100,
                ..Default::default()
            })
            .unwrap()["total"],
            1
        );
        assert!(!target.join("semantic-key.dpapi").exists());
    }
    #[test]
    fn natural_alias_and_exclusion_resolve_canonical() {
        let mut f = F::new();
        let t = f.s.create_tag("参考素材").unwrap();
        let mut v = json!({"id":t.id,"name":t.name,"version":t.version,"aliases":["学习资料"]});
        v["preview"] = f.s.aliases_preview(&v).unwrap();
        f.s.aliases_update(&v).unwrap();
        let r =
            f.s.natural_preview(&json!({"text":"最近一周 PDF，排除「学习资料」"}))
                .unwrap();
        assert_eq!(r["query"]["exclude"][0], t.id);
        assert_eq!(r["query"]["include"], json!([]));
    }
    #[test]
    fn quoted_tag_words_do_not_invent_types_dates_or_scope() {
        let mut f = F::new();
        let t = f.s.create_tag("收藏PDF最近7天").unwrap();
        let r =
            f.s.natural_preview(&json!({"text":"包含「收藏PDF最近7天」"}))
                .unwrap();
        assert_eq!(r["query"]["scope"], "all");
        assert_eq!(r["query"]["kinds"], json!([]));
        assert!(r["query"]["from"].is_null());
        assert_eq!(r["query"]["include"][0], t.id);
        assert!(f
            .s
            .natural_preview(&json!({"text":"包含「收藏PDF最近7天」，排除「收藏PDF最近7天」"}))
            .is_err());
    }
    #[test]
    fn damaged_derived_cache_recovers_without_losing_annotations() {
        let mut f = F::new();
        let a = f.add("a.txt", "原文件正文");
        let t = f.s.create_tag("保留").unwrap();
        let root = f.s.root.clone();
        drop(f.s);
        fs::write(
            root.join("cache/semantic/index.sqlite"),
            b"broken derived cache",
        )
        .unwrap();
        let s = Store::open(&root).unwrap();
        assert_eq!(s.file(&a).unwrap().name, "a.txt");
        assert!(s.tags().unwrap().iter().any(|x| x.id == t.id));
        assert_eq!(s.semantic_status().unwrap()["files"], 0);
    }
    #[test]
    fn modified_source_is_hidden_from_existing_rank_session() {
        let mut f = F::new();
        let a = f.add("a.txt", "旧正文");
        f.enable();
        f.cache(&a, vec![1., 0.]);
        let c = f.s.semantic_config().unwrap();
        let hits = f.s.semantic_rank(&c, &[1., 0.]).unwrap();
        fs::write(
            f.d.path().join("a.txt"),
            "更新后的新正文，不应继续引用旧片段",
        )
        .unwrap();
        let r =
            f.s.semantic_result(
                &Query {
                    limit: 100,
                    ..Default::default()
                },
                &hits,
            )
            .unwrap();
        assert_eq!(r["total"], 0);
        assert_eq!(r["files"], json!([]));
    }
}
