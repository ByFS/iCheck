use std::path::{Component, Path};

/// 名字是不是一个安全的相对路径
///
/// 上游清单与本地锚点文件里的 `name` 都会拼到模型根目录上再读, 而 `Path::join` 遇到
/// 绝对路径会**替换**基路径, 遇到 `..` 会跳出去 —— 也就是说一个被篡改的上游响应能让
/// 工具去读模型目录之外的任意文件, 并把它的哈希写进 JSON
///
/// 判定交给 `Component`: 只接受 `Normal` 组件, 绝对路径, 盘符, `.` 与 `..` 一律拒绝
/// (在 Windows 上 `C:\x` 会解析成 `Prefix`, 在这里自动被挡住, 不需要额外的黑名单)
pub fn is_safe_relative(name: &str) -> bool {
    if name.is_empty() || name.contains('\0') {
        return false;
    }
    let mut seen = false;
    for c in Path::new(name).components() {
        match c {
            Component::Normal(_) => seen = true,
            _ => return false,
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_relative_names() {
        assert!(is_safe_relative("config.json"));
        assert!(is_safe_relative("sub/notes.md"));
        assert!(is_safe_relative("a/b/c/d.bin"));
        // 点文件是正常的上游条目, 不能被误伤
        assert!(is_safe_relative(".gitattributes"));
        assert!(is_safe_relative("sub/.gitattributes"));
        // 重复与末尾的分隔符由 components() 折叠, 落点不变
        assert!(is_safe_relative("a//b"));
        assert!(is_safe_relative("sub/"));
        // 中间的 `.` 也被规范化掉, 落点仍在根目录之内
        assert!(is_safe_relative("a/./b"));
    }

    #[test]
    fn rejects_escaping_the_root() {
        assert!(!is_safe_relative(""));
        assert!(!is_safe_relative("/etc/hostname"));
        assert!(!is_safe_relative("../../../../etc/hostname"));
        assert!(!is_safe_relative("sub/../../outside"));
        assert!(!is_safe_relative(".."));
        assert!(!is_safe_relative("a/.."));
        assert!(!is_safe_relative("a/../b"));
    }

    #[test]
    fn rejects_odd_components() {
        // 开头的 `.` 不会被规范化掉, 会作为 CurDir 留在组件里
        assert!(!is_safe_relative("."));
        assert!(!is_safe_relative("./a"));
        assert!(!is_safe_relative("with\0nul"));
    }
}
