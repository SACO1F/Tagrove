//! Repeatable production Store benchmark. Synthetic metadata, not thumbnail/OS cold-start timing.
use crate::{db::Store, model::*, watch};
use std::{path::Path, time::Instant};

#[test]
fn unchanged_refresh_does_not_write_or_change_annotations() {
    let d = crate::fsops::TmpDir::new();
    let mut s = Store::open(&d.path().join("library")).unwrap();
    let p = d.path().join("稳定.txt");
    std::fs::write(&p, b"stable").unwrap();
    s.add_file_with_ai(&crate::fsops::inspect(&p).unwrap(), false)
        .unwrap();
    let id: String = s
        .conn
        .query_row("SELECT id FROM files", [], |r| r.get(0))
        .unwrap();
    let before = serde_json::to_value(s.file(&id).unwrap()).unwrap();
    let writes: i64 = s
        .conn
        .query_row("SELECT total_changes()", [], |r| r.get(0))
        .unwrap();
    for _ in 0..3 {
        assert!(!s.refresh_metadata(&id).unwrap());
    }
    let after: i64 = s
        .conn
        .query_row("SELECT total_changes()", [], |r| r.get(0))
        .unwrap();
    assert_eq!(writes, after);
    assert_eq!(
        serde_json::to_value(s.refresh_file(&id).unwrap()).unwrap(),
        before
    );
    std::fs::remove_file(&p).unwrap();
    assert!(s.refresh_metadata(&id).unwrap());
    assert!(!s.refresh_metadata(&id).unwrap());
    assert_eq!(s.file(&id).unwrap().status, "missing");
    assert_eq!(
        s.file(&id).unwrap().tags.len(),
        before["tags"].as_array().unwrap().len()
    );
}

#[test]
fn batch_query_preserves_tags_sources_ai_and_pagination() {
    let d = crate::fsops::TmpDir::new();
    let mut s = Store::open(&d.path().join("library")).unwrap();
    s.save_settings(&serde_json::json!({"key":"folderAutoTagging","value":false}))
        .unwrap();
    let tag = s.create_tag("人工").unwrap();
    let mut ids = vec![];
    for i in 0..3 {
        let p = d.path().join(format!("文件{i}.txt"));
        std::fs::write(&p, b"synthetic").unwrap();
        s.add_file_with_ai(&crate::fsops::inspect(&p).unwrap(), false)
            .unwrap();
        let fid: String = s
            .conn
            .query_row(
                "SELECT id FROM files WHERE path=?",
                [crate::fsops::canonical_display(&p).unwrap()],
                |r| r.get(0),
            )
            .unwrap();
        s.conn.execute("INSERT INTO file_tags(file_id,tag_id,source,ai_meta) VALUES(?,?,'ai',?)",rusqlite::params![fid,tag.id,"{\"confirmed\":true,\"model\":\"fixture\",\"reason\":\"测试\",\"updatedAt\":1}"]).unwrap();
        s.conn.execute("INSERT INTO ai_jobs(file_id,status,error,updated_at) VALUES(?,'failed','合成错误',5)",[&fid]).unwrap();
        ids.push(fid);
    }
    let q = Query {
        limit: 2,
        sort: "name".into(),
        direction: "asc".into(),
        ..Default::default()
    };
    let page = s.query(&q).unwrap();
    assert_eq!(page["total"], 3);
    assert_eq!(page["hasMore"], true);
    for f in page["files"].as_array().unwrap() {
        assert_eq!(
            f,
            &serde_json::to_value(s.file(f["id"].as_str().unwrap()).unwrap()).unwrap()
        );
    }
    let tail = s.query(&Query { offset: 2, ..q }).unwrap();
    assert_eq!(tail["hasMore"], false);
    assert_eq!(tail["files"].as_array().unwrap().len(), 1);
    s.conn
        .execute("UPDATE files SET removed_at=1 WHERE id=?", [&ids[0]])
        .unwrap();
    assert_eq!(s.tags().unwrap()[0].count, 2);
    assert!(s.tags().unwrap()[0].accepted);
    crate::db::ensure_performance_indexes(&s.conn).unwrap();
    s.validate().unwrap();
}

#[test]
fn indexed_watch_candidates_match_component_scan() {
    let d = crate::fsops::TmpDir::new();
    let mut s = Store::open(&d.path().join("library")).unwrap();
    let base = d.path().join("关注目录");
    for rel in [
        "关注目录/a.txt",
        "关注目录/子目录/b.txt",
        "关注目录/跳过.tmp",
        "关注目录2/c.txt",
        "Ärea/d.txt",
    ] {
        let p = d.path().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b"synthetic").unwrap();
        s.add_file_with_ai(&crate::fsops::inspect(&p).unwrap(), false)
            .unwrap();
    }
    s.conn
        .execute("UPDATE files SET removed_at=1 WHERE name='a.txt'", [])
        .unwrap();
    let mut rules = vec![];
    for recursive in [true, false] {
        rules.push(watch::Rule {
            id: "test".into(),
            path: base.to_string_lossy().into(),
            recursive,
            ignore: vec!["*.tmp".into()],
            ai: false,
            paused: false,
        });
    }
    rules.push(watch::Rule {
        id: "unicode".into(),
        path: d.path().join("ärea").to_string_lossy().into(),
        recursive: true,
        ignore: vec![],
        ai: false,
        paused: false,
    });
    rules.push(watch::Rule {
        id: "root".into(),
        path: if cfg!(windows) {
            "C:\\".into()
        } else {
            "/".into()
        },
        recursive: false,
        ignore: vec![],
        ai: false,
        paused: false,
    });
    for r in rules {
        let mut stmt = s.conn.prepare("SELECT id,path FROM files").unwrap();
        let all = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .unwrap();
        let mut expected: Vec<_> = all
            .map(|r| r.unwrap())
            .filter(|(_, p)| r.includes(Path::new(p)))
            .map(|(id, _)| id)
            .collect();
        expected.sort();
        let mut actual: Vec<_> = watch::known(&s, &r)
            .unwrap()
            .into_iter()
            .map(|k| k.id)
            .collect();
        actual.sort();
        assert_eq!(actual, expected);
    }
}

#[test]
#[ignore = "Explicit 10k/50k performance measurement; run with --ignored --nocapture"]
fn large_library_performance() {
    for n in std::env::var("SHIQIAN_BENCH_SIZES")
        .unwrap_or_else(|_| "1000,10000".into())
        .split(',')
        .map(|v| v.parse::<usize>().expect("integer benchmark size"))
    {
        let dir = crate::fsops::TmpDir::new();
        let mut s = Store::open(&dir.path().join("library")).unwrap();
        let tags: Vec<_> = (0..40)
            .map(|i| s.create_tag(&format!("项目{i}")).unwrap())
            .collect();
        let root = dir.path().join("素材");
        let target = root.join("关注目录");
        let tx = s.conn.transaction().unwrap();
        for i in 0..n {
            let parent = if i < 100 {
                target.clone()
            } else {
                root.join(format!("目录{}", i % 100))
            };
            let name = format!("资料{i} 产品设计.txt");
            let path = parent.join(&name).to_string_lossy().to_string();
            let fid = format!("perf-{i:06}");
            tx.execute("INSERT INTO files(id,path,path_key,parent,name,name_key,extension,kind,bytes,modified,added,revision,note,note_key) VALUES(?,?,?,?,?,?,'txt','text',1024,?,?, 'rev',?,?)", rusqlite::params![fid,path,path,parent.to_string_lossy(),name,key(&name),i,i,"客户反馈 项目调研","客户反馈 项目调研"]).unwrap();
            for j in 0..3 {
                tx.execute(
                    "INSERT INTO file_tags(file_id,tag_id) VALUES(?,?)",
                    rusqlite::params![fid, tags[(i + j * 7) % 40].id],
                )
                .unwrap();
            }
        }
        tx.commit().unwrap();
        for case in ["page", "deep_page", "search", "tag", "bootstrap", "watch"] {
            let mut times = vec![];
            for k in 0..41 {
                let q = Query {
                    limit: 100,
                    direction: "desc".into(),
                    offset: if case == "deep_page" { n - 100 } else { 0 },
                    text: if case == "search" {
                        "产品 客户反馈".into()
                    } else {
                        String::new()
                    },
                    include: if case == "tag" {
                        vec![tags[0].id.clone()]
                    } else {
                        vec![]
                    },
                    ..Default::default()
                };
                let t = Instant::now();
                if case == "bootstrap" {
                    s.bootstrap().unwrap();
                } else if case == "watch" {
                    let rule = watch::Rule {
                        id: "bench".into(),
                        path: target.to_string_lossy().into(),
                        recursive: true,
                        ignore: vec![],
                        ai: false,
                        paused: false,
                    };
                    assert_eq!(watch::known(&s, &rule).unwrap().len(), 100);
                } else {
                    let value = s.query(&q).unwrap();
                    assert_eq!(
                        value["files"].as_array().unwrap().len(),
                        100.min(value["total"].as_u64().unwrap() as usize)
                    );
                    assert_eq!(value["files"][0]["tags"].as_array().unwrap().len(), 3);
                }
                let elapsed = t.elapsed().as_secs_f64() * 1000.;
                if k == 0 {
                    println!("PERF first n={n} case={case} ms={elapsed:.3}");
                } else {
                    times.push(elapsed);
                }
            }
            times.sort_by(f64::total_cmp);
            println!(
                "PERF warm n={n} case={case} p50={:.3} p95={:.3} max={:.3}",
                times[20], times[38], times[39]
            );
        }
        s.validate().unwrap();
        assert!(Path::new(&s.root).is_dir());
    }
}
