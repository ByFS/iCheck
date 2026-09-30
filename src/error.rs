use std::fmt;

/// 退出码
///   0 通过
///   1 仅集合级差异(缺失 / 大小不符 / 多余 / 读不了)
///   2 至少一处内容哈希不符
///   3 工具 / 上游 / 数据 / 用法故障
///
/// 0 到 2 由各命令按 Outcome 返回, 见 main::finish; 这里只负责 3
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
        3
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Usage(m) => write!(f, "{m}"),
            Error::Http(m) => write!(f, "request to upstream failed: {m}"),
            Error::Upstream(m) => write!(f, "unexpected upstream response: {m}"),
            Error::Data(m) => write!(f, "bad data: {m}"),
            Error::Io(e) => write!(f, "io error: {e}"),
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
