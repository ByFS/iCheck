use std::fmt;

/// 退出码暂定(用户已明确暂缓设计):
///   0 全部通过
///   1 跑完但有非 pass 条目
///   2 用法错误
///   3 工具 / 上游 / 数据故障
#[derive(Debug)]
pub enum Error {
    Usage(String),
    Http(String),
    Upstream(String),
    Data(String),
    Io(std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn exit_code(&self) -> u8 {
        match self {
            Error::Usage(_) => 2,
            _ => 3,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Usage(m) => write!(f, "用法错误: {m}"),
            Error::Http(m) => write!(f, "请求上游失败: {m}"),
            Error::Upstream(m) => write!(f, "上游返回异常: {m}"),
            Error::Data(m) => write!(f, "数据格式错误: {m}"),
            Error::Io(e) => write!(f, "IO 错误: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Data(e.to_string())
    }
}
