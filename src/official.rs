use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::upstream::{Entry, Platform};

/// files[].check 的取值
/// pending 是唯一未判定的状态; 其余都是终态
/// "文件已校验完成" 定义为: 不存在 pending 条目
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum CheckState {
    /// 上游哈希已获取, 本地尚未校验
    Pending,
    /// 本地 SHA-256 与上游一致
    Pass,
    /// 本地大小或 SHA-256 与上游不一致
    Fail,
    /// 上游清单里有, 本地目录中不存在
    Missing,
    /// 本地存在但读不了(权限 / IO 错误 / 不是普通文件)
    Unreadable,
}

impl CheckState {
    pub fn is_terminal(self) -> bool {
        !matches!(self, CheckState::Pending)
    }

    /// 结果块里用的标签
    pub fn tag(self) -> &'static str {
        match self {
            CheckState::Pass => "[PASS]",
            CheckState::Fail => "[FAIL]",
            CheckState::Missing => "[MISSING]",
            CheckState::Unreadable => "[UNREADABLE]",
            CheckState::Pending => "[PENDING]",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Source {
    pub platform: String,
    pub model_id: String,
    pub url: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileEntry {
    pub name: String,
    pub size: u64,
    pub revision: String,
    pub sha256: String,
    pub check: CheckState,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OfficialHash {
    pub tool: String,
    pub fetched_at: String,
    pub source: Source,
    pub files: Vec<FileEntry>,
}

/// 落盘位置: <模型目录>/.iCheck/official/official_hash.json
pub fn path_in(root: &Path) -> PathBuf {
    root.join(crate::anchor::OFFICIAL_REL)
}

impl OfficialHash {
    /// 从上游清单新建, 全部条目为 pending按 name 字节序排序, 保证输出与并发无关
    pub fn new(platform: Platform, model_id: &str, entries: &[Entry]) -> Self {
        let mut files: Vec<FileEntry> = entries
            .iter()
            .map(|e| FileEntry {
                name: e.name.clone(),
                size: e.size,
                revision: e.revision.clone(),
                sha256: e.sha256.clone(),
                check: CheckState::Pending,
            })
            .collect();
        files.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));

        OfficialHash {
            tool: crate::TOOL.to_string(),
            fetched_at: crate::now_rfc3339(),
            source: Source {
                platform: platform.as_str().to_string(),
                model_id: model_id.to_string(),
                url: platform.page_url(model_id),
            },
            files,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.files.iter().all(|f| f.check.is_terminal())
    }

    pub fn count(&self, s: CheckState) -> usize {
        self.files.iter().filter(|f| f.check == s).count()
    }
}
