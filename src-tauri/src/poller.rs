//! 轮询内核：退避、水位推进、通道兜底判定。
//!
//! 三个纯函数可单测；`run_poller` 异步循环在 Task 7 组装、联调覆盖（Task 8/10）。

use crate::config::{Config, SourceKind};
use crate::source::{PollResult, Thread};
use std::time::Duration;

/// 轮询间隔：连续失败 <3 次用正常间隔，>=3 次退避到 30 秒。
pub fn next_delay(interval: Duration, consecutive_failures: u32) -> Duration {
    if consecutive_failures >= 3 {
        Duration::from_secs(30)
    } else {
        interval
    }
}

/// 推进水位并返回需要展示的新帖。
///
/// - Changed → (top, threads)
/// - Unchanged → (max(top, watermark) 防数据回退, 空)
pub fn apply_result(watermark: u64, res: &PollResult) -> (u64, Vec<Thread>) {
    match res {
        PollResult::Unchanged { top } => ((*top).max(watermark), Vec::new()),
        PollResult::Changed { top, threads } => ((*top).max(watermark), threads.clone()),
    }
}

/// 通道选择：配置源优先；仅当配置为 Wqq 且连败 >=6 时切 Rss 兜底（Task 1 ledger：RSS 通道分钟级）。
pub fn pick_source(cfg: &Config, consecutive_failures: u32) -> SourceKind {
    if cfg.source == SourceKind::Wqq && consecutive_failures >= 6 {
        SourceKind::Rss
    } else {
        cfg.source.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::default_config;

    #[test]
    fn backoff_after_three_failures() {
        let i = Duration::from_secs(5);
        assert_eq!(next_delay(i, 0), i);
        assert_eq!(next_delay(i, 2), i);
        assert_eq!(next_delay(i, 3), Duration::from_secs(30));
        assert_eq!(next_delay(i, 9), Duration::from_secs(30));
    }

    #[test]
    fn apply_unchanged_keeps_monotonic_watermark() {
        // 数据回退：服务器 top 小于本地水位 → 水位不回退
        let (wm, t) = apply_result(334500, &PollResult::Unchanged { top: 100 });
        assert_eq!(wm, 334500);
        assert!(t.is_empty());
        // 正常推进
        let (wm, t) = apply_result(334500, &PollResult::Unchanged { top: 334600 });
        assert_eq!(wm, 334600);
        assert!(t.is_empty());
    }

    #[test]
    fn apply_changed_returns_threads_and_top() {
        let th = Thread {
            tid: 334601,
            subject: "s".into(),
            author: "a".into(),
            authorid: 1,
            dateline: 1,
            fid: 2,
            forumname: "f".into(),
            views: 0,
            replies: 0,
            digest: 0,
            summary: String::new(),
            url: "u".into(),
        };
        let (wm, t) = apply_result(
            334600,
            &PollResult::Changed {
                top: 334601,
                threads: vec![th],
            },
        );
        assert_eq!(wm, 334601);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].tid, 334601);
    }

    #[test]
    fn pick_source_falls_back_after_six_failures() {
        let mut cfg = default_config(); // source = Wqq
        assert_eq!(pick_source(&cfg, 3), SourceKind::Wqq);
        assert_eq!(pick_source(&cfg, 6), SourceKind::Rss);
        cfg.source = SourceKind::Rss;
        assert_eq!(pick_source(&cfg, 0), SourceKind::Rss);
        assert_eq!(pick_source(&cfg, 99), SourceKind::Rss);
    }
}

// ===== 轮询循环（组装层，联调覆盖） =====

use crate::log as wqqlog;
use crate::source::{RssSource, WqqApiSource};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

/// 轮询主循环：快照共享状态 → 选源 → poll → 推水位/发事件 → 按退避睡眠。
pub async fn run_poller(
    app: AppHandle,
    state: Arc<Mutex<crate::state::SharedState>>,
    log_dir: PathBuf,
    config_path: PathBuf,
) {
    let mut failures: u32 = 0;
    let mut last_base = String::new();
    let mut wqq_src: Option<WqqApiSource> = None;
    let mut rss_src: Option<RssSource> = None;

    loop {
        let (interval, watermark, base, cfg_source) = {
            let s = state.lock().expect("state lock poisoned");
            (
                crate::config::clamp_interval(s.cfg.poll_interval_secs),
                s.watermark,
                s.cfg.forum_base.clone(),
                s.cfg.source.clone(),
            )
        };

        // 窗口位置有变动（Moved 事件标脏）→ 落盘
        let pos_dirty = {
            let mut s = state.lock().expect("state lock poisoned");
            let d = s.cfg_pos_dirty;
            s.cfg_pos_dirty = false;
            d
        };
        if pos_dirty {
            let cfg = state.lock().expect("state lock poisoned").cfg.clone();
            let _ = crate::config::save(&config_path, &cfg);
        }

        // 换论坛地址时重建源（reqwest 连接池随 base 失效）
        if base != last_base || wqq_src.is_none() {
            wqq_src = Some(WqqApiSource::new(&base));
            rss_src = Some(RssSource::new(&base));
            last_base = base.clone();
        }

        let kind = pick_source(
            &crate::config::Config {
                forum_base: base.clone(),
                source: cfg_source.clone(),
                poll_interval_secs: interval,
                window_pos: None,
            },
            failures,
        );

        let used_rss = kind == SourceKind::Rss;
        let poll = if used_rss {
            rss_src.as_ref().expect("rss source").poll(watermark).await
        } else {
            wqq_src.as_ref().expect("wqq source").poll(watermark).await
        };

        match poll {
            Ok(res) => {
                failures = 0;
                let (new_wm, threads) = apply_result(watermark, &res);
                if let Ok(mut s) = state.lock() {
                    s.watermark = new_wm;
                    s.via_rss_fallback = s.cfg.source == SourceKind::Wqq && used_rss;
                }
                if !threads.is_empty() {
                    let _ = app.emit("new-threads", &threads);
                }
                let via = if used_rss { "rss" } else { "wqq" };
                let _ = app.emit("conn-state", serde_json::json!({ "ok": true, "via": via }));
            }
            Err(e) => {
                failures += 1;
                wqqlog::append_line(
                    &log_dir,
                    &format!("poll 失败(连败{failures}, 源={kind:?}): {e:#}"),
                );
                let via = if used_rss { "rss" } else { "wqq" };
                let _ = app.emit("conn-state", serde_json::json!({ "ok": false, "via": via }));
            }
        }

        tokio::time::sleep(next_delay(Duration::from_secs(interval), failures)).await;
    }
}

/// 方案一' 光标轮询（spike 裁定）：10ms 检查光标是否落在前端上报的可见矩形内，
/// 动态切换 set_ignore_cursor_events——矩形内正常交互，矩形外整窗穿透。
pub async fn run_cursor_watcher(app: AppHandle, state: Arc<Mutex<crate::state::SharedState>>) {
    let mut ignore = true; // main setup 已设初始穿透
    loop {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let Some(w) = app.get_webview_window("float") else { continue };
        let rects = state
            .lock()
            .map(|s| s.ui_rects.clone())
            .unwrap_or_default();
        if rects.is_empty() {
            // 前端尚未上报：保持穿透
            if !ignore {
                ignore = true;
                let _ = w.set_ignore_cursor_events(true);
            }
            continue;
        }
        let (Ok(cur), Ok(win)) = (app.cursor_position(), w.outer_position()) else {
            continue;
        };
        let dx = cur.x - win.x as f64;
        let dy = cur.y - win.y as f64;
        let inside = rects
            .iter()
            .any(|r| dx >= r[0] && dx < r[0] + r[2] && dy >= r[1] && dy < r[1] + r[3]);
        if inside == ignore {
            ignore = !inside;
            let _ = w.set_ignore_cursor_events(ignore);
        }
    }
}
