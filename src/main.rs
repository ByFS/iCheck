mod anchor;
mod check;
mod error;
mod gate;
mod generate;
mod hashing;
mod jsonio;
mod official;
mod pool;
mod report;
mod upstream;
mod verify;
mod walk;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use error::{Error, Result};
use upstream::Platform;

pub fn now_rfc3339() -> String {
    chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match dispatch(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("错误: {e}");
            ExitCode::from(e.exit_code())
        }
    }
}

fn dispatch(args: &[String]) -> Result<ExitCode> {
    let Some(command) = args.first() else {
        print_help();
        return Ok(ExitCode::from(2));
    };

    match command.as_str() {
        "check" => {
            let (positional, revision) = split_args(&args[1..])?;
            if positional.len() != 3 {
                return Err(Error::Usage(
                    "check 需要 <path> <source> <author>/<model>".to_string(),
                ));
            }
            let root = PathBuf::from(&positional[0]);
            let platform = Platform::parse(&positional[1]).ok_or_else(|| {
                Error::Usage(format!("未知的 source: {} (目前只支持 ms)", positional[1]))
            })?;
            let model_id = positional[2].clone();

            let outcome = check::run(&root, platform, &model_id, revision.as_deref())?;
            Ok(finish(outcome.failed))
        }
        "generate" => {
            let (root, force) = path_and_flags(&args[1..], "generate", true)?;
            let outcome = generate::run(&root, force)?;
            Ok(finish(outcome.failed))
        }
        "verify" => {
            let (root, _) = path_and_flags(&args[1..], "verify", false)?;
            let outcome = verify::run(&root)?;
            Ok(finish(outcome.failed))
        }
        "-h" | "--help" | "help" => {
            print_help();
            Ok(ExitCode::SUCCESS)
        }
        other => Err(Error::Usage(format!("未知的命令: {other}"))),
    }
}

fn finish(failed: usize) -> ExitCode {
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// generate / verify 只收一个路径参数
fn path_and_flags(args: &[String], command: &str, allow_force: bool) -> Result<(PathBuf, bool)> {
    let mut positional = Vec::new();
    let mut force = false;

    for a in args {
        match a.as_str() {
            "-f" | "--force" if allow_force => force = true,
            other if other.starts_with('-') => {
                return Err(Error::Usage(format!("{command} 不认识的选项: {other}")));
            }
            other => positional.push(other.to_string()),
        }
    }

    if positional.len() != 1 {
        return Err(Error::Usage(format!("{command} 需要 <path>")));
    }
    Ok((Path::new(&positional[0]).to_path_buf(), force))
}

/// 位置参数与 --revision 分离
fn split_args(args: &[String]) -> Result<(Vec<String>, Option<String>)> {
    let mut positional = Vec::new();
    let mut revision = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--revision" {
            let v = it
                .next()
                .ok_or_else(|| Error::Usage("--revision 需要一个值".to_string()))?;
            revision = Some(v.clone());
        } else {
            positional.push(a.clone());
        }
    }
    Ok((positional, revision))
}

fn print_help() {
    println!("iCheck - AI 模型完整性校验");
    println!();
    println!("用法:");
    println!("  icheck check <path> <source> <author>/<model> [--revision <rev>]");
    println!("  icheck generate <path> [-f|--force]");
    println!("  icheck verify <path>");
    println!();
    println!("source:");
    println!("  ms   ModelScope");
    println!();
    println!("选项:");
    println!("  -f, --force   generate 时忽略官方校验未通过的条目, 强制建立锚点");
    println!("                只放开前置检查, 不改写 official_hash 里的状态");
    println!();
    println!("环境变量:");
    println!("  ICHECK_WORKERS  硬指定并发线程上限(默认取可用核数, 运行中自适应)");
}
