pub mod modelscope;

use crate::error::Result;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Platform {
    ModelScope,
}

impl Platform {
    pub fn parse(s: &str) -> Option<Platform> {
        match s {
            "ms" | "modelscope" => Some(Platform::ModelScope),
            _ => None,
        }
    }

    /// 落进 official_hash 的 source.platform
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::ModelScope => "modelscope",
        }
    }

    /// 给用户看的名字, 用于 INFO: Source: 那一行
    pub fn display_name(self) -> &'static str {
        match self {
            Platform::ModelScope => "ModelScope",
        }
    }

    /// 从落盘的 source.platform 反查显示名, 认不出来就原样返回
    pub fn display_for(platform: &str) -> &str {
        match platform {
            "modelscope" => "ModelScope",
            "huggingface" => "HuggingFace",
            other => other,
        }
    }

    /// 落进 official_hash 的 source.url, 仅溯源
    pub fn page_url(self, model_id: &str) -> String {
        match self {
            Platform::ModelScope => format!("https://modelscope.cn/models/{model_id}"),
        }
    }
}

/// 适配器对外的规范化条目平台特有的字段名在各自模块里消化掉
#[derive(Clone, Debug)]
pub struct Entry {
    /// 仓库相对路径, POSIX 分隔符, 含子目录
    pub name: String,
    /// 上游报的内容字节数
    pub size: u64,
    /// 该文件最后一次修改的 commitModelScope 有, 其它平台可能为空
    pub revision: String,
    /// 内容 SHA-256, 小写 64 hex
    pub sha256: String,
}

#[derive(Clone, Debug)]
pub struct Manifest {
    pub entries: Vec<Entry>,
}

pub fn fetch(platform: Platform, model_id: &str, revision: Option<&str>) -> Result<Manifest> {
    match platform {
        Platform::ModelScope => modelscope::fetch(model_id, revision),
    }
}
