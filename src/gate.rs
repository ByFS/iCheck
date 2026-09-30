use std::io::IsTerminal;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::hashing::{self, Algorithm};

/// 探测窗口: 至少 1 秒, 且至少读到这么多字节, 最多 3 秒
const MIN_WINDOW: Duration = Duration::from_secs(1);
const MAX_WINDOW: Duration = Duration::from_secs(3);
const MIN_WINDOW_BYTES: u64 = 64 << 20;

/// 开工前探测多少字节
const PROBE_BYTES: usize = 32 << 20;

struct Inner {
    active: usize,
    limit: usize,
}

/// 自适应并发的闸门
///
/// 线程数一次起满(上限 = 可用核数), 但**同时真正在读**的线程数由 limit 控制
/// 控制器只改 limit 这样没有线程创建/销毁开销, 也不受线程池大小不可变的限制
pub struct Gate {
    inner: Mutex<Inner>,
    cv: Condvar,
}

/// 持有它表示占了一个并发额度, drop 时归还
pub struct Permit<'a> {
    gate: &'a Gate,
}

impl Gate {
    pub fn new(limit: usize) -> Self {
        Gate {
            inner: Mutex::new(Inner {
                active: 0,
                limit: limit.max(1),
            }),
            cv: Condvar::new(),
        }
    }

    pub fn acquire(&self) -> Permit<'_> {
        let mut g = self.inner.lock().unwrap();
        while g.active >= g.limit {
            g = self.cv.wait(g).unwrap();
        }
        g.active += 1;
        drop(g);
        Permit { gate: self }
    }

    pub fn set_limit(&self, n: usize) {
        let mut g = self.inner.lock().unwrap();
        g.limit = n.max(1);
        drop(g);
        self.cv.notify_all();
    }
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        let mut g = self.gate.inner.lock().unwrap();
        g.active -= 1;
        drop(g);
        self.gate.cv.notify_one();
    }
}

/// 并发控制器
///
/// 一次开工前的探测结果
pub struct Probe {
    /// 单线程顺序读的带宽, 字节/秒
    pub read_bps: f64,
    /// 单线程的哈希速率, 字节/秒
    pub hash_bps: f64,
}

/// 读一个真实文件的前 PROBE_BYTES 字节, 测出读带宽与单线程哈希率
///
/// 为什么实测而不是按存储类型猜: 并发数的正确取值是 `读带宽 / 单线程哈希率`。
/// 盘能给 3 GB/s 而 BLAKE3 单线程也 3 GB/s 时, 一个 worker 就饱和了;
/// 页缓存能给 12 GB/s 时才需要多个。这个比值只有量出来才知道,
/// 而且它自带缓存语义 —— 数据在页缓存里就读得快, 本来也该开更多并发
pub fn probe(path: &Path, algo: Algorithm) -> Option<Probe> {
    use std::io::Read;

    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; PROBE_BYTES];
    let started = Instant::now();
    let mut filled = 0usize;
    while filled < PROBE_BYTES {
        match file.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(_) => break,
        }
    }
    let read_secs = started.elapsed().as_secs_f64();
    if filled == 0 || read_secs <= 0.0 {
        return None;
    }
    let read_bps = filled as f64 / read_secs;

    let started = Instant::now();
    let _ = hashing::hash_bytes(&buf[..filled], algo);
    let hash_secs = started.elapsed().as_secs_f64();
    if hash_secs <= 0.0 {
        return None;
    }
    let hash_bps = filled as f64 / hash_secs;

    crate::debug_log!(
        "probe {}: read {} MiB in {} ms ({:.2} GB/s), {} in {} ms ({:.2} GB/s)",
        path.display(),
        filled >> 20,
        (read_secs * 1000.0) as u64,
        read_bps / 1e9,
        algo.name(),
        (hash_secs * 1000.0) as u64,
        hash_bps / 1e9,
    );

    Some(Probe { read_bps, hash_bps })
}

/// 开工前量两个数, **只用于报告**
///
/// 刻意不用它决定并发数: 它量的是**单流顺序读**, 而并发读会把访问模式打成随机,
/// 有效带宽可能掉一个数量级 —— 生产机上实测过, 240 个 worker 把顺序读的 0.77 GB/s
/// 打成了约 90 MB/s, 并发越大反而越慢。
/// 所以从 1 开始, 由 Governor 按**真实并发下的吞吐**向上爬
pub fn report_rates(largest: Option<&Path>, algo: Algorithm, cores: usize) {
    match largest.and_then(|p| probe(p, algo)) {
        Some(p) => println!(
            "INFO: Measured: sequential read {:.2} GB/s, {} {:.2} GB/s per thread",
            p.read_bps / 1e9,
            algo.name(),
            p.hash_bps / 1e9
        ),
        None => println!("INFO: Measured: probe unavailable"),
    }
    if cores <= 1 {
        println!("INFO: Workers: 1 (single core)");
    } else {
        println!("INFO: Workers: start at 1 of {cores} cores, raised while throughput improves");
    }
    println!();
}

/// 运行中的自适应控制器
///
/// 初始值由 `calibrate` 实测给出, 之后用一个带回退的爬山确认:
/// 先在初始值处测一窗作为基准, 再翻倍试一窗 —— 吞吐真的改善就继续翻倍,
/// 持平或变差就退回并停手。**"持平"必须当成到达平台期, 不能继续加**,
/// 否则在存储已饱和时会一路加到核数上限, 白白浪费核心。
/// 窗口至少 1 秒且至少读到 64 MiB(最多 3 秒); 窗口内没有读到任何字节时不动
pub struct Governor {
    workers: usize,
    limit: usize,
    /// 上一个被接受的 limit, 回退时用
    prev_limit: usize,
    /// 基准吞吐(在 prev_limit 处测到的)与本次窗口的吞吐
    baseline: Option<f64>,
    last_rate: f64,
    settled: bool,
    last_bytes: u64,
    last_tick: Instant,
}

impl Governor {
    pub fn new(workers: usize, initial: usize) -> Self {
        let workers = workers.max(1);
        Governor {
            workers,
            limit: initial.clamp(1, workers),
            prev_limit: initial.clamp(1, workers),
            baseline: None,
            last_rate: 0.0,
            settled: workers <= 1,
            last_bytes: 0,
            last_tick: Instant::now(),
        }
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    /// 最近一个窗口的吞吐, 字节/秒
    pub fn rate_bps(&self) -> f64 {
        self.last_rate
    }

    /// 窗口到点就调整一次并返回 true, 调用方据此刷进度
    pub fn tick(&mut self, gate: &Gate, bytes_now: u64) -> bool {
        let elapsed = self.last_tick.elapsed();
        let progressed = bytes_now.saturating_sub(self.last_bytes);
        if elapsed < MIN_WINDOW {
            return false;
        }
        // 数据太少的窗口测不准, 但也不能无限等
        if progressed < MIN_WINDOW_BYTES && elapsed < MAX_WINDOW {
            return false;
        }

        // 一点进展都没有: 只等, 不下调
        if progressed == 0 {
            crate::debug_log!("window {:.2}s: no progress, holding", elapsed.as_secs_f64());
            self.last_tick = Instant::now();
            return true;
        }

        let rate = progressed as f64 / elapsed.as_secs_f64();
        self.last_rate = rate;
        let before = self.limit;
        let mut decision = "settled";

        if !self.settled {
            match self.baseline {
                // 第一窗: 只记基准, 不比较
                None => {
                    self.baseline = Some(rate);
                    self.prev_limit = self.limit;
                    if self.limit < self.workers {
                        self.limit = (self.limit * 2).min(self.workers);
                        gate.set_limit(self.limit);
                        decision = "baseline";
                    } else {
                        self.settled = true;
                        decision = "already at core count";
                    }
                }
                Some(base) => {
                    if rate > base * 1.02 {
                        // 还有改善, 接受这一档并继续翻倍
                        self.baseline = Some(rate);
                        self.prev_limit = self.limit;
                        if self.limit < self.workers {
                            self.limit = (self.limit * 2).min(self.workers);
                            gate.set_limit(self.limit);
                            decision = "improved";
                        } else {
                            self.settled = true;
                            decision = "at core count";
                        }
                    } else {
                        // 持平或变差: 退回上一档, 到此为止
                        if self.limit != self.prev_limit {
                            self.limit = self.prev_limit;
                            gate.set_limit(self.limit);
                        }
                        self.settled = true;
                        decision = "no gain, retreat and settle";
                    }
                }
            }
        }

        crate::debug_log!(
            "window {:.2}s, {} MiB, {:.3} GB/s, limit {before} -> {} ({decision})",
            elapsed.as_secs_f64(),
            progressed >> 20,
            rate / 1e9,
            self.limit
        );

        self.last_tick = Instant::now();
        self.last_bytes = bytes_now;
        true
    }
}

/// 默认并发上限真正的并发数由 Gate 在运行中自适应, 这里只是线程数上限
/// 可用 ICHECK_WORKERS 硬指定, 自动控制在混合存储/特殊挂载上一定会猜错
pub fn max_workers() -> usize {
    if let Ok(v) = std::env::var("ICHECK_WORKERS") {
        if let Ok(n) = v.trim().parse::<usize>() {
            if n > 0 {
                return n;
            }
        }
    }
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

/// 进度走 stderr, 用 \r 覆盖同一行, 不污染重定向到文件的 stdout
/// 只有 stderr 是终端时才打: 否则会把转义序列写进日志
///
/// 带上速率是刻意的: 判断"瓶颈在存储还是 CPU"就看它 ——
/// 速率贴着存储的实测读带宽, 瓶颈在存储; 明显低于它, 瓶颈在 CPU
pub fn print_progress(
    done: usize,
    total: usize,
    bytes: u64,
    total_bytes: u64,
    limit: usize,
    rate_bps: f64,
) {
    if !std::io::stderr().is_terminal() {
        return;
    }
    let pct = if total_bytes > 0 {
        bytes as f64 / total_bytes as f64 * 100.0
    } else {
        0.0
    };
    let rate = if rate_bps > 0.0 {
        format!("  {:.2} GB/s", rate_bps / 1e9)
    } else {
        String::new()
    };
    eprint!(
        "\r\x1b[2K  [{done}/{total}] {:.2}/{:.2} GB ({pct:.1}%){rate}  workers: {limit}",
        bytes as f64 / 1e9,
        total_bytes as f64 / 1e9
    );
}

pub fn clear_progress() {
    if std::io::stderr().is_terminal() {
        eprint!("\r\x1b[2K");
    }
}

/// 读字节计数器
pub fn new_counter() -> AtomicU64 {
    AtomicU64::new(0)
}

pub fn read_counter(c: &AtomicU64) -> u64 {
    c.load(Ordering::Relaxed)
}
