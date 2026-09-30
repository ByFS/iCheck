use std::path::Path;

use crate::anchor::{self, AnchorFile, AnchorHash, AnchorIndex, Summary};
use crate::error::{Error, Result};
use crate::hashing::Algorithm;
use crate::jsonio;
use crate::official::{self, CheckState, FileEntry, OfficialHash};
use crate::pool::{self, Job};
use crate::report::Row;
use crate::walk;

pub struct Outcome {
    /// 非 pass 的条目数, 决定退出码
    pub failed: usize,
}

/// -f 时最多列出这么多条未通过项, 避免刷屏
const FORCE_LIST_LIMIT: usize = 20;

pub fn run(root: &Path, force: bool) -> Result<Outcome> {
    let official_path = anchor::official_path(root);
    let official: OfficialHash = jsonio::load(&official_path)?.ok_or_else(|| {
        Error::Data(format!("找不到 {}, 请先运行 check", official_path.display()))
    })?;

    println!(
        "INFO: Source: {}",
        crate::upstream::Platform::display_for(&official.source.platform)
    );
    println!("INFO: Model_id: {}", official.source.model_id);

    // 前置: 官方校验必须全部通过, 否则拒绝建立锚点
    // 锚点的含义是"这个目录是经过官方校验的快照", 锚一个残缺集合会让它语义变浑
    // -f 只放开这道前置, 绝不改写 official_hash 里的状态
    let bad: Vec<&FileEntry> = official
        .files
        .iter()
        .filter(|f| f.check != CheckState::Pass)
        .collect();
    if !bad.is_empty() {
        if !force {
            let rows: Vec<Row> = bad.iter().map(|f| Row::new(f.check.tag(), &f.name)).collect();
            println!("INFO: Checking model");
            println!();
            crate::report::print_rows(&rows);
            println!();
            println!("Files: {}", official.files.len());
            println!("Passed: {}", official.files.len() - bad.len());
            println!("Failed: {}", bad.len());
            println!();
            println!("Result: FAIL");
            println!();
            println!("INFO: Refuse to anchor: official check has not fully passed");
            println!("      Re-run with -f to anchor anyway");
            return Ok(Outcome { failed: bad.len() });
        }

        let count = |s: CheckState| bad.iter().filter(|f| f.check == s).count();
        println!();
        println!(
            "WARN: -f, skipping the precondition: {} entry(ies) did not pass the official check",
            bad.len()
        );
        println!(
            "      missing:    {} (not on disk, will NOT be covered by the anchor, so verify cannot catch them)",
            count(CheckState::Missing)
        );
        println!(
            "      fail:       {} (anchored as-is, without official backing)",
            count(CheckState::Fail)
        );
        println!(
            "      unreadable: {} (may also fail to hash)",
            count(CheckState::Unreadable)
        );
        println!();
        let shown = bad.len().min(FORCE_LIST_LIMIT);
        let rows: Vec<Row> = bad[..shown]
            .iter()
            .map(|f| Row::new(f.check.tag(), &f.name))
            .collect();
        crate::report::print_rows(&rows);
        if shown < bad.len() {
            println!("        ... {} more", bad.len() - shown);
        }
    }

    // 遍历本地目录
    let excluded = walk::default_excluded();
    let walked = walk::walk(root, &excluded)?;
    let total_bytes: u64 = walked.iter().map(|e| e.size).sum();

    println!("INFO: Files: {} (local)", walked.len());
    println!(
        "INFO: Computing {} ({} file(s), {:.2} GB)",
        Algorithm::Blake3.name(),
        walked.len(),
        total_bytes as f64 / 1e9
    );
    println!();

    let jobs: Vec<Job> = walked
        .iter()
        .map(|e| Job {
            path: root.join(&e.name),
            size: e.size,
        })
        .collect();
    let digests = pool::hash_all(&jobs, Algorithm::Blake3);

    let mut files: Vec<AnchorFile> = Vec::new();
    let mut unreadable: Vec<String> = Vec::new();
    for (entry, digest) in walked.iter().zip(digests) {
        match digest {
            Some(blake3) => files.push(AnchorFile {
                name: entry.name.clone(),
                size: entry.size,
                blake3,
            }),
            None => unreadable.push(entry.name.clone()),
        }
    }

    // cli.md 的格式: 每个文件三行
    for f in &files {
        println!("INFO: name: {}", f.name);
        println!("INFO: BLAKE3: {}", f.blake3);
        println!("INFO: size: {}", f.size);
    }
    for name in &unreadable {
        println!("[UNREADABLE] {name}");
    }

    let now = crate::now_rfc3339();

    // 先写 anchor_hash
    let anchor_hash = AnchorHash {
        tool: official::TOOL.to_string(),
        computed_at: now.clone(),
        files,
    };
    jsonio::save(&anchor::hash_path(root), &anchor_hash)?;

    // 最后写 anchor_index, 它是提交点
    let previous: Option<AnchorIndex> = jsonio::load(&anchor::index_path(root))?;
    let created_at = previous
        .map(|p| p.created_at)
        .unwrap_or_else(|| now.clone());
    let index = AnchorIndex {
        tool: official::TOOL.to_string(),
        created_at,
        updated_at: now,
        official_hash: anchor::OFFICIAL_REL.to_string(),
        anchor_hash: anchor::ANCHOR_HASH_REL.to_string(),
        excluded,
        source: official.source.clone(),
        summary: Summary {
            total_files: anchor_hash.files.len(),
            official_files: official.files.len(),
            blake3_files: anchor_hash.files.len(),
        },
    };
    jsonio::save(&anchor::index_path(root), &index)?;

    let failed = unreadable.len();
    println!();
    println!("Files: {}", anchor_hash.files.len() + failed);
    println!("Anchored: {}", anchor_hash.files.len());
    println!("Failed: {failed}");
    if force && !bad.is_empty() {
        println!("Forced: {} (no official backing)", bad.len());
    }
    println!();
    println!("Result: {}", if failed == 0 { "PASS" } else { "FAIL" });
    println!();
    println!("INFO: Anchor: {}", anchor::index_path(root).display());

    Ok(Outcome { failed })
}
