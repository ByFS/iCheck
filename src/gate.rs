use std::io::IsTerminal;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// 控制器窗口长度HDD 上多一个 worker 立刻掉吞吐, 所以这个窗口要够长才测得准
const WINDOW: Duration = Duration::from_secs(3);

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
/// 从 limit = 1 开始: HDD 上永远不会被拉到 2 以上(加一个立刻掉吞吐, 下一窗口回退)
/// 窗口内没有读到任何字节(NFS 卡住等)时不下调, 只等待, 避免把网络抖动误判成并发过高
pub struct Governor {
    workers: usize,
    limit: usize,
    best_rate: f64,
    last_bytes: u64,
    last_tick: Instant,
}

impl Governor {
    pub fn new(workers: usize) -> Self {
        Governor {
            workers,
            limit: 1,
            best_rate: 0.0,
            last_bytes: 0,
            last_tick: Instant::now(),
        }
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    /// 窗口到点就调整一次并返回 true, 调用方据此刷进度
    pub fn tick(&mut self, gate: &Gate, bytes_now: u64) -> bool {
        let elapsed = self.last_tick.elapsed();
        if elapsed < WINDOW {
            return false;
        }

        let rate = (bytes_now - self.last_bytes) as f64 / elapsed.as_secs_f64();
        if bytes_now > self.last_bytes {
            if rate > self.best_rate * 1.05 && self.limit < self.workers {
                self.best_rate = rate;
                self.limit += 1;
                gate.set_limit(self.limit);
            } else if rate < self.best_rate * 0.8 && self.limit > 1 {
                self.limit = (self.limit / 2).max(1);
                self.best_rate = rate;
                gate.set_limit(self.limit);
            } else if rate > self.best_rate {
                self.best_rate = rate;
            }
        }
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
pub fn print_progress(done: usize, total: usize, bytes: u64, total_bytes: u64, limit: usize) {
    if !std::io::stderr().is_terminal() {
        return;
    }
    let pct = if total_bytes > 0 {
        bytes as f64 / total_bytes as f64 * 100.0
    } else {
        0.0
    };
    eprint!(
        "\r\x1b[2K  [{done}/{total}] {:.2}/{:.2} GB ({pct:.1}%)  workers: {limit}",
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
