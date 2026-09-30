//! 端到端: 真的跑二进制, 只看退出码与两个流上的文本
//!
//! check 要联网, 所以这里覆盖的是 generate / verify 这条锚链:
//! 建目录 -> 建锚点 -> 全通过 -> 改一个字节 -> 必须报 FAIL
//!
//! 顺带把已经定下来的约定钉住: 报告只走 stdout 日志只走 stderr,
//! 以及 0 / 1 / 2 / 3 四个退出码各自的含义

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// cargo 为集成测试注入被测二进制的路径
const BIN: &str = env!("CARGO_BIN_EXE_icheck");

/// 夹具里的四个文件
const NAMES: [&str; 4] = ["config.json", "a.bin", "sub/notes.md", ".gitattributes"];

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("icheck-e2e-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn put(root: &Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, bytes).unwrap();
}

/// 建一个已经有 official_hash.json 的模型目录
///
/// generate 只读 check 字段, verify 只比 BLAKE3, 所以 sha256 用占位值就够
/// bad 指定的那一条写成 missing, 用来试"官方没全过"的两条路径
fn model(tag: &str, bad: Option<&str>) -> PathBuf {
    let root = scratch(tag);
    put(&root, "config.json", b"{\"model_type\":\"demo\"}\n");
    put(&root, "a.bin", &[b'a'; 4096]);
    put(&root, "sub/notes.md", b"# demo\n");
    put(&root, ".gitattributes", b"*.bin filter=lfs -text\n");

    let entries: Vec<String> = NAMES
        .iter()
        .map(|name| {
            let size = std::fs::metadata(root.join(name)).unwrap().len();
            let check = if bad == Some(*name) { "missing" } else { "pass" };
            format!(
                r#"{{"name":"{name}","size":{size},"revision":"master","sha256":"{}","check":"{check}"}}"#,
                "0".repeat(64)
            )
        })
        .collect();
    let state = format!(
        r#"{{"tool":"icheck test","fetched_at":"2026-01-01T00:00:00+08:00","source":{{"platform":"modelscope","model_id":"demo/tiny","url":"https://example.invalid/demo/tiny"}},"files":[{}]}}"#,
        entries.join(",")
    );
    put(
        &root,
        ".iCheck/official/official_hash.json",
        state.as_bytes(),
    );
    root
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("failed to run icheck")
}

fn code(o: &Output) -> i32 {
    o.status.code().expect("icheck was killed by a signal")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn root_of(root: &Path) -> &str {
    root.to_str().unwrap()
}

/// 结果块里有没有这一行, 不关心标签与文件名之间的对齐宽度
fn has_row(out: &str, tag: &str, name: &str) -> bool {
    out.lines()
        .any(|l| l.starts_with(tag) && l.trim_end().ends_with(name))
}

/// 摘要块里某个键的值
fn summary(out: &str, key: &str) -> Option<String> {
    let head = format!("{key}:");
    out.lines()
        .find(|l| l.starts_with(&head))
        .map(|l| l[head.len()..].trim().to_string())
}

fn anchor_exists(root: &Path) -> bool {
    root.join(".iCheck/anchor/anchor_index.json").exists()
}

#[test]
fn generate_then_verify_passes() {
    let root = model("pass", None);

    let g = run(&["generate", root_of(&root)]);
    assert_eq!(code(&g), 0, "{}", stderr(&g));
    assert_eq!(summary(&stdout(&g), "Result").as_deref(), Some("PASS"));
    assert_eq!(summary(&stdout(&g), "Passed").as_deref(), Some("4"));

    let v = run(&["verify", root_of(&root)]);
    assert_eq!(code(&v), 0, "{}", stderr(&v));
    let out = stdout(&v);
    for name in NAMES {
        assert!(has_row(&out, "[PASS]", name), "missing a PASS row for {name}:\n{out}");
    }
    assert_eq!(summary(&out, "Result").as_deref(), Some("PASS"));
}

/// 同样长度改一个字节: 是内容不符, 退出码 2
#[test]
fn verify_reports_a_one_byte_change_as_a_hash_mismatch() {
    let root = model("byte", None);
    assert_eq!(code(&run(&["generate", root_of(&root)])), 0);

    let mut data = std::fs::read(root.join("a.bin")).unwrap();
    data[100] ^= 0xFF;
    std::fs::write(root.join("a.bin"), &data).unwrap();

    let v = run(&["verify", root_of(&root)]);
    let out = stdout(&v);
    assert_eq!(code(&v), 2, "{out}");
    assert!(has_row(&out, "[FAIL]", "a.bin"), "{out}");
    assert!(out.contains("Expected:") && out.contains("Actual:"), "{out}");
    assert_eq!(summary(&out, "Result").as_deref(), Some("FAIL"));
}

/// 长度变了就不必读字节, 是集合级差异, 退出码 1
#[test]
fn verify_reports_a_size_change_as_a_set_level_difference() {
    let root = model("size", None);
    assert_eq!(code(&run(&["generate", root_of(&root)])), 0);

    put(&root, "a.bin", &[b'a'; 5000]);

    let v = run(&["verify", root_of(&root)]);
    let out = stdout(&v);
    assert_eq!(code(&v), 1, "{out}");
    assert!(has_row(&out, "[FAIL]", "a.bin"), "{out}");
    assert_eq!(summary(&out, "Result").as_deref(), Some("FAIL"));
}

#[test]
fn verify_reports_missing_and_added() {
    let root = model("missing", None);
    assert_eq!(code(&run(&["generate", root_of(&root)])), 0);

    std::fs::remove_file(root.join("sub/notes.md")).unwrap();
    put(&root, "extra.txt", b"local addition\n");

    let v = run(&["verify", root_of(&root)]);
    let out = stdout(&v);
    assert_eq!(code(&v), 1, "{out}");
    assert!(has_row(&out, "[MISSING]", "sub/notes.md"), "{out}");
    assert!(has_row(&out, "[ADDED]", "extra.txt"), "{out}");
    // 多余的文件不算失败, 只单独计数
    assert_eq!(summary(&out, "Failed").as_deref(), Some("1"));
    assert_eq!(summary(&out, "Added").as_deref(), Some("1"));
}

/// 读不了的条目走另一条分支, 也归到集合级差异
#[test]
fn verify_reports_an_unreadable_entry() {
    let root = model("unreadable", None);
    assert_eq!(code(&run(&["generate", root_of(&root)])), 0);

    // 用目录冒充文件: metadata 成功, 但不是普通文件
    std::fs::remove_file(root.join("sub/notes.md")).unwrap();
    std::fs::create_dir(root.join("sub/notes.md")).unwrap();

    let v = run(&["verify", root_of(&root)]);
    let out = stdout(&v);
    assert_eq!(code(&v), 1, "{out}");
    assert!(has_row(&out, "[UNREADABLE]", "sub/notes.md"), "{out}");
    assert!(out.contains("Reason:"), "{out}");
}

/// 报告只走 stdout, 日志只走 stderr —— 这样重定向任一边都是干净的
#[test]
fn report_and_logs_are_on_separate_streams() {
    let root = model("streams", None);
    let g = run(&["generate", root_of(&root)]);
    let (out, err) = (stdout(&g), stderr(&g));

    assert!(!out.contains("INFO:"), "stdout has a log line:\n{out}");
    assert!(!out.contains("WARN:"), "stdout has a log line:\n{out}");
    assert!(!err.contains("[PASS]"), "stderr has a report row:\n{err}");
    assert!(!err.contains("Result:"), "stderr has a report line:\n{err}");
    assert!(err.contains("INFO: Source: "), "no header log:\n{err}");
}

/// 官方没全过时拒绝建锚点, 且不写出锚点文件
#[test]
fn generate_refuses_without_a_full_official_check() {
    let root = model("refuse", Some("sub/notes.md"));

    let g = run(&["generate", root_of(&root)]);
    let out = stdout(&g);
    assert_eq!(code(&g), 1, "{out}");
    // 拒绝时列出的行用的是 CheckState::tag(), 顺带钉住那套标签
    assert!(has_row(&out, "[MISSING]", "sub/notes.md"), "{out}");
    assert_eq!(summary(&out, "Result").as_deref(), Some("FAIL"));
    assert!(stderr(&g).contains("Refusing to anchor"), "{}", stderr(&g));
    assert!(!anchor_exists(&root), "an anchor was written anyway");
}

#[test]
fn force_anchors_anyway_and_says_so() {
    let root = model("force", Some("sub/notes.md"));

    let g = run(&["generate", root_of(&root), "-f"]);
    assert_eq!(code(&g), 0, "{}", stderr(&g));
    assert!(stderr(&g).contains("-f, skipping the official check"), "{}", stderr(&g));
    assert!(anchor_exists(&root));
}

/// 续跑前先修好官方状态, 否则 verify 会拿一个残缺的锚点当真
#[test]
fn verify_without_an_anchor_is_a_data_failure() {
    let root = model("noanchor", None);
    let v = run(&["verify", root_of(&root)]);
    assert_eq!(code(&v), 3);
    assert!(stderr(&v).contains("run generate first"), "{}", stderr(&v));
}

#[test]
fn an_option_from_another_command_says_which_one() {
    let root = model("hint", None);
    for args in [vec!["verify", root_of(&root), "-f"], vec!["check", root_of(&root), "ms", "a/b", "-f"]] {
        let o = run(&args);
        assert_eq!(code(&o), 3, "{}", stdout(&o));
        assert!(
            stderr(&o).contains("it only applies to generate"),
            "{:?} -> {}",
            args,
            stderr(&o)
        );
    }
}

#[test]
fn a_typo_suggests_the_closest_command() {
    let o = run(&["veraty", "/tmp"]);
    assert_eq!(code(&o), 3);
    assert!(stderr(&o).contains("did you mean `verify`?"), "{}", stderr(&o));
}

#[test]
fn a_typo_suggests_the_closest_option() {
    let root = model("typo", None);
    let o = run(&["check", root_of(&root), "ms", "a/b", "--revesion", "master"]);
    assert_eq!(code(&o), 3);
    assert!(
        stderr(&o).contains("did you mean `--revision`?"),
        "{}",
        stderr(&o)
    );
}

#[test]
fn no_command_is_a_usage_error() {
    let o = run(&[]);
    assert_eq!(code(&o), 3);
    assert!(stderr(&o).contains("run `icheck --help`"), "{}", stderr(&o));
}

/// 帮助与版本写在命令后面也认
#[test]
fn help_and_version_work_anywhere() {
    for args in [vec!["--help"], vec!["verify", "/nonexistent", "--help"]] {
        let o = run(&args);
        assert_eq!(code(&o), 0, "{:?}", args);
        assert!(stdout(&o).contains("Usage:"), "{:?} -> {}", args, stdout(&o));
    }

    let o = run(&["--version"]);
    assert_eq!(code(&o), 0);
    assert!(stdout(&o).starts_with("icheck "), "{}", stdout(&o));
}
