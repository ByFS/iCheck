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
            } else if file_type.is_symlink() {
                if let Ok(md) = std::fs::metadata(entry.path()) {
                    if md.is_file() {
                        out.push(WalkEntry {
                            name: rel,
                            size: md.len(),
                        });
                    }
                }
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
