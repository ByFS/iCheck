mod anchor;
mod check;
mod cli;
mod error;
mod gate;
mod generate;
mod hashing;
mod jsonio;
mod log;
mod official;
mod path;
mod pool;
mod report;
mod upstream;
mod verify;
mod walk;

use std::process::ExitCode;

use error::{Error, Result};
use upstream::Platform;

/// 工具名与版本, 同时用于 --version 与落入 JSON 的 tool 字段
pub const TOOL: &str = concat!("icheck ", env!("CARGO_PKG_VERSION"));

pub fn now_rfc3339() -> String {
    chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();

    // --debug 是全局开关, 从任何位置拿走, 不参与各命令的参数解析
    if let Some(pos) = args.iter().position(|a| a == "--debug") {
        args.remove(pos);
        log::enable_debug();
        crate::debug!("argv: icheck {}", args.join(" "));
        crate::debug!(
            "build: {}, cores: {}, ICHECK_WORKERS: {}",
            if cfg!(debug_assertions) { "debug (unoptimized)" } else { "release" },
            std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
            std::env::var("ICHECK_WORKERS").unwrap_or_else(|_| "unset".to_string())
        );
    }

    // 问用法的人经常把 -h 写在命令后面, 所以这三个也当全局开关接住
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "-v" || a == "--version") {
        println!("{TOOL}");
        return ExitCode::SUCCESS;
    }

    match dispatch(&args) {
        Ok(code) => code,
        Err(e) => {
            crate::error!("{e}");
            ExitCode::from(e.exit_code())
        }
    }
}

fn dispatch(args: &[String]) -> Result<ExitCode> {
    let Some(command) = args.first() else {
        return Err(Error::Usage(
            "no command given, run `icheck --help` for usage".to_string(),
        ));
    };

    match command.as_str() {
        "check" => {
            let (positional, revision, source) = check_args(&args[1..])?;
            if positional.len() > 2 {
                return Err(Error::Usage(too_many_args(&positional)));
            }
            let root = cli::resolve_root(positional.first().map(String::as_str))?;
            let (model_id, platform) = cli::resolve_check(
                &root,
                positional.get(1).map(String::as_str),
                source,
            )?;

            crate::info!("Root: {}", root.display());
            warn_unoptimized();
            let outcome = check::run(&root, platform, &model_id, revision.as_deref())?;
            Ok(finish(outcome.failed, outcome.hash_mismatch))
        }
        "generate" => {
            let (path, force) = path_and_flags(&args[1..], "generate", true)?;
            let root = cli::resolve_root(path.as_deref())?;
            crate::info!("Root: {}", root.display());
            warn_unoptimized();
            let outcome = generate::run(&root, force)?;
            Ok(finish(outcome.failed, 0))
        }
        "verify" => {
            let (path, _) = path_and_flags(&args[1..], "verify", false)?;
            let root = cli::resolve_root(path.as_deref())?;
            crate::info!("Root: {}", root.display());
            warn_unoptimized();
            let outcome = verify::run(&root)?;
            Ok(finish(outcome.failed, outcome.hash_mismatch))
        }
        "help" => {
            print_help();
            Ok(ExitCode::SUCCESS)
        }
        "version" => {
            println!("{TOOL}");
            Ok(ExitCode::SUCCESS)
        }
        other => Err(unknown_command(other)),
    }
}

/// debug 构建下哈希慢约 20 倍, 而从输出里完全看不出来 —— 开工前直接说破
///
/// 生产机上跑过 debug 二进制, 单线程 SHA-256 只有 0.03 GB/s, 被误当成存储或
/// CPU 配额的问题查了很久, 所以这里主动提示, 免得再踩
fn warn_unoptimized() {
    #[cfg(debug_assertions)]
    crate::warn!(
        "unoptimized build, hashing runs roughly 20x slower; rebuild with `cargo build --release`"
    );
}

/// 提示用的词表
const COMMANDS: [&str; 4] = ["check", "generate", "verify", "help"];
const OPTIONS: [&str; 9] = [
    "-h", "--help", "-v", "--version", "--debug", "-f", "--force", "--revision", "--source",
];
/// 来源名, 打错时用来提示
const SOURCES: [&str; 4] = ["ms", "modelscope", "hf", "huggingface"];
/// 只在某个命令下有意义的选项, 用错了就直接说清它属于谁
const COMMAND_ONLY: [(&str, &str); 4] = [
    ("-f", "generate"),
    ("--force", "generate"),
    ("--revision", "check"),
    ("--source", "check"),
];
const HELP_HINT: &str = "run `icheck --help` for usage";

/// 命令名打错时的提示
fn unknown_command(got: &str) -> Error {
    Error::Usage(match suggest(got, &COMMANDS) {
        Some(s) => format!("unknown command: {got}, did you mean `{s}`?"),
        None => format!("unknown command: {got}, {HELP_HINT}"),
    })
}

/// check 的位置参数给多了
///
/// 最常见的多给一个就是旧写法 `check <path> <source> <model>`: 来源已经改成 --source,
/// 所以这里要认出来并给出新写法, 否则老习惯会撞在一句干巴巴的"参数太多"上
fn too_many_args(positional: &[String]) -> String {
    if let Some(platform) = positional.get(1).and_then(|v| Platform::parse(v)) {
        return format!(
            "check takes <path> and <author>/<model>, the platform moved to an option, try: icheck check {} --source {} {}",
            positional[0],
            platform.as_str(),
            positional[2..].join(" ")
        );
    }
    format!(
        "check takes at most <path> and <author>/<model>, got {} arguments",
        positional.len()
    )
}

/// 来源名打错时的提示
fn unknown_source(got: &str) -> Error {
    Error::Usage(match suggest(got, &SOURCES) {
        Some(s) => format!("unknown source: {got}, did you mean `{s}`?"),
        None => format!("unknown source: {got}, use ms or hf"),
    })
}

/// 选项不认识的提示: 先看它是不是别的命令的选项, 再猜最接近的那个
fn unknown_option(command: &str, got: &str) -> Error {
    if let Some((_, owner)) = COMMAND_ONLY.iter().find(|(o, _)| *o == got) {
        return Error::Usage(format!(
            "{command}: unknown option {got}, it only applies to {owner}"
        ));
    }
    Error::Usage(match suggest(got, &OPTIONS) {
        Some(s) => format!("{command}: unknown option {got}, did you mean `{s}`?"),
        None => format!("{command}: unknown option {got}, {HELP_HINT}"),
    })
}

/// 最接近的候选, 差太远就不猜
///
/// 一两个字符的选项也不猜: `-x` 到 `-f` 的距离同样是 1, 猜了只会误导
fn suggest<'a>(got: &str, candidates: &[&'a str]) -> Option<&'a str> {
    if got.chars().count() < 3 {
        return None;
    }
    candidates
        .iter()
        .map(|c| (*c, edit_distance(got, c)))
        .filter(|(c, d)| *d <= 2 && *d * 2 < c.chars().count())
        .min_by_key(|(_, d)| *d)
        .map(|(c, _)| c)
}

/// 编辑距离, 只用来判断"是不是打错了", 输入都很短
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];

    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// 退出码: 0 通过 / 1 仅集合级差异 / 2 内容不符 / 3 工具, 上游, 数据或用法故障(见 error)
fn finish(failed: usize, hash_mismatch: usize) -> ExitCode {
    if failed == 0 {
        ExitCode::SUCCESS
    } else if hash_mismatch > 0 {
        ExitCode::from(2)
    } else {
        ExitCode::from(1)
    }
}

/// generate / verify 最多收一个路径, 省掉就用当前目录往上找
fn path_and_flags(
    args: &[String],
    command: &str,
    allow_force: bool,
) -> Result<(Option<String>, bool)> {
    let mut positional = Vec::new();
    let mut force = false;

    for a in args {
        match a.as_str() {
            "-f" | "--force" if allow_force => force = true,
            other if other.starts_with('-') => {
                return Err(unknown_option(command, other));
            }
            other => positional.push(other.to_string()),
        }
    }

    if positional.len() > 1 {
        return Err(Error::Usage(format!(
            "{command} takes at most one <path>, got {} arguments",
            positional.len()
        )));
    }
    Ok((positional.into_iter().next(), force))
}

/// check 最多收两个位置参数(路径 / 模型 ID), 外加 --source 与 --revision
///
/// 来源改成选项而不是位置参数: 路径与模型 ID 都能省掉之后, "第一个位置参数到底是
/// 路径还是来源"就说不清了
fn check_args(args: &[String]) -> Result<(Vec<String>, Option<String>, Option<Platform>)> {
    let mut positional = Vec::new();
    let mut revision = None;
    let mut source = None;
    let mut it = args.iter();

    while let Some(a) = it.next() {
        match a.as_str() {
            "--revision" => {
                let v = it
                    .next()
                    .ok_or_else(|| Error::Usage("--revision requires a value".to_string()))?;
                revision = Some(v.clone());
            }
            "--source" => {
                let v = it
                    .next()
                    .ok_or_else(|| Error::Usage("--source requires a value".to_string()))?;
                source = Some(Platform::parse(v).ok_or_else(|| unknown_source(v))?);
            }
            // 选项跟在位置参数后面时最容易打错, 不能默默当成路径收下
            other if other.starts_with('-') => return Err(unknown_option("check", other)),
            other => positional.push(other.to_string()),
        }
    }
    Ok((positional, revision, source))
}

fn print_help() {
    println!("iCheck - AI model integrity checker");
    println!();
    println!("Usage:");
    println!("  icheck check [<path>] [<author>/<model>] [--source <name>] [--revision <rev>]");
    println!("  icheck generate [<path>] [-f|--force]");
    println!("  icheck verify [<path>]");
    println!();
    println!("Defaults:");
    println!("  <path>            the model directory; without it the current directory is used,");
    println!("                    or the nearest parent holding .iCheck");
    println!("  <author>/<model>  read from .iCheck/official/official_hash.json");
    println!("  --source          the platform recorded there, else ms");
    println!();
    println!("Commands:");
    println!("  check      fetch upstream hashes and verify the local model");
    println!("  generate   build the local anchor from official_hash.json");
    println!("  verify     verify the local model against the anchor");
    println!();
    println!("Source:");
    println!("  ms   ModelScope (default)");
    println!("  hf   HuggingFace");
    println!();
    println!("Options:");
    println!("  -h, --help      show this help");
    println!("  -v, --version   show the version");
    println!("  --debug         print detailed diagnostics (timings, windows, per-file verdicts)");
    println!("  --source <name> check: platform to fetch the official hashes from (ms, hf)");
    println!("  --revision <r>  check: pin an upstream revision (default: the platform default)");
    println!("  -f, --force     generate: anchor even if the official check has not fully passed");
    println!("                  only the precondition is relaxed, official_hash is left untouched");
    println!();
    println!("Exit codes:");
    println!("  0  all good");
    println!("  1  set-level differences only (missing / size / added / unreadable)");
    println!("  2  at least one content hash mismatch");
    println!("  3  tool, upstream, data or usage failure");
    println!();
    println!("Environment:");
    println!("  ICHECK_WORKERS  hard limit on worker threads (default: cores, tuned at runtime)");
}
