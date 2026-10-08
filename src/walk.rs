use std::path::Path;

use crate::error::Result;

/// generate 遍历本地目录时用的默认排除集
/// 会被写进 anchor_index.excluded, verify 读它
pub const DEFAULT_EXCLUDED: &[&str] = &[".iCheck", ".git", ".cache", "__pycache__", "*.lock"];

pub fn default_excluded() -> Vec<String> {
    DEFAULT_EXCLUDED.iter().map(|s| s.to_string()).collect()
}

pub struct WalkEntry {
    /// 仓库相对路径, POSIX 分隔符
    pub name: String,
    pub size: u64,
}

/// 递归遍历模型目录, 返回按 name 字节序排序的文件清单
///
/// 符号链接:**文件链接跟随**(与 check 的 metadata() 口径一致), 目录链接不递归,
/// 避免成环或逃出模型根
pub fn walk(root: &Path, excluded: &[String]) -> Result<Vec<WalkEntry>> {
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), String::new())];

    while let Some((dir, prefix)) = stack.pop() {
        let read = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for entry in read {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let name = entry.file_name().to_string_lossy().to_string();
            let rel = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if is_excluded(&rel, excluded) {
                continue;
            }

            let file_type = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };

            if file_type.is_dir() {
                stack.push((entry.path(), rel));
            } else if file_type.is_file() {
                if let Ok(md) = std::fs::metadata(entry.path()) {
                    out.push(WalkEntry {
                        name: rel,
                        size: md.len(),
                    });
                }
            } else if file_type.is_symlink()
                && let Ok(md) = std::fs::metadata(entry.path())
                && md.is_file()
            {
                out.push(WalkEntry {
                    name: rel,
                    size: md.len(),
                });
            }
        }
    }

    out.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
    Ok(out)
}

/// 命中任意一条规则就排除
/// 不带通配的规则按"任意一层路径组件相等"匹配, 带通配的按单层名字匹配
fn is_excluded(rel: &str, patterns: &[String]) -> bool {
    let comps: Vec<&str> = rel.split('/').collect();
    for p in patterns {
        if p.contains('*') || p.contains('?') {
            if comps.iter().any(|c| glob_match(p, c)) {
                return true;
            }
        } else if comps.iter().any(|c| c == p) {
            return true;
        }
    }
    false
}

/// 只支持 `*` 与 `?` 的简单通配
fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (None, 0usize);

    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// 每个测试一个自己的临时目录, 名字带测试名以免并行互踩
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("icheck-walk-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn put(root: &Path, rel: &str, bytes: usize) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, vec![b'x'; bytes]).unwrap();
    }

    fn names(root: &Path) -> Vec<String> {
        walk(root, &default_excluded())
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect()
    }

    /// 不带通配的规则按路径组件匹配, 所以任意一层命中都排除
    #[test]
    fn excludes_a_component_at_any_depth() {
        let root = scratch("depth");
        put(&root, "keep.txt", 1);
        put(&root, "sub/keep2.txt", 1);
        put(&root, ".iCheck/official/official_hash.json", 1);
        put(&root, "sub/.git/config", 1);
        put(&root, ".cache/blob", 1);

        assert_eq!(names(&root), ["keep.txt", "sub/keep2.txt"]);
    }

    /// 带通配的规则按单层名字匹配, 不跨 `/`
    #[test]
    fn excludes_by_glob_within_one_component() {
        let root = scratch("glob");
        put(&root, "keep.txt", 1);
        put(&root, "poetry.lock", 1);
        put(&root, "sub/other.lock", 1);
        put(&root, "locked/x.txt", 1);

        assert_eq!(names(&root), ["keep.txt", "locked/x.txt"]);
    }

    /// 被排除的目录连同内容一起跳过
    #[test]
    fn an_excluded_directory_hides_its_contents() {
        let root = scratch("dir");
        put(&root, "keep.txt", 1);
        put(&root, "__pycache__/a.pyc", 1);
        put(&root, "sub/__pycache__/b.pyc", 1);
        put(&root, "sub/keep2.txt", 1);

        assert_eq!(names(&root), ["keep.txt", "sub/keep2.txt"]);
    }

    /// 路径一律 POSIX 分隔符, 且按字节序排
    #[test]
    fn sorts_by_name_bytes() {
        let root = scratch("sort");
        put(&root, "c.txt", 1);
        put(&root, "Z.txt", 1);
        put(&root, "a.txt", 1);
        put(&root, "sub/b.txt", 1);

        // 'Z'(0x5A) 在 'a'(0x61) 前面
        assert_eq!(names(&root), ["Z.txt", "a.txt", "c.txt", "sub/b.txt"]);
    }

    /// 大小也算出来, 供 verify 做集合级比对
    #[test]
    fn records_file_size() {
        let root = scratch("size");
        put(&root, "a.bin", 1234);
        let entries = walk(&root, &default_excluded()).unwrap();
        assert_eq!(entries[0].size, 1234);
    }

    /// 文件软链跟随(与 check 的 metadata 口径一致), 目录软链不递归
    #[cfg(unix)]
    #[test]
    fn follows_a_file_symlink_but_not_a_directory_symlink() {
        let root = scratch("symlink");
        put(&root, "real.txt", 7);
        put(&root, "target/inner.txt", 7);
        std::os::unix::fs::symlink(root.join("real.txt"), root.join("link.txt")).unwrap();
        std::os::unix::fs::symlink(root.join("target"), root.join("dirlink")).unwrap();

        assert_eq!(
            names(&root),
            ["link.txt", "real.txt", "target/inner.txt"]
        );
    }

    /// 通配只认 `*` 与 `?`; 这里是纯字符串匹配, `*` 会连 `/` 一起吃
    #[test]
    fn glob_rules() {
        assert!(glob_match("*.lock", "poetry.lock"));
        assert!(glob_match("*.lock", ".lock"));
        assert!(!glob_match("*.lock", "poetry.locked"));

        assert!(glob_match("a?c", "abc"));
        assert!(!glob_match("a?c", "ac"));
        assert!(!glob_match("a?c", "abbc"));

        assert!(glob_match("*", "anything"));
        assert!(glob_match("a*b*c", "axxbyyc"));
        assert!(!glob_match("a*b*c", "axxbyy"));
        assert!(!glob_match("abc", "ab"));
    }

    /// 不跨层是 is_excluded 保证的: 它逐组件调 glob_match, 所以规则永远看不到 `/`
    /// 目录名命中时它的内容也一起被排除
    #[test]
    fn glob_patterns_apply_to_one_component_at_a_time() {
        let pat = ["*.lock".to_string()];
        assert!(is_excluded("poetry.lock", &pat));
        assert!(is_excluded("sub/poetry.lock", &pat));
        assert!(is_excluded("sub/poetry.lock/inner.txt", &pat));
        assert!(!is_excluded("poetry/locked.txt", &pat));
        assert!(!is_excluded("poetry.locked/sub", &pat));
    }
}
