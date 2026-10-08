use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{de::DeserializeOwned, Serialize};

use crate::error::{Error, Result};

/// 读 JSON, 文件不存在返回 None
pub fn load<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// 写 JSON, 4 空格缩进, 原子落盘
pub fn save<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut buf = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
    value.serialize(&mut ser)?;
    buf.push(b'\n');
    let result = write_atomic(path, &buf);
    crate::debug!(
        "save {} ({} bytes): {}",
        path.display(),
        buf.len(),
        if result.is_ok() { "ok" } else { "failed" }
    );
    result
}

/// 写临时文件 -> fsync -> 原子改名 -> fsync 目录
///
/// 中间两个 fsync 不能省: 少了它们, 断电时 rename 可能还没落盘, 续跑就白做了
///
/// 临时名带进程号与纳秒时间戳, 并且用 `create_new`(等价 O_EXCL)打开, 因为固定名字的临时
/// 文件会被人拿去做文章: 打开时跟随同名符号链接, 写下去等于覆盖任意一个本进程有权写的文件,
/// 紧接着的 rename 还会把那根符号链接搬到目标路径上, 之后每次写入都从链接走
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| Error::Data(format!("path has no parent directory: {}", path.display())))?;
    fs::create_dir_all(dir)?;

    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "out.json".to_string());
    let tmp = dir.join(format!(".{name}.{}.tmp", nonce()));

    let written = (|| -> std::io::Result<()> {
        let mut f = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, path)?;
        if let Ok(d) = File::open(dir) {
            let _ = d.sync_all();
        }
        Ok(())
    })();

    if written.is_err() {
        // 失败时别把临时文件留下; 已经改名成功的话这里什么也删不到
        let _ = fs::remove_file(&tmp);
    }
    Ok(written?)
}

/// 临时名只要本地攻击者猜不到就够, 不要求密码学随机
fn nonce() -> u128 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    nanos ^ (u128::from(std::process::id()) << 64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Doc {
        n: u32,
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("icheck-jsonio-{tag}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn round_trips() {
        let dir = scratch("round");
        let path = dir.join("sub/out.json");
        save(&path, &Doc { n: 7 }).unwrap();
        assert_eq!(load::<Doc>(&path).unwrap(), Some(Doc { n: 7 }));
    }

    /// 旧实现用固定的 `.{name}.tmp`, 在那上面放一根符号链接就能把写入引到别处
    /// 这条测试钉住"不再用可预测的临时名"
    #[cfg(unix)]
    #[test]
    fn does_not_write_through_a_symlink_at_the_temp_path() {
        let dir = scratch("temp");
        let victim = dir.join("victim.txt");
        fs::write(&victim, b"untouched").unwrap();
        std::os::unix::fs::symlink(&victim, dir.join(".out.json.tmp")).unwrap();

        let path = dir.join("out.json");
        save(&path, &Doc { n: 1 }).unwrap();

        assert_eq!(fs::read(&victim).unwrap(), b"untouched");
        assert_eq!(load::<Doc>(&path).unwrap(), Some(Doc { n: 1 }));
    }

    /// 目标路径本身是符号链接时, rename 应该换掉链接而不是写穿它
    #[cfg(unix)]
    #[test]
    fn replaces_a_symlink_at_the_destination() {
        let dir = scratch("dest");
        let victim = dir.join("victim.txt");
        fs::write(&victim, b"untouched").unwrap();

        let path = dir.join("out.json");
        std::os::unix::fs::symlink(&victim, &path).unwrap();

        save(&path, &Doc { n: 2 }).unwrap();

        assert_eq!(fs::read(&victim).unwrap(), b"untouched");
        assert!(fs::symlink_metadata(&path).unwrap().is_file());
        assert_eq!(load::<Doc>(&path).unwrap(), Some(Doc { n: 2 }));
    }
}
