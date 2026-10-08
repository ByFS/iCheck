//! 上游请求的公共部分: 超时, 响应体上限, 状态码透传
//!
//! 两个平台的接口形态不同, 但"怎么发一次请求"是一样的, 集中在这里只有一处要改

use std::io::Read;
use std::time::{Duration, Instant};

use crate::error::{Error, Result};

/// 上游清单正常只有几十 KB, 这里留三个数量级的余量
const MAX_BODY_BYTES: u64 = 32 << 20;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// 两次读到字节之间的静默上限
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// 整个请求的上限, 防止对手一个字节一个字节地喂
const TOTAL_TIMEOUT: Duration = Duration::from_secs(120);

/// 发一次 GET, 返回状态码与响应体
///
/// 非 2xx 不在这里当成错误: 上游的错误正文往往有信息(HuggingFace 用 401 表示仓库不存在),
/// 交给各适配器判断怎么报
pub fn get(url: &str, query: &[(&str, &str)]) -> Result<(u16, String)> {
    // ureq 默认只给连接超时, 读是没有上限的: 上游接受连接之后不吐字节, 这里就会一直挂着
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout_read(READ_TIMEOUT)
        .timeout(TOTAL_TIMEOUT)
        .build();

    let started = Instant::now();
    let mut req = agent.get(url);
    for (k, v) in query {
        req = req.query(k, v);
    }

    let resp = match req.call() {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            // 错误正文读不出来也不影响报状态码, 所以这里不往上抛
            let body = read_body(r.into_reader()).unwrap_or_default();
            crate::debug!("http {code} in {} ms", started.elapsed().as_millis());
            return Ok((code, body));
        }
        Err(e) => return Err(Error::Http(format!("{url}: {e}"))),
    };

    let status = resp.status();
    let body = read_body(resp.into_reader())?;
    crate::debug!("http {status} in {} ms", started.elapsed().as_millis());
    Ok((status, body))
}

/// 读响应体, 超过上限就整份拒绝
///
/// ureq 只给响应头设了上限(100 KB / 100 个), 响应体是没有的: 上游回一个超大
/// 响应就能把内存打满; 抽成独立函数是为了能单测, 地址写死在各适配器里, 端到端测不了
fn read_body(reader: impl Read) -> Result<String> {
    let mut body = String::new();
    // 多读一个字节, 用来分辨"正好等于上限"与"超了"
    reader
        .take(MAX_BODY_BYTES + 1)
        .read_to_string(&mut body)
        .map_err(|e| Error::Http(format!("failed to read the response: {e}")))?;
    if body.len() as u64 > MAX_BODY_BYTES {
        return Err(Error::Upstream(format!(
            "the response is larger than {MAX_BODY_BYTES} bytes, refusing it"
        )));
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
