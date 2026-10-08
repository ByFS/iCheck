use std::io::Read;
use std::time::Duration;

use serde::Deserialize;

use super::{Entry, Manifest};
use crate::error::{Error, Result};

const ENDPOINT: &str = "https://www.modelscope.cn/api/v1/models";

/// 上游清单正常只有几十 KB, 这里留三个数量级的余量
const MAX_BODY_BYTES: u64 = 32 << 20;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// 两次读到字节之间的静默上限
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// 整个请求的上限, 防止对手一个字节一个字节地喂
const TOTAL_TIMEOUT: Duration = Duration::from_secs(120);

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
    crate::debug!("GET {url} (Revision={rev}, Recursive=true)");

    // ureq 默认只给连接超时, 读没有上限: 上游接受连接之后不吐字节, 这里就会一直挂着
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout_read(READ_TIMEOUT)
        .timeout(TOTAL_TIMEOUT)
        .build();

    let started = std::time::Instant::now();
    let resp = agent
        .get(&url)
        .query("Revision", rev)
        .query("Recursive", "true")
        .call()
        .map_err(|e| Error::Http(format!("{url}: {e}")))?;
    crate::debug!(
        "http {} in {} ms",
        resp.status(),
        started.elapsed().as_millis()
    );

    let body = read_body(resp.into_reader())?;
    crate::debug!("body {} bytes", body.len());

    Ok(Manifest {
        entries: parse(&body)?,
    })
}

/// 读响应体, 超过上限就整份拒绝
///
/// ureq 只给响应头设了上限(100 KB / 100 个), 响应体是没有的: 上游回一个超大
/// 响应就能把内存打满。抽成独立函数是为了能单测, 地址是写死的, 端到端测不了
fn read_body(reader: impl Read) -> Result<String> {
    let mut body = String::new();
    // 多读一个字节, 用来分辨"正好等于上限"与"超了"
    reader
        .take(MAX_BODY_BYTES + 1)
        .read_to_string(&mut body)
        .map_err(|e| Error::Http(format!("failed to read the response: {e}")))?;
    if body.len() as u64 > MAX_BODY_BYTES {
        return Err(Error::Upstream(format!(
            "the manifest is larger than {MAX_BODY_BYTES} bytes, refusing it"
        )));
    }
    Ok(body)
}

/// 解析响应体
///
/// 与网络分开是为了能被单测: 上游的字段名与"哪些条目该丢"的规则都集中在这里,
/// 而这部分是上游一变就会坏掉的地方
fn parse(body: &str) -> Result<Vec<Entry>> {
    let parsed: Resp = serde_json::from_str(body)?;

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
        // name 会被拼到模型根目录上再读, 能跳出去的整份清单都不要
        if !crate::path::is_safe_relative(&item.path) {
            return Err(Error::Upstream(format!(
                "manifest has an unsafe path: {}",
                item.path
            )));
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
    crate::debug!(
        "parsed {raw} entries: {} blobs kept, {dirs} directories skipped, {no_hash} without sha256",
        entries.len()
    );

    if entries.is_empty() {
        return Err(Error::Upstream("manifest is empty".to_string()));
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一个 Files[] 条目, 只写测试关心的字段
    fn item(path: &str, size: u64, sha: Option<&str>, rev: Option<&str>, kind: &str) -> String {
        let sha = match sha {
            Some(s) => format!("\"{s}\""),
            None => "null".to_string(),
        };
        let rev = match rev {
            Some(r) => format!("\"{r}\""),
            None => "null".to_string(),
        };
        format!(r#"{{"Path":"{path}","Size":{size},"Sha256":{sha},"Revision":{rev},"Type":"{kind}"}}"#)
    }

    fn blob(path: &str) -> String {
        item(path, 3, Some("aa"), Some("master"), "blob")
    }

    /// 只按上游真实字段名拼, 手写字段名错了测试就该红
    fn body(items: &[String]) -> String {
        let mut s = String::from(r#"{"Code":200,"Success":true,"Message":"ok","Data":{"Files":["#);
        s.push_str(&items.join(","));
        s.push_str("]}}");
        s
    }

    /// 目录条目不该进清单: 上游把文件和目录放在同一个数组里
    #[test]
    fn keeps_blobs_and_drops_directories() {
        let raw = body(&[item("sub", 0, None, None, "tree"), blob("sub/a.txt")]);
        let entries = parse(&raw).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "sub/a.txt");
    }

    /// 大小写和空白都在这里抹平, 下游一律拿小写比较
    #[test]
    fn normalizes_sha256_and_revision() {
        let raw = body(&[item("a.txt", 1, Some("AABB"), Some("  MASTER  "), "blob")]);
        let entries = parse(&raw).unwrap();
        assert_eq!(entries[0].sha256, "aabb");
        assert_eq!(entries[0].revision, "master");
    }

    /// 拿不到哈希的条目没有校验价值, 不进结构
    #[test]
    fn drops_entries_without_sha256() {
        let raw = body(&[
            item("none.txt", 1, None, None, "blob"),
            item("empty.txt", 1, Some(""), None, "blob"),
            item("blank.txt", 1, Some("   "), None, "blob"),
            blob("kept.txt"),
        ]);
        let entries = parse(&raw).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "kept.txt");
    }

    /// revision 缺失落成空串, 不 panic
    #[test]
    fn missing_revision_becomes_empty() {
        let raw = body(&[item("a.txt", 1, Some("aa"), None, "blob")]);
        assert_eq!(parse(&raw).unwrap()[0].revision, "");
    }

    /// Code / Success 不对时报的是上游错误, 带上 Message 便于定位
    #[test]
    fn reports_upstream_failure() {
        let raw = r#"{"Code":404,"Success":false,"Message":"model not found"}"#;
        match parse(raw) {
            Err(Error::Upstream(m)) => {
                assert!(m.contains("404"), "{m}");
                assert!(m.contains("model not found"), "{m}");
            }
            other => panic!("expected an upstream error, got {other:?}"),
        }
    }

    #[test]
    fn missing_data_field_is_an_error() {
        let raw = r#"{"Code":200,"Success":true}"#;
        match parse(raw) {
            Err(Error::Upstream(m)) => assert!(m.contains("no Data"), "{m}"),
            other => panic!("expected an upstream error, got {other:?}"),
        }
    }

    /// 空清单当错误: 静默通过会让 check 报一个全 0 的 PASS
    #[test]
    fn empty_manifest_is_an_error() {
        let empty = body(&[]);
        match parse(&empty) {
            Err(Error::Upstream(m)) => assert!(m.contains("empty"), "{m}"),
            other => panic!("expected an upstream error, got {other:?}"),
        }

        // 只有目录也一样
        let only_dirs = body(&[item("sub", 0, None, None, "tree")]);
        assert!(matches!(parse(&only_dirs), Err(Error::Upstream(_))));
    }

    #[test]
    fn malformed_body_is_a_data_error() {
        assert!(matches!(parse("not json at all"), Err(Error::Data(_))));
    }

    #[test]
    fn reads_a_normal_body() {
        assert_eq!(read_body(&b"{\"Code\":200}"[..]).unwrap(), "{\"Code\":200}");
    }

    /// 上游回一个超大响应不能把内存打满
    #[test]
    fn refuses_a_body_over_the_cap() {
        let huge = vec![b'x'; (MAX_BODY_BYTES + 10) as usize];
        match read_body(&huge[..]) {
            Err(Error::Upstream(m)) => assert!(m.contains("larger than"), "{m}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn rejects_a_body_that_is_not_utf8() {
        assert!(matches!(read_body(&[0xff, 0xfe][..]), Err(Error::Http(_))));
    }

    /// 上游能指定任意路径的话, check 就会去读模型目录之外的文件
    #[test]
    fn rejects_paths_that_escape_the_model_directory() {
        for bad in ["../outside", "/etc/hostname", "sub/../../outside", ".."] {
            let raw = body(&[item(bad, 3, Some("aa"), Some("master"), "blob")]);
            match parse(&raw) {
                Err(Error::Upstream(m)) => assert!(m.contains("unsafe"), "{bad} -> {m}"),
                other => panic!("{bad} should be rejected, got {other:?}"),
            }
        }
    }

    /// 目录条目不参与拼路径, 名字再怪也不该让整份清单失败
    #[test]
    fn ignores_odd_paths_on_directories() {
        let raw = body(&[item("../odd", 0, None, None, "tree"), blob("a.txt")]);
        assert_eq!(parse(&raw).unwrap().len(), 1);
    }
}
