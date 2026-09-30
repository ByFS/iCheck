use std::sync::atomic::{AtomicBool, Ordering};

static ON: AtomicBool = AtomicBool::new(false);

pub fn enable() {
    ON.store(true, Ordering::Relaxed);
}

pub fn on() -> bool {
    ON.load(Ordering::Relaxed)
}

/// 只在 --debug 时打印的排查信息
///
/// 带毫秒时间戳, 因为排查并发问题时"什么时候发生的"和"发生了什么"一样重要 ——
/// 窗口决策、逐文件判定、刷盘节奏都要能按时间排起来看
#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        if $crate::debug::on() {
            println!(
                "DEBUG: {} {}",
                chrono::Local::now().format("%H:%M:%S%.3f"),
                format_args!($($arg)*)
            );
        }
    };
}
