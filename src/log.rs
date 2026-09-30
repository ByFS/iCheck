use std::fmt;
use std::io::{IsTerminal, Write};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// 统一输出口, 所有日志只能从这里出去
///
/// 规范:
///   - 报告走 stdout, 日志走 stderr, 这样 `> report.txt 2> run.log` 两边都干净
///   - 级别只有四个: DEBUG INFO WARN ERROR
///   - 只有 DEBUG 带时间戳: 交互使用时不需要, 排查时需要
///   - 打日志前先清掉进度行, 进度行不带换行, 不清就会接成 "workers: 8DEBUG: ..."
///   - 一个事件一行, 不打印多行日志
///
/// 级别与退出码一致: ERROR 只用于退出码 3 的故障,
/// 校验不通过(退出码 1 / 2)是"干完了活", 不是错误

static DEBUG_ON: AtomicBool = AtomicBool::new(false);

/// 当前进度行的内容, 日志打完立刻重画, 免得进度条被日志擦掉
static PROGRESS: Mutex<Option<String>> = Mutex::new(None);

pub fn enable_debug() {
    DEBUG_ON.store(true, Ordering::Relaxed);
}

pub fn debug_on() -> bool {
    DEBUG_ON.load(Ordering::Relaxed)
}

pub fn info(args: fmt::Arguments<'_>) {
    emit("INFO", None, args);
}

pub fn warn(args: fmt::Arguments<'_>) {
    emit("WARN", None, args);
}

pub fn error(args: fmt::Arguments<'_>) {
    emit("ERROR", None, args);
}

/// 排查用, 只有 --debug 才输出
///
/// 带毫秒时间戳: 排查并发问题时"什么时候发生的"和"发生了什么"一样重要,
/// 窗口决策, 逐文件判定, 刷盘节奏都要能按时间排起来看
pub fn debug(args: fmt::Arguments<'_>) {
    if !debug_on() {
        return;
    }
    let stamp = chrono::Local::now().format("%H:%M:%S%.3f").to_string();
    emit("DEBUG", Some(stamp), args);
}

/// 空行分隔日志块, 同样走 stderr, stdout 只留报告
pub fn blank() {
    write_line("");
}

fn emit(level: &str, stamp: Option<String>, args: fmt::Arguments<'_>) {
    let mut line = String::with_capacity(128);
    line.push_str(level);
    line.push_str(": ");
    if let Some(s) = stamp {
        line.push_str(&s);
        line.push(' ');
    }
    line.push_str(&args.to_string());
    write_line(&line);
}

/// 写一行到 stderr
///
/// 先清掉进度行, 写完再把进度行重画回去 —— 顺序反过来会让日志把进度条吃掉
fn write_line(text: &str) {
    let progress = progress_text();
    let mut err = std::io::stderr().lock();
    if progress.is_some() {
        let _ = write!(err, "\r\x1b[2K");
    }
    let _ = writeln!(err, "{text}");
    if let Some(p) = &progress {
        let _ = write!(err, "{p}");
    }
    let _ = err.flush();
}

/// 画进度行, 传 None 收工
///
/// 非终端直接不画: 否则 \r 与转义序列会被写进重定向的日志
pub fn set_progress(text: Option<String>) {
    if !std::io::stderr().is_terminal() {
        return;
    }
    {
        *PROGRESS.lock().unwrap() = text.clone();
    }
    let mut err = std::io::stderr().lock();
    match &text {
        Some(t) => {
            let _ = write!(err, "\r\x1b[2K{t}");
        }
        None => {
            let _ = write!(err, "\r\x1b[2K");
        }
    }
    let _ = err.flush();
}

/// 取进度行内容后立刻放锁, 免得和 set_progress 反向持锁
fn progress_text() -> Option<String> {
    PROGRESS.lock().unwrap().clone()
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::log::info(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::log::warn(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::log::error(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::log::debug(format_args!($($arg)*))
    };
}
