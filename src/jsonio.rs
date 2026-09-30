use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

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
    write_atomic(path, &buf)
}

/// 写临时文件 -> fsync -> 原子改名 -> fsync 目录
///
/// 中间两个 fsync 不能省: 少了它们, 断电时 rename 可能还没落盘, 续跑就白做了
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| Error::Data(format!("路径没有父目录: {}", path.display())))?;
    fs::create_dir_all(dir)?;

    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "out.json".to_string());
    let tmp = dir.join(format!(".{name}.tmp"));

    {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    if let Ok(d) = File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}
