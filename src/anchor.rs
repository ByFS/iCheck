use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::official::Source;

pub const OFFICIAL_REL: &str = ".iCheck/official/official_hash.json";
pub const ANCHOR_HASH_REL: &str = ".iCheck/anchor/anchor_hash.json";
pub const ANCHOR_INDEX_REL: &str = ".iCheck/anchor/anchor_index.json";

pub fn official_path(root: &Path) -> PathBuf {
    root.join(OFFICIAL_REL)
}

pub fn hash_path(root: &Path) -> PathBuf {
    root.join(ANCHOR_HASH_REL)
}

pub fn index_path(root: &Path) -> PathBuf {
    root.join(ANCHOR_INDEX_REL)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AnchorFile {
    pub name: String,
    pub size: u64,
    pub blake3: String,
}

/// anchor_hash.json, 覆盖本地目录的全部文件, 不存上游字段
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AnchorHash {
    pub tool: String,
    pub computed_at: String,
    pub files: Vec<AnchorFile>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Summary {
    pub total_files: usize,
    pub official_files: usize,
    pub blake3_files: usize,
}

/// anchor_index.json, 只有模型信息与配置, 不记录任何哈希值
/// 没有 files[], 文件清单的唯一归属是 anchor_hash
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AnchorIndex {
    pub tool: String,
    pub created_at: String,
    pub updated_at: String,
    pub official_hash: String,
    pub anchor_hash: String,
    pub excluded: Vec<String>,
    pub source: Source,
    pub summary: Summary,
}
