use serde::Deserialize;

use super::{Entry, Manifest};
use crate::error::{Error, Result};

const ENDPOINT: &str = "https://www.modelscope.cn/api/v1/models";

#[derive(Deserialize)]
struct Resp {
    #[serde(rename = "Code")]
    code: i64,
    #[serde(rename = "Success")]
    success: bool,
    #[serde(rename = "Message")]
    message: Option<String>,
    #[serde(rename = "Data")]
    data: Option<Data>,
}

#[derive(Deserialize)]
struct Data {
    #[serde(rename = "Files")]
    files: Vec<FileItem>,
}

#[derive(Deserialize)]
struct FileItem {
    #[serde(rename = "Path")]
    path: String,
    #[serde(rename = "Size")]
    size: u64,
    #[serde(rename = "Sha256")]
    sha256: Option<String>,
    #[serde(rename = "Revision")]
    revision: Option<String>,
    #[serde(rename = "Type")]
    kind: String,
}

/// 取某个 revision 的文件清单
///
/// 只需要一个请求: 同一个响应里既有文件清单, 也有仓库 HEAD 信息
/// 本工具不用 HEAD(不落 revision), 所以不需要做短 SHA 还原
pub fn fetch(model_id: &str, revision: Option<&str>) -> Result<Manifest> {
    let url = format!("{ENDPOINT}/{model_id}/repo/files");
    let rev = revision.unwrap_or("master");
    crate::debug_log!("GET {url} (Revision={rev}, Recursive=true)");

    let started = std::time::Instant::now();
    let resp = ureq::get(&url)
        .query("Revision", rev)
        .query("Recursive", "true")
        .call()
        .map_err(|e| Error::Http(format!("{url}: {e}")))?;
    crate::debug_log!(
        "http {} in {} ms",
        resp.status(),
        started.elapsed().as_millis()
    );

    let body = resp
        .into_string()
        .map_err(|e| Error::Http(format!("failed to read the response: {e}")))?;
    crate::debug_log!("body {} bytes", body.len());
    let parsed: Resp = serde_json::from_str(&body)?;

    if !parsed.success || parsed.code != 200 {
        return Err(Error::Upstream(format!(
            "Code={} Message={}",
            parsed.code,
            parsed.message.unwrap_or_default()
        )));
    }

    let data = parsed
        .data
        .ok_or_else(|| Error::Upstream("response has no Data field".to_string()))?;

    let raw = data.files.len();
    let mut entries = Vec::new();
    let mut no_hash = 0usize;
    let mut dirs = 0usize;
    for item in data.files {
        // Files[] 里同时含文件与目录, 只取 blob
        if item.kind != "blob" {
            dirs += 1;
            continue;
        }
        // 上游获取不到哈希的不进结构
        let sha256 = match item.sha256 {
            Some(s) if !s.trim().is_empty() => s.trim().to_ascii_lowercase(),
            _ => {
                no_hash += 1;
                continue;
            }
        };
        entries.push(Entry {
            name: item.path,
            size: item.size,
            revision: item
                .revision
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase(),
            sha256,
        });
    }
    crate::debug_log!(
        "parsed {raw} entries: {} blobs kept, {dirs} directories skipped, {no_hash} without sha256",
        entries.len()
    );

    if entries.is_empty() {
        return Err(Error::Upstream("manifest is empty".to_string()));
    }
    Ok(Manifest { entries })
}
