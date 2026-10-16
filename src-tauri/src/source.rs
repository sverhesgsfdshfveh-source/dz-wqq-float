//! 数据源与统一帖子模型。
//!
//! 本文件先实现 wqq_notifier.php 的 JSON 契约解析；
//! RSS 源与两个 Source 实现在 Task 5 加入。

use serde::Serialize;

/// 统一帖子模型（wqq JSON 与 RSS 两条源都映射到它）。
#[derive(Debug, Clone, Serialize)]
pub struct Thread {
    pub tid: u64,
    pub subject: String,
    pub author: String,
    pub authorid: u64,
    /// 发帖时间，unix 秒
    pub dateline: i64,
    pub fid: u64,
    pub forumname: String,
    pub views: u64,
    pub replies: u64,
    pub digest: u8,
    /// 首帖摘要（受限帖为空串）
    pub summary: String,
    /// 帖子网页地址（wqq 源拼伪静态，RSS 源用 item 自带 link）
    pub url: String,
}

/// 一次轮询的结果。
#[derive(Debug)]
pub enum PollResult {
    /// 无新帖，top 为服务端当前最大 tid
    Unchanged { top: u64 },
    /// 有新帖，threads 按 dateline 倒序（最新在 [0]），top 为列表最大 tid
    Changed { top: u64, threads: Vec<Thread> },
}

/// 解析 wqq_notifier.php 响应体。
///
/// `base_url` 用于拼接帖子伪静态地址 `thread-{tid}-1-1.html`，末尾 `/` 会被归一化。
pub fn parse_notifier_json(body: &str, base_url: &str) -> anyhow::Result<PollResult> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| anyhow::anyhow!("非 JSON 响应: {e}"))?;
    if v.get("ok").and_then(|x| x.as_i64()) != Some(1) {
        anyhow::bail!("响应 ok != 1");
    }
    let base = base_url.trim_end_matches('/');
    let top = v.get("top").and_then(|x| x.as_u64()).unwrap_or(0);
    match v.get("changed").and_then(|x| x.as_u64()) {
        Some(0) => Ok(PollResult::Unchanged { top }),
        Some(_) => {
            let arr = v
                .get("threads")
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default();
            let mut threads = Vec::with_capacity(arr.len());
            for it in &arr {
                let tid = it.get("tid").and_then(|x| x.as_u64()).unwrap_or(0);
                threads.push(Thread {
                    tid,
                    subject: it
                        .get("subject")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    author: it
                        .get("author")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    authorid: it.get("authorid").and_then(|x| x.as_u64()).unwrap_or(0),
                    dateline: it.get("dateline").and_then(|x| x.as_i64()).unwrap_or(0),
                    fid: it.get("fid").and_then(|x| x.as_u64()).unwrap_or(0),
                    forumname: it
                        .get("forumname")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    views: it.get("views").and_then(|x| x.as_u64()).unwrap_or(0),
                    replies: it.get("replies").and_then(|x| x.as_u64()).unwrap_or(0),
                    digest: it.get("digest").and_then(|x| x.as_u64()).unwrap_or(0) as u8,
                    summary: it
                        .get("summary")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    url: format!("{base}/thread-{tid}-1-1.html"),
                });
            }
            Ok(PollResult::Changed { top, threads })
        }
        None => anyhow::bail!("响应缺少 changed 字段"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHANGED0: &str = include_str!("../../tests/fixtures/notifier_changed0.json");
    const CHANGED1: &str = include_str!("../../tests/fixtures/notifier_changed1.json");
    const BASE: &str = "https://bbs.weiququ.cn";

    #[test]
    fn parses_unchanged_response() {
        let res = parse_notifier_json(CHANGED0, BASE).unwrap();
        match res {
            PollResult::Unchanged { top } => assert!(top > 330000, "top 应为真实水位: {top}"),
            _ => panic!("应为 Unchanged，得到 Changed"),
        }
    }

    #[test]
    fn parses_changed_response() {
        let res = parse_notifier_json(CHANGED1, BASE).unwrap();
        match res {
            PollResult::Changed { top, threads } => {
                assert!(!threads.is_empty() && threads.len() <= 20);
                // 最新在 [0]：首条 tid 即列表最大
                let max_tid = threads.iter().map(|t| t.tid).max().unwrap();
                assert_eq!(threads[0].tid, max_tid);
                assert_eq!(top, max_tid);
                let t = &threads[0];
                assert_eq!(
                    t.url,
                    format!("{BASE}/thread-{}-1-1.html", t.tid),
                    "url 应拼伪静态"
                );
                assert!(t.subject.contains("二锅头"), "首条 subject 应为 fixture 真实值");
                assert!(!t.author.is_empty());
                assert!(t.dateline > 1760000000, "dateline 应为 2025-10 之后的 unix 秒");
                assert!(!t.forumname.is_empty());
                assert!(!t.summary.is_empty(), "首帖非受限帖，summary 非空");
            }
            _ => panic!("应为 Changed，得到 Unchanged"),
        }
    }

    #[test]
    fn base_url_trailing_slash_is_normalized() {
        let res = parse_notifier_json(CHANGED1, "https://bbs.weiququ.cn/").unwrap();
        if let PollResult::Changed { threads, .. } = res {
            assert_eq!(
                threads[0].url,
                format!("https://bbs.weiququ.cn/thread-{}-1-1.html", threads[0].tid)
            );
        } else {
            panic!("应为 Changed");
        }
    }

    #[test]
    fn rejects_html_garbage_and_empty() {
        assert!(parse_notifier_json("<html>503 Service</html>", BASE).is_err());
        assert!(parse_notifier_json("", BASE).is_err());
    }

    #[test]
    fn rejects_bad_ok_and_missing_changed() {
        assert!(parse_notifier_json(r#"{"ok":0,"changed":0}"#, BASE).is_err());
        assert!(parse_notifier_json(r#"{"ok":1}"#, BASE).is_err());
    }
}
