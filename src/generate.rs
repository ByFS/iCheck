use std::path::Path;

use crate::anchor::{self, AnchorFile, AnchorHash, AnchorIndex, Summary};
use crate::error::{Error, Result};
use crate::hashing::Algorithm;
use crate::jsonio;
use crate::official::{CheckState, FileEntry, OfficialHash};
use crate::pool::{self, Job};
use crate::report::Row;
use crate::walk;

pub struct Outcome {
    /// 非 pass 的条目数, 决定退出码
    pub failed: usize,
}

pub fn run(root: &Path, force: bool) -> Result<Outcome> {
    let official_path = anchor::official_path(root);
    let official: OfficialHash = jsonio::load(&official_path)?.ok_or_else(|| {
        Error::Data(format!("{} not found, run check first", official_path.display()))
    })?;

    println!(
        "INFO: Source: {}",
        crate::upstream::Platform::display_for(&official.source.platform)
    );
    println!("INFO: Model ID: {}", official.source.model_id);

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

        // -f 已经是在明确接受不完美的前提下强行建锚点, 不再展开统计与逐条清单
        println!();
        println!(
            "WARN: -f, skipping the official check for {} entry(ies)",
            bad.len()
        );
    }

    // 遍历本地目录
    let excluded = walk::default_excluded();
    let walked = walk::walk(root, &excluded)?;
    let total_bytes: u64 = walked.iter().map(|e| e.size).sum();

    println!("INFO: Files: {} (local directory)", walked.len());
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

    // 只在有问题时逐条列出, 算过的文件不刷屏 —— 清单与哈希都在 JSON 里
    if !unreadable.is_empty() {
        let rows: Vec<Row> = unreadable
            .iter()
            .map(|name| Row::new("[UNREADABLE]", name))
            .collect();
        crate::report::print_rows(&rows);
    }

    let now = crate::now_rfc3339();

    // 先写 anchor_hash
    let anchor_hash = AnchorHash {
        tool: crate::TOOL.to_string(),
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
        tool: crate::TOOL.to_string(),
        created_at,
        updated_at: now,
        official_hash: anchor::OFFICIAL_REL.to_string(),
        anchor_hash: anchor::ANCHOR_HASH_REL.to_string(),
        excluded,
        source: official.source.clone(),
        summary: Summary {
            total_files: anchor_hash.files.len(),
            // 只有 pass 的条目才算有官方背书: fail 的字节与官方不符, missing 的没被锚定
            official_files: official.count(CheckState::Pass),
            blake3_files: anchor_hash.files.len(),
        },
    };
    jsonio::save(&anchor::index_path(root), &index)?;

    let failed = unreadable.len();
    let mut pairs: Vec<(&str, String)> = vec![
        ("Files", (anchor_hash.files.len() + failed).to_string()),
        ("Passed", anchor_hash.files.len().to_string()),
        ("Failed", failed.to_string()),
    ];
    pairs.push(("Anchor", anchor::index_path(root).display().to_string()));

    crate::report::print_summary(&pairs, if failed == 0 { "PASS" } else { "FAIL" });

    Ok(Outcome { failed })
}
