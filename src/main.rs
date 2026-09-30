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
            eprintln!("ERROR: {e}");
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
                    "check requires <path> <source> <author>/<model>".to_string(),
                ));
            }
            let root = PathBuf::from(&positional[0]);
            let platform = Platform::parse(&positional[1]).ok_or_else(|| {
                Error::Usage(format!("unknown source: {} (only ms is supported)", positional[1]))
            })?;
            let model_id = positional[2].clone();

            let outcome = check::run(&root, platform, &model_id, revision.as_deref())?;
            Ok(finish(outcome.failed, outcome.hash_mismatch))
        }
        "generate" => {
            let (root, force) = path_and_flags(&args[1..], "generate", true)?;
            let outcome = generate::run(&root, force)?;
            Ok(finish(outcome.failed, 0))
        }
        "verify" => {
            let (root, _) = path_and_flags(&args[1..], "verify", false)?;
            let outcome = verify::run(&root)?;
            Ok(finish(outcome.failed, outcome.hash_mismatch))
        }
        "-h" | "--help" | "help" => {
            print_help();
            Ok(ExitCode::SUCCESS)
        }
        other => Err(Error::Usage(format!("unknown command: {other}"))),
    }
}

/// 退出码: 0 通过 / 1 仅集合级差异 / 2 内容不符 / 3 工具或数据故障(见 error)
fn finish(failed: usize, hash_mismatch: usize) -> ExitCode {
    if failed == 0 {
        ExitCode::SUCCESS
    } else if hash_mismatch > 0 {
        ExitCode::from(2)
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
                return Err(Error::Usage(format!("{command}: unknown option {other}")));
            }
            other => positional.push(other.to_string()),
        }
    }

    if positional.len() != 1 {
        return Err(Error::Usage(format!("{command} requires <path>")));
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
                .ok_or_else(|| Error::Usage("--revision requires a value".to_string()))?;
            revision = Some(v.clone());
        } else {
            positional.push(a.clone());
        }
    }
    Ok((positional, revision))
}

fn print_help() {
    println!("iCheck - AI model integrity checker");
    println!();
    println!("Usage:");
    println!("  icheck check <path> <source> <author>/<model> [--revision <rev>]");
    println!("  icheck generate <path> [-f|--force]");
    println!("  icheck verify <path>");
    println!();
    println!("Commands:");
    println!("  check      fetch upstream hashes and verify the local model");
    println!("  generate   build the local anchor from official_hash.json");
    println!("  verify     verify the local model against the anchor");
    println!();
    println!("Source:");
    println!("  ms   ModelScope");
    println!();
    println!("Options:");
    println!("  -h, --help      show this help");
    println!("  -f, --force     generate: anchor even if the official check has not fully passed");
    println!("                  only the precondition is relaxed, official_hash is left untouched");
    println!();
    println!("Exit codes:");
    println!("  0  all good");
    println!("  1  set-level differences only (missing / size / added / unreadable)");
    println!("  2  at least one content hash mismatch");
    println!("  3  tool, upstream or data failure");
    println!();
    println!("Environment:");
    println!("  ICHECK_WORKERS  hard limit on worker threads (default: cores, tuned at runtime)");
}
