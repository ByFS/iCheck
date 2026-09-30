use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest as _, Sha256};

use crate::error::Result;

/// 每 worker 的读缓冲 4 MiB 在 HDD 到 NVMe 上都不算小, 也不会撑爆内存
const BUF_SIZE: usize = 4 << 20;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Algorithm {
    Sha256,
    Blake3,
}

impl Algorithm {
    pub fn name(self) -> &'static str {
        match self {
            Algorithm::Sha256 => "SHA-256",
            Algorithm::Blake3 => "BLAKE3",
        }
    }
}

/// 让两种算法共用同一段读循环
trait Streaming {
    fn update(&mut self, bytes: &[u8]);
    fn finish(self: Box<Self>) -> String;
}

struct Sha256Stream(Sha256);

impl Streaming for Sha256Stream {
    fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes)
    }
    fn finish(self: Box<Self>) -> String {
        to_hex(&self.0.finalize())
    }
}

struct Blake3Stream(blake3::Hasher);

impl Streaming for Blake3Stream {
    fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }
    fn finish(self: Box<Self>) -> String {
        self.0.finalize().to_hex().to_string()
    }
}

/// 计算文件的哈希, 返回十六进制摘要与读到的字节数
///
/// 每读完一个缓冲就把字节数累加进 progress 并发控制器必须看这个连续读数,
/// 不能看"每窗口完成的文件字节", 文件大小悬殊时, 一个大文件完成会产生尖峰,
/// 随后的尾巴会被误判成吞吐崩塌
///
/// TODO 后续可加 posix_fadvise(SEQUENTIAL) / madvise(MADV_SEQUENTIAL);
///      两个轴不能同时开, 文件内并发只对 BLAKE3 有意义, SHA-256 结构上不可并行
pub fn hash_file(path: &Path, algo: Algorithm, progress: &AtomicU64) -> Result<(String, u64)> {
    let mut stream: Box<dyn Streaming> = match algo {
        Algorithm::Sha256 => Box::new(Sha256Stream(Sha256::new())),
        Algorithm::Blake3 => Box::new(Blake3Stream(blake3::Hasher::new())),
    };

    let mut file = File::open(path)?;
    let mut buf = vec![0u8; BUF_SIZE];
    let mut read_total: u64 = 0;

    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        stream.update(&buf[..n]);
        read_total += n as u64;
        progress.fetch_add(n as u64, Ordering::Relaxed);
    }

    Ok((stream.finish(), read_total))
}

pub fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

/// 对内存里的一段字节求哈希, 供开工前的探测用
pub fn hash_bytes(bytes: &[u8], algo: Algorithm) -> String {
    match algo {
        Algorithm::Sha256 => to_hex(&Sha256::digest(bytes)),
        Algorithm::Blake3 => blake3::hash(bytes).to_hex().to_string(),
    }
}
