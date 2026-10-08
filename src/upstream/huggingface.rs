use serde::Deserialize;

use super::http;
use super::{Entry, Manifest};
use crate::error::{Error, Result};

const ENDPOINT: &str = "https://huggingface.co/api/models";

#[derive(Deserialize)]
struct Repo {
    #[serde(default)]
    siblings: Vec<Sibling>,
}

#[derive(Deserialize)]
struct Sibling {
    rfilename: String,
    size: Option<u64>,
    /// 只有走 LFS 的文件才有
    lfs: Option<Lfs>,
}

#[derive(Deserialize)]
struct Lfs {
    sha256: Option<String>,
}

/// 取某个 revision 的文件清单
///
/// `?blobs=true` 才会带上每个文件的大小与 LFS 信息; 这个接口返回的就是文件清单,
/// 不像 tree 接口还要自己滤掉目录(实测同一个仓库 tree 给 96 条, 其中 7 条是目录)
pub fn fetch(model_id: &str, revision: Option<&str>) -> Result<Manifest> {
    let url = format!("{ENDPOINT}/{model_id}");
    let mut query = vec![("blobs", "true")];
    if let Some(rev) = revision {
        query.push(("revision", rev));
    }
    crate::debug!(
        "GET {url}?blobs=true{}",
        revision.map(|r| format!(" (revision={r})")).unwrap_or_default()
    );

    let (status, body) = http::get(&url, &query)?;
    crate::debug!("body {} bytes", body.len());
    match status {
        200 => {}
        // 仓库不存在时 HF 回的是 401, 正文为 {"error":"Invalid username or password."},
        // 原样抛出去会把打错模型名的人带偏
        401 | 404 => {
            return Err(Error::Http(format!(
                "{url}: HTTP {status}, the repository was not found or it needs a token"
            )));
        }
        other => return Err(Error::Http(format!("{url}: HTTP {other}"))),
    }

    parse(&body)
}

/// 解析响应体
///
/// 与 ModelScope 的差别: HF 只给走 LFS 的文件提供内容 SHA-256(`lfs.sha256`), 其余文件
/// 只有 git blob SHA-1(`blobId`), 而本工具只认内容 SHA-256, 所以后者按"上游不给哈希"处理,
/// 不进结构并计数上报 —— 这条路在 ModelScope 上几乎不触发, 在 HF 上会丢将近一半的文件,
/// 所以计数必须让人看见
fn parse(body: &str) -> Result<Manifest> {
    let repo: Repo = serde_json::from_str(body)?;

    let raw = repo.siblings.len();
    let mut entries = Vec::new();
    let mut no_hash = 0usize;
    let mut no_size = 0usize;
    for s in repo.siblings {
        // name 会被拼到模型根目录上再读, 能跳出去的整份清单都不要
        if !crate::path::is_safe_relative(&s.rfilename) {
            return Err(Error::Upstream(format!(
                "manifest has an unsafe path: {}",
                s.rfilename
            )));
        }
        // 上游获取不到哈希的不进结构
        let sha256 = match s.lfs.as_ref().and_then(|l| l.sha256.as_deref()) {
            Some(v) if !v.trim().is_empty() => v.trim().to_ascii_lowercase(),
            _ => {
                no_hash += 1;
                continue;
            }
        };
        // 没有大小的条目做不了集合级比对
        let Some(size) = s.size else {
            no_size += 1;
            continue;
        };
        entries.push(Entry {
            name: s.rfilename,
            size,
            // HF 不提供"该文件最后一次修改的 commit", 留空
            revision: String::new(),
            sha256,
        });
    }
    crate::debug!(
        "parsed {raw} entries: {} with a content sha256, {no_hash} without lfs, {no_size} without size",
        entries.len()
    );

    if entries.is_empty() {
        return Err(Error::Upstream("manifest is empty".to_string()));
    }
    Ok(Manifest {
        entries,
        uncovered: no_hash + no_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sibling(name: &str, size: Option<u64>, lfs_sha256: Option<&str>) -> String {
        let size = match size {
            Some(v) => v.to_string(),
            None => "null".to_string(),
        };
        let lfs = match lfs_sha256 {
            Some(v) => format!(r#","lfs":{{"sha256":"{v}","size":{size}}}"#),
            None => String::new(),
        };
        format!(
            r#"{{"rfilename":"{name}","blobId":"9b59a7525c7b7870bd2cbdc41a4e91fd63fcb6cb","size":{size}{lfs}}}"#
        )
    }

    /// 只按上游真实字段名拼, 手写字段名错了测试就该红
    fn body(items: &[String]) -> String {
        let mut s = String::from(r#"{"id":"a/b","siblings":["#);
        s.push_str(&items.join(","));
        s.push_str("]}");
        s
    }

    #[test]
    fn keeps_entries_that_have_an_lfs_hash() {
        let raw = body(&[sibling("model.safetensors", Some(988097824), Some("aabb"))]);
        let m = parse(&raw).unwrap();
        assert_eq!(m.entries.len(), 1);
        assert_eq!(m.entries[0].name, "model.safetensors");
        assert_eq!(m.entries[0].size, 988097824);
        assert_eq!(m.entries[0].sha256, "aabb");
        assert_eq!(m.uncovered, 0);
    }

    /// 非 LFS 文件在 HF 上拿不到内容 SHA-256, 不进结构, 但要计数
    #[test]
    fn drops_entries_without_an_lfs_hash() {
        let raw = body(&[
            sibling("config.json", Some(1701), None),
            sibling("tokenizer.json", Some(7000000), None),
            sibling("model.safetensors", Some(100), Some("cc")),
        ]);
        let m = parse(&raw).unwrap();
        assert_eq!(m.entries.len(), 1);
        assert_eq!(m.entries[0].name, "model.safetensors");
        assert_eq!(m.uncovered, 2);
    }

    /// lfs 字段在但 sha256 为空, 等同于没给
    #[test]
    fn treats_an_empty_lfs_sha256_as_missing() {
        let raw = body(&[
            sibling("a.bin", Some(1), Some("")),
            sibling("b.bin", Some(2), Some("   ")),
            sibling("c.bin", Some(3), Some("dd")),
        ]);
        let m = parse(&raw).unwrap();
        assert_eq!(m.entries.len(), 1);
        assert_eq!(m.uncovered, 2);
    }

    #[test]
    fn normalizes_case_and_whitespace() {
        let raw = body(&[sibling("a.bin", Some(1), Some("  AABB  "))]);
        assert_eq!(parse(&raw).unwrap().entries[0].sha256, "aabb");
    }

    /// HF 不给逐文件的 commit, revision 一律留空
    #[test]
    fn leaves_revision_empty() {
        let raw = body(&[sibling("a.bin", Some(1), Some("aa"))]);
        assert_eq!(parse(&raw).unwrap().entries[0].revision, "");
    }

    #[test]
    fn drops_entries_without_a_size() {
        let raw = body(&[
            sibling("no-size.bin", None, Some("aa")),
            sibling("ok.bin", Some(1), Some("bb")),
        ]);
        let m = parse(&raw).unwrap();
        assert_eq!(m.entries.len(), 1);
        assert_eq!(m.entries[0].name, "ok.bin");
        assert_eq!(m.uncovered, 1);
    }

    #[test]
    fn rejects_paths_that_escape_the_model_directory() {
        for bad in ["../outside", "/etc/hostname", "sub/../../outside"] {
            let raw = body(&[sibling(bad, Some(1), Some("aa"))]);
            match parse(&raw) {
                Err(Error::Upstream(m)) => assert!(m.contains("unsafe"), "{bad} -> {m}"),
                other => panic!("{bad} should be rejected, got {other:?}"),
            }
        }
    }

    /// 全是非 LFS 文件时清单为空, 要当错误而不是静默通过
    #[test]
    fn empty_manifest_is_an_error() {
        match parse(&body(&[])) {
            Err(Error::Upstream(m)) => assert!(m.contains("empty"), "{m}"),
            other => panic!("expected an upstream error, got {other:?}"),
        }

        let only_plain = body(&[sibling("config.json", Some(1), None)]);
        assert!(matches!(parse(&only_plain), Err(Error::Upstream(_))));
    }

    #[test]
    fn malformed_body_is_a_data_error() {
        assert!(matches!(parse("not json at all"), Err(Error::Data(_))));
    }
}
