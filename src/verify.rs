use std::collections::HashSet;
use std::path::Path;

use crate::anchor::{self, AnchorHash, AnchorIndex};
use crate::error::{Error, Result};
use crate::hashing::Algorithm;
use crate::jsonio;
use crate::pool::{self, Job};
use crate::report::{self, Row};
use crate::walk;

pub struct Outcome {
    /// 非通过项与多余文件之和, 决定退出码
    pub failed: usize,
}

/// 集合级比对之后, 每个 anchor 条目的去向
enum Slot {
    /// 大小一致, 需要算哈希; 值是 jobs 里的下标
    Hash(usize),
    /// 集合级就已判定不通过
    Broken,
}

pub fn run(root: &Path) -> Result<Outcome> {
    let index_path = anchor::index_path(root);
    let index: AnchorIndex = jsonio::load(&index_path)?.ok_or_else(|| {
        Error::Data(format!("找不到 {}, 请先运行 generate", index_path.display()))
    })?;
    let hash_path = root.join(&index.anchor_hash);
    let anchor_hash: AnchorHash =
        jsonio::load(&hash_path)?.ok_or_else(|| Error::Data(format!("找不到 {}", hash_path.display())))?;

    println!("INFO: Start quick check");
    println!("INFO: Model_id: {}", index.source.model_id);
    println!("INFO: Load anchor index");
    println!("INFO: Files: {}", anchor_hash.files.len());
    println!();

    // 1 集合级比对, 只 stat 不读字节
    let mut slots: Vec<Slot> = Vec::with_capacity(anchor_hash.files.len());
    let mut jobs: Vec<Job> = Vec::new();
    let mut broken_rows: Vec<Row> = Vec::new();

    for f in &anchor_hash.files {
        let path = root.join(&f.name);
        match std::fs::metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                broken_rows.push(Row::new("[MISSING]", &f.name));
                slots.push(Slot::Broken);
            }
            Err(e) => {
                broken_rows.push(Row::with_detail(
                    "[UNREADABLE]",
                    &f.name,
                    vec![format!("reason: {e}")],
                ));
                slots.push(Slot::Broken);
            }
            Ok(m) if !m.is_file() => {
                broken_rows.push(Row::with_detail(
                    "[UNREADABLE]",
                    &f.name,
                    vec!["reason: not a regular file".to_string()],
                ));
                slots.push(Slot::Broken);
            }
            Ok(m) if m.len() != f.size => {
                broken_rows.push(Row::with_detail(
                    "[SIZE-MISMATCH]",
                    &f.name,
                    vec![
                        format!("expected: {}", f.size),
                        format!("actual:   {}", m.len()),
                    ],
                ));
                slots.push(Slot::Broken);
            }
            Ok(_) => {
                let job = jobs.len();
                jobs.push(Job {
                    path,
                    size: f.size,
                });
                slots.push(Slot::Hash(job));
            }
        }
    }

    // 本地多出来的文件
    let expected: HashSet<&str> = anchor_hash.files.iter().map(|f| f.name.as_str()).collect();
    let actual = walk::walk(root, &index.excluded)?;
    let mut added_rows: Vec<Row> = Vec::new();
    for entry in &actual {
        if !expected.contains(entry.name.as_str()) {
            added_rows.push(Row::with_detail(
                "[ADDED]",
                &entry.name,
                vec![format!("size: {}", entry.size)],
            ));
        }
    }

    let broken = slots
        .iter()
        .filter(|s| matches!(s, Slot::Broken))
        .count();
    let added = added_rows.len();
    let problems = broken + added;

    if !broken_rows.is_empty() || !added_rows.is_empty() {
        report::print_rows(&broken_rows);
        report::print_rows(&added_rows);
        println!();
    }
    if problems > 0 {
        println!("INFO: Quick check found {problems} problem(s), start verification");
    } else {
        println!("INFO: Quick check passed, start verification");
    }
    println!();

    // 2 只对大小一致的文件算 BLAKE3
    let digests = pool::hash_all(&jobs, Algorithm::Blake3);

    // 3 逐条输出, 按 name 顺序
    let mut rows: Vec<Row> = Vec::new();
    let mut passed = 0usize;
    let mut hash_failed = 0usize;
    for (f, slot) in anchor_hash.files.iter().zip(slots.iter()) {
        let job = match slot {
            Slot::Hash(i) => *i,
            Slot::Broken => continue,
        };
        match &digests[job] {
            Some(hex) if *hex == f.blake3 => {
                rows.push(Row::new("[OK]", &f.name));
                passed += 1;
            }
            Some(hex) => {
                rows.push(Row::with_detail(
                    "[FAIL]",
                    &f.name,
                    vec![
                        format!("expected: {}", f.blake3),
                        format!("actual:   {hex}"),
                    ],
                ));
                hash_failed += 1;
            }
            None => {
                rows.push(Row::with_detail(
                    "[UNREADABLE]",
                    &f.name,
                    vec!["reason: read failed".to_string()],
                ));
                hash_failed += 1;
            }
        }
    }
    report::print_rows(&rows);

    let total = anchor_hash.files.len();
    let failed = broken + hash_failed;

    println!();
    println!("Files: {total}");
    println!("Passed: {passed}");
    println!("Failed: {failed}");
    if added > 0 {
        println!("Added: {added}");
    }
    println!();
    println!(
        "Result: {}",
        if failed == 0 && added == 0 { "PASS" } else { "FAIL" }
    );
    println!();
    println!("INFO: Anchor: {}", index_path.display());

    Ok(Outcome {
        failed: failed + added,
    })
}
