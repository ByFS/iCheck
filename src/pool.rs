use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use crate::gate::{self, Gate, Governor};
use crate::hashing::{self, Algorithm};

const POLL: Duration = Duration::from_millis(200);

pub struct Job {
    pub path: PathBuf,
    /// 预期大小, 只用于进度分母
    pub size: u64,
}

/// 并发计算一批文件的哈希, 返回与输入等长的结果(读失败为 None)
///
/// 没有续跑: anchor_hash 的格式里没有状态字段, 中断即从头再来
pub fn hash_all(jobs: &[Job], algo: Algorithm) -> Vec<Option<String>> {
    let n = jobs.len();
    let mut out: Vec<Option<String>> = (0..n).map(|_| None).collect();
    if n == 0 {
        return out;
    }

    let workers = gate::max_workers();
    let gate = Gate::new(1);
    let mut governor = Governor::new(workers);
    let next = AtomicUsize::new(0);
    let bytes = gate::new_counter();
    let total_bytes: u64 = jobs.iter().map(|j| j.size).sum();

    std::thread::scope(|scope| {
        let (tx, rx) = mpsc::channel::<(usize, Option<String>)>();

        for _ in 0..workers {
            let tx = tx.clone();
            let gate = &gate;
            let next = &next;
            let bytes = &bytes;
            scope.spawn(move || {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= jobs.len() {
                        break;
                    }
                    let job = &jobs[i];
                    let digest = {
                        let _permit = gate.acquire();
                        hashing::hash_file(&job.path, algo, bytes)
                            .ok()
                            .map(|(hex, _)| hex)
                    };
                    if tx.send((i, digest)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);

        let mut received = 0usize;
        while received < n {
            match rx.recv_timeout(POLL) {
                Ok((i, digest)) => {
                    out[i] = digest;
                    received += 1;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            let now = gate::read_counter(&bytes);
            if governor.tick(&gate, now) {
                gate::print_progress(received, n, now, total_bytes, governor.limit());
            }
        }
        gate::clear_progress();
    });

    out
}
