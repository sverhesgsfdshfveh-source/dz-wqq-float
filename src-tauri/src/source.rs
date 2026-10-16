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

// ===== Task 5：RSS 源 =====

/// 解析 DZ RSS 2.0（forum.php?mod=rss）。
///
/// - item link 直接作为 url（伪静态由服务端给出）；tid 从 link 的 `thread-N` 提取
/// - description 为 CDATA（原样保留）或普通文本（XML 实体反转义）
/// - **0 个 item 是合法结果**（DZ rss 缓存过期重建的当次请求返回空列表，见 spike-notes/Task 1）
/// - views/replies/digest/authorid/fid RSS 不提供，恒 0；forumname 取 category
pub fn parse_rss(body: &str, base_url: &str) -> anyhow::Result<Vec<Thread>> {
    use quick_xml::events::Event;

    let base = base_url.trim_end_matches('/');
    let mut reader = quick_xml::Reader::from_str(body);
    let mut items: Vec<Thread> = Vec::new();
    let mut buf = Vec::new();
    let mut in_item = false;
    let mut cur_tag = String::new();
    let mut title = String::new();
    let mut link = String::new();
    let mut desc = String::new();
    let mut category = String::new();
    let mut author = String::new();
    let mut pubdate = String::new();

    let mut flush = |title: &mut String,
                     link: &mut String,
                     desc: &mut String,
                     category: &mut String,
                     author: &mut String,
                     pubdate: &mut String,
                     items: &mut Vec<Thread>| {
        // tid 从 link 的 thread-N 提取；无 tid 的残缺 item 跳过
        let tid = extract_tid(link);
        if tid > 0 {
            let url = if link.is_empty() {
                format!("{base}/thread-{tid}-1-1.html")
            } else {
                link.clone()
            };
            items.push(Thread {
                tid,
                subject: std::mem::take(title),
                author: std::mem::take(author),
                authorid: 0,
                dateline: parse_rfc822(pubdate),
                fid: 0,
                forumname: std::mem::take(category),
                views: 0,
                replies: 0,
                digest: 0,
                summary: std::mem::take(desc),
                url,
            });
        }
        title.clear();
        link.clear();
        desc.clear();
        category.clear();
        author.clear();
        pubdate.clear();
    };

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if name == "item" {
                    in_item = true;
                }
                cur_tag = name;
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if name == "item" {
                    in_item = false;
                    flush(
                        &mut title,
                        &mut link,
                        &mut desc,
                        &mut category,
                        &mut author,
                        &mut pubdate,
                        &mut items,
                    );
                }
                cur_tag.clear();
            }
            Ok(Event::Text(t)) => {
                if in_item && !cur_tag.is_empty() {
                    let s = t.unescape().map_err(anyhow::Error::from)?.into_owned();
                    assign_field(&cur_tag, &s, &mut title, &mut link, &mut desc, &mut category, &mut author, &mut pubdate);
                }
            }
            Ok(Event::CData(t)) => {
                if in_item && !cur_tag.is_empty() {
                    let s = String::from_utf8_lossy(&t.into_inner()).into_owned();
                    assign_field(&cur_tag, &s, &mut title, &mut link, &mut desc, &mut category, &mut author, &mut pubdate);
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow::anyhow!("RSS 解析失败: {e}")),
            _ => {}
        }
        buf.clear();
    }
    Ok(items)
}

fn assign_field(
    tag: &str,
    s: &str,
    title: &mut String,
    link: &mut String,
    desc: &mut String,
    category: &mut String,
    author: &mut String,
    pubdate: &mut String,
) {
    match tag {
        "title" => *title = s.to_string(),
        "link" => *link = s.trim().to_string(),
        "description" => *desc = s.to_string(),
        "category" => *category = s.to_string(),
        "author" => *author = s.to_string(),
        "pubDate" => *pubdate = s.to_string(),
        _ => {}
    }
}

/// 从伪静态 link 提取 tid（`thread-334540-1-1.html` → 334540）
fn extract_tid(link: &str) -> u64 {
    let b = link.as_bytes();
    let mut i = 0;
    while i + 7 <= b.len() {
        if &link[i..i + 7] == "thread-" {
            let start = i + 7;
            let mut j = start;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j > start {
                return link[start..j].parse().unwrap_or(0);
            }
        }
        i += 1;
    }
    0
}

/// RSS 没有服务端 changed 语义，由本地比对：只保留比水位新的帖子。
pub fn rss_split(all: Vec<Thread>, last_tid: u64) -> PollResult {
    let top = all.iter().map(|t| t.tid).max().unwrap_or(last_tid);
    if top > last_tid {
        let threads = all.into_iter().filter(|t| t.tid > last_tid).collect();
        PollResult::Changed { top, threads }
    } else {
        PollResult::Unchanged { top }
    }
}

/// 解析 RFC 822 GMT 时间（DZ 输出形如 `Fri, 16 Oct 2026 09:15:03 +0000`），失败返回 0。
fn parse_rfc822(s: &str) -> i64 {
    let s = s.trim();
    let rest = s.split_once(", ").map(|x| x.1).unwrap_or(s);
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() < 5 {
        return 0;
    }
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let day: i64 = parts[0].parse().unwrap_or(0);
    let mon = months.iter().position(|m| *m == parts[1]).map(|i| i as i64 + 1).unwrap_or(1);
    let year: i64 = parts[2].parse().unwrap_or(0);
    let mut hms = parts[3].split(':');
    let h: i64 = hms.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    let m: i64 = hms.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    let sec: i64 = hms.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    let off = parts[4];
    let sign: i64 = if off.starts_with('-') { -1 } else { 1 };
    let oh: i64 = off.get(1..3).and_then(|x| x.parse().ok()).unwrap_or(0);
    let om: i64 = off.get(3..5).and_then(|x| x.parse().ok()).unwrap_or(0);
    days_from_civil(year, mon, day) * 86400 + h * 3600 + m * 60 + sec - sign * (oh * 3600 + om * 60)
}

/// Howard Hinnant days_from_civil：公历日期 → 自 1970-01-01 的天数。
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

#[cfg(test)]
mod rss_tests {
    use super::*;

    const RSS: &str = include_str!("../../tests/fixtures/rss_sample.xml");
    const BASE: &str = "https://bbs.weiququ.cn";

    #[test]
    fn parses_rss_sample() {
        let threads = parse_rss(RSS, BASE).unwrap();
        assert_eq!(threads.len(), 20);
        let t = &threads[0];
        assert!(t.subject.contains("二锅头"));
        assert_eq!(t.tid, 334540);
        assert_eq!(t.url, "https://bbs.weiququ.cn/thread-334540-1-1.html");
        assert_eq!(t.forumname, "综合线报");
        assert_eq!(t.author, "weiququ");
        assert_eq!(t.dateline, 1792142103, "pubDate GMT 应换算为 unix 秒");
        assert!(t.summary.contains("京东搜索"));
    }

    #[test]
    fn rss_entities_are_unescaped() {
        let xml = concat!(
            r#"<rss><channel><item>"#,
            r#"<title>A &amp; B 测试</title>"#,
            r#"<link>https://x/thread-1-1-1.html</link>"#,
            r#"<description>a &amp; b</description>"#,
            r#"<pubDate>Fri, 16 Oct 2026 09:15:03 +0000</pubDate>"#,
            r#"</item></channel></rss>"#
        );
        let t = parse_rss(xml, "https://x").unwrap();
        assert_eq!(t[0].subject, "A & B 测试");
        assert_eq!(t[0].summary, "a & b");
    }

    #[test]
    fn rss_zero_items_is_ok() {
        let xml = r#"<rss version="2.0"><channel><title>x</title></channel></rss>"#;
        let t = parse_rss(xml, BASE).unwrap();
        assert!(t.is_empty());
    }

    #[test]
    fn rss_truncated_degrades_to_empty() {
        // 截断/坏 RSS 不 panic：返回空列表 → 走 Unchanged 路径，下一轮自愈
        // （与 DZ RSS 缓存重建期的合法 0 条同语义，HTTP 层无法区分）
        let t = parse_rss(&RSS[..100], BASE).unwrap_or_default();
        assert!(t.is_empty());
    }

    #[test]
    fn rss_source_poll_computes_changed_and_unchanged() {
        let all = parse_rss(RSS, BASE).unwrap();
        match rss_split(all.clone(), 334539) {
            PollResult::Changed { top, threads } => {
                assert_eq!(top, 334540);
                assert_eq!(threads.len(), 1);
                assert_eq!(threads[0].tid, 334540);
            }
            _ => panic!("水位 334539 应判 Changed"),
        }
        match rss_split(all, 334540) {
            PollResult::Unchanged { top } => assert_eq!(top, 334540),
            _ => panic!("水位 334540 应判 Unchanged"),
        }
    }
}

// ===== 源实例（网络层；解析逻辑已单测，poll 本体在联调覆盖） =====

const HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// 微趣趣专用 JSON 源（秒级）
pub struct WqqApiSource {
    client: reqwest::Client,
    base_url: String,
}
impl WqqApiSource {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
    pub async fn poll(&self, last_tid: u64) -> anyhow::Result<PollResult> {
        let url = format!("{}/wqq_notifier.php?last={}", self.base_url, last_tid);
        let body = self
            .client
            .get(&url)
            .timeout(HTTP_TIMEOUT)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        parse_notifier_json(&body, &self.base_url)
    }
}

/// 通用 DZ 论坛 RSS 源（分钟级）
pub struct RssSource {
    client: reqwest::Client,
    base_url: String,
}
impl RssSource {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
    pub async fn poll(&self, last_tid: u64) -> anyhow::Result<PollResult> {
        let url = format!("{}/forum.php?mod=rss", self.base_url);
        let body = self
            .client
            .get(&url)
            .timeout(HTTP_TIMEOUT)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let all = parse_rss(&body, &self.base_url)?;
        Ok(rss_split(all, last_tid))
    }
}
