use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use crate::gate::{self, Gate, Governor};
use crate::hashing::{self, Algorithm};
use crate::jsonio;
use crate::official::{self, CheckState, FileEntry, OfficialHash};
use crate::report::{self, Row};
use crate::upstream::{self, Manifest, Platform};

/// 最多丢一个窗口的进度
const FLUSH_EVERY: usize = 32;
const FLUSH_INTERVAL: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(200);

pub struct Outcome {
    /// 非 pass 的条目数, 决定退出码
    pub failed: usize,
    /// 其中"内容哈希不符"的条数, 用于与"仅集合级差异"区分退出码
    pub hash_mismatch: usize,
}

struct Task {
    idx: usize,
    path: PathBuf,
    size: u64,
    sha256: String,
}

/// 单个文件的判定结果
struct Verdict {
    state: CheckState,
    /// 只在非 pass 时给, 逐行展开在结果块里的文件行下面
    detail: Vec<String>,
    /// 这次失败是"内容哈希不符"而不是"大小不符 / 读不了"
    hash_mismatch: bool,
}

impl Verdict {
    fn plain(state: CheckState) -> Self {
        Verdict {
            state,
            detail: Vec::new(),
            hash_mismatch: false,
        }
    }

    fn with_detail(state: CheckState, detail: Vec<String>) -> Self {
        Verdict {
            state,
            detail,
            hash_mismatch: false,
        }
    }

    fn mismatch(detail: Vec<String>) -> Self {
        Verdict {
            state: CheckState::Fail,
            detail,
            hash_mismatch: true,
        }
    }
}

pub fn run(root: &Path, platform: Platform, model_id: &str, revision: Option<&str>) -> Result<Outcome> {
    let state_path = official::path_in(root);

    println!("INFO: Source: {}", platform.display_name());
    println!("INFO: Model ID: {model_id}");
    println!("INFO: Obtain model information");

    let manifest = upstream::fetch(platform, model_id, revision)?;

    println!("INFO: Files: {} (upstream manifest)", manifest.entries.len());
    println!("INFO: Start obtaining information");

    let prev: Option<OfficialHash> = jsonio::load(&state_path)?;

    // 续跑只在上次中断时成立
    // 上次跑完了再跑就是一次新的校验, 一律从新快照重来
    // 否则会在文件被改过之后谎报 PASS
    let resuming = prev.as_ref().is_some_and(|p| !p.is_complete());

    let mut state = if resuming {
        let p = prev.as_ref().unwrap();
        // 跨快照不能续跑: 已记录的 pass/fail 属于旧快照, 续下去没有意义
        verify_same_snapshot(p, &manifest)?;
        let mut s = OfficialHash::new(platform, model_id, &manifest.entries);
        let old: HashMap<&str, CheckState> =
            p.files.iter().map(|f| (f.name.as_str(), f.check)).collect();
        for f in s.files.iter_mut() {
            if let Some(c) = old.get(f.name.as_str()) {
                f.check = *c;
            }
        }
        // 快照没变, 保留首次抓取时间作为溯源地
        s.fetched_at = p.fetched_at.clone();
        s
    } else {
        OfficialHash::new(platform, model_id, &manifest.entries)
    };

    println!();
    for f in &state.files {
        println!("INFO: name: {}", f.name);
        println!("INFO: size: {}", f.size);
        println!("INFO: sha256: {}", f.sha256);
    }

    // 建任务: 续跑时只有 pass 且本地大小仍与记录一致的才跳过, 其余一律重算
    let mut tasks = Vec::new();
    let mut skipped = 0usize;
    for (idx, f) in state.files.iter().enumerate() {
        let local = root.join(&f.name);
        let local_size = std::fs::metadata(&local).ok().map(|m| m.len());
        if f.check == CheckState::Pass && local_size == Some(f.size) {
            skipped += 1;
            continue;
        }
        tasks.push(Task {
            idx,
            path: local,
            size: f.size,
            sha256: f.sha256.clone(),
        });
    }

    let total_bytes: u64 = tasks.iter().map(|t| t.size).sum();

    println!();
    println!("INFO: Checking model");
    if skipped > 0 {
        println!(
            "INFO: Resume: {skipped} file(s) already passed, {} to verify",
            tasks.len()
        );
    }
    println!();

    let mut details: HashMap<usize, Vec<String>> = HashMap::new();
    let mut hash_mismatch = 0usize;
    if !tasks.is_empty() {
        hash_mismatch = run_tasks(&mut state, &tasks, total_bytes, &state_path, &mut details)?;
    }

    let failed = report(&state, &details, &state_path);

    Ok(Outcome {
        failed,
        hash_mismatch,
    })
}

/// 结果报告, 跳过重算的路径和正常跑完的路径共用
/// 返回非 pass 的条目数, 由调用方决定退出码
fn report(state: &OfficialHash, details: &HashMap<usize, Vec<String>>, state_path: &Path) -> usize {
    let pass = state.count(CheckState::Pass);
    let total = state.files.len();
    let failed = total - pass;

    // 条目太多时只列非 pass 的, 避免上万个小文件刷屏
    let list_all = total <= report::LIST_LIMIT;
    let mut rows: Vec<Row> = Vec::new();
    for (idx, f) in state.files.iter().enumerate() {
        if !list_all && f.check == CheckState::Pass {
            continue;
        }
        let detail = details.get(&idx).cloned().unwrap_or_default();
        rows.push(Row::with_detail(f.check.tag(), &f.name, detail));
    }
    report::print_rows(&rows);
    if !list_all {
        println!(
            "{:width$} ... {pass} passed file(s) not listed",
            "",
            width = report::TAG_WIDTH
        );
    }

    println!();
    report::print_summary(
        &[
            ("Files", total.to_string()),
            ("Passed", pass.to_string()),
            ("Failed", failed.to_string()),
            ("Anchor", state_path.display().to_string()),
        ],
        if failed == 0 { "PASS" } else { "FAIL" },
    );
    failed
}

/// 续跑前确认上游快照没变, 按哈希清单比对, 不依赖平台的 revision 语义
fn verify_same_snapshot(prev: &OfficialHash, manifest: &Manifest) -> Result<()> {
    if prev.files.len() != manifest.entries.len() {
        return Err(Error::Upstream(format!(
            "manifest entry count changed from {} to {}, cannot resume; check --revision, or delete official_hash.json and start over",
            prev.files.len(),
            manifest.entries.len()
        )));
    }
    let mut prev_map: HashMap<&str, &FileEntry> =
        prev.files.iter().map(|f| (f.name.as_str(), f)).collect();
    for e in &manifest.entries {
        match prev_map.remove(e.name.as_str()) {
            None => {
                return Err(Error::Upstream(format!(
                    "upstream added {}, cannot resume; check --revision, or delete official_hash.json and start over",
                    e.name
                )));
            }
            Some(p) if p.sha256 != e.sha256 => {
                return Err(Error::Upstream(format!(
                    "upstream changed: sha256 of {} differs from the record, cannot resume; check --revision, or delete official_hash.json and start over",
                    e.name
                )));
            }
            Some(_) => {}
        }
    }
    Ok(())
}

fn run_tasks(
    state: &mut OfficialHash,
    tasks: &[Task],
    total_bytes: u64,
    state_path: &Path,
    details: &mut HashMap<usize, Vec<String>>,
) -> Result<usize> {
    let workers = gate::max_workers();
    let gate = Gate::new(1);
    let mut governor = Governor::new(workers);
    let next = AtomicUsize::new(0);
    // 连续计量"实际读到的字节", 控制器与进度都用它
    let bytes_read = AtomicU64::new(0);

    std::thread::scope(|scope| -> Result<usize> {
        let (tx, rx) = mpsc::channel::<(usize, Verdict)>();

        for _ in 0..workers {
            let tx = tx.clone();
            let gate = &gate;
            let next = &next;
            let bytes_read = &bytes_read;
            scope.spawn(move || {
                loop {
                    // 先领任务再排队占额度; 领不到就退出额度是"正在读"的许可,
                    // 所以空闲 worker 不会占着任务不放
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= tasks.len() {
                        break;
                    }
                    let t = &tasks[i];
                    let v = {
                        let _permit = gate.acquire();
                        evaluate(&t.path, t.size, &t.sha256, bytes_read)
                    };
                    if tx.send((t.idx, v)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);

        // 主线程既是唯一写者, 也是并发控制器
        let mut received = 0usize;
        let mut since_flush = 0usize;
        let mut last_flush = Instant::now();
        let mut hash_mismatch = 0usize;

        while received < tasks.len() {
            match rx.recv_timeout(POLL) {
                Ok((idx, v)) => {
                    state.files[idx].check = v.state;
                    if v.hash_mismatch {
                        hash_mismatch += 1;
                    }
                    if !v.detail.is_empty() {
                        details.insert(idx, v.detail);
                    }
                    received += 1;
                    since_flush += 1;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }

            let now = gate::read_counter(&bytes_read);
            if governor.tick(&gate, now) {
                gate::print_progress(received, tasks.len(), now, total_bytes, governor.limit());
            }

            if since_flush >= FLUSH_EVERY || last_flush.elapsed() >= FLUSH_INTERVAL {
                jsonio::save(state_path, state)?;
                since_flush = 0;
                last_flush = Instant::now();
            }
        }

        gate::clear_progress();
        jsonio::save(state_path, state)?;
        Ok(hash_mismatch)
    })
}

/// 判定单个文件
fn evaluate(path: &Path, want_size: u64, want_sha: &str, progress: &AtomicU64) -> Verdict {
    match std::fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Verdict::plain(CheckState::Missing),
        Err(e) => Verdict::with_detail(CheckState::Unreadable, vec![format!("reason: {e}")]),
        Ok(m) if !m.is_file() => {
            Verdict::with_detail(CheckState::Unreadable, vec!["reason: not a regular file".into()])
        }
        // 大小不符直接判失败, 不再算哈希
        Ok(m) if m.len() != want_size => Verdict::with_detail(
            CheckState::Fail,
            vec![
                format!("expected: {want_size}"),
                format!("actual:   {}", m.len()),
            ],
        ),
        Ok(_) => match hashing::hash_file(path, Algorithm::Sha256, progress) {
            Err(e) => Verdict::with_detail(CheckState::Unreadable, vec![format!("reason: {e}")]),
            Ok((actual, _)) => {
                if actual == want_sha {
                    Verdict::plain(CheckState::Pass)
                } else {
                    Verdict::mismatch(vec![
                        format!("expected: {want_sha}"),
                        format!("actual:   {actual}"),
                    ])
                }
            }
        },
    }
}
