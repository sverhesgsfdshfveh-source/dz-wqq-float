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
                theme: crate::config::ThemeKind::Dark,
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
                // 首轮（水位尚为 0）的帖子是历史存量，前端不播提示音
                let initial = watermark == 0;
                let (new_wm, threads) = apply_result(watermark, &res);
                if let Ok(mut s) = state.lock() {
                    s.watermark = new_wm;
                    s.via_rss_fallback = s.cfg.source == SourceKind::Wqq && used_rss;
                    // 保留最新帖供前端主动补拉（首轮 poll 常早于页面就绪，emit 会丢）
                    if let Some(t) = threads.first() {
                        s.latest = Some(t.clone());
                    }
                }
                if !threads.is_empty() {
                    let _ = app.emit(
                        "new-threads",
                        serde_json::json!({ "threads": threads, "initial": initial }),
                    );
                    // 运行期新帖才响；首轮 20 条存量帖不响
                    if !initial {
                        play_notify();
                    }
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
/// 拖动按下期（前端 set_dragging 报告）冻结判定：系统拖动循环中切穿透会中断拖动。
/// 移动停止 400ms 后做一次右缘吸附（贴近屏幕右缘时贴边）。
pub async fn run_cursor_watcher(app: AppHandle, state: Arc<Mutex<crate::state::SharedState>>) {
    let mut ignore = true; // main setup 已设初始穿透
    loop {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let Some(w) = app.get_webview_window("float") else { continue };

        let (should_settle, base_size) = {
            let Ok(mut s) = state.lock() else { continue };
            let due = match s.last_moved {
                Some(t) if t.elapsed() >= Duration::from_millis(400) => {
                    s.last_moved = None; // 每次停止移动只整定一次
                    true
                }
                _ => false,
            };
            (due, s.base_size)
        };
        if should_settle {
            settle_geometry(&w, base_size, &state);
        }

        let (rects, dragging) = state
            .lock()
            .map(|s| (s.ui_rects.clone(), s.dragging()))
            .unwrap_or_default();
        if dragging {
            // 拖动中：强制保持可交互（拖到球边缘时矩形判定会抖出界）
            if ignore {
                ignore = false;
                let _ = w.set_ignore_cursor_events(false);
            }
            continue;
        }
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

/// 新帖提示音：内嵌 mp3（提取自 ZCode task-notification-pop）用 rodio 解码播放。
/// 独立线程 + 不阻塞轮询；播放设备不可用/解码失败时静默。
/// 不走前端 WebView2：其 autoplay 策略需要 additionalBrowserArgs 放开，
/// 而该参数会破坏设置窗渲染（实测白屏），故移到 Rust 侧。
fn play_notify() {
    std::thread::spawn(|| {
        let Ok((_stream, handle)) = rodio::OutputStream::try_default() else {
            return;
        };
        let Ok(sink) = rodio::Sink::try_new(&handle) else {
            return;
        };
        let data: &[u8] = include_bytes!("../../ui/assets/notify.mp3");
        let Ok(src) = rodio::Decoder::new(std::io::Cursor::new(data)) else {
            return;
        };
        sink.append(src);
        sink.sleep_until_end();
    });
}

/// 移动停止后的窗口几何整定：
/// 1) 尺寸守卫——HTCAPTION 拖动会触发 Windows Aero Snap 改无边框窗口尺寸
///    （实测 760x700 被压成 760x443，卡片展开即被窗口底裁掉），恢复基准尺寸；
/// 2) 位置 clamp 回所在显示器（Snap 后可能半出屏）；
/// 3) 右缘吸附（距屏幕右缘 <40px 时贴 8px 边距）。
fn settle_geometry(
    w: &tauri::WebviewWindow,
    base_size: Option<(u32, u32)>,
    state: &Arc<Mutex<crate::state::SharedState>>,
) {
    // 1) 尺寸守卫
    if let (Ok(ws), Some((bw, bh))) = (w.outer_size(), base_size) {
        if ws.width != bw || ws.height != bh {
            let _ = w.set_size(tauri::PhysicalSize::new(bw, bh));
            // set_size 是异步派发：重新标记，400ms 后再整定位置
            if let Ok(mut s) = state.lock() {
                s.last_moved = Some(std::time::Instant::now());
            }
            return;
        }
    }
    let (Ok(pos), Ok(Some(mon))) = (w.outer_position(), w.current_monitor()) else {
        return;
    };
    let Ok(ws) = w.outer_size() else { return };
    let mp = mon.position();
    let ms = mon.size();
    // 2) 位置 clamp
    let min_x = mp.x;
    let max_x = mp.x + ms.width as i32 - ws.width as i32;
    let min_y = mp.y;
    let max_y = mp.y + ms.height as i32 - ws.height as i32;
    let nx = pos.x.clamp(min_x.min(max_x), min_x.max(max_x));
    let ny = pos.y.clamp(min_y.min(max_y), min_y.max(max_y));
    // 3) 右缘吸附
    let right_gap = (mp.x + ms.width as i32) - (nx + ws.width as i32);
    let target_x = if (0..40).contains(&right_gap) {
        mp.x + ms.width as i32 - ws.width as i32 - 8
    } else {
        nx
    };
    if target_x != pos.x || ny != pos.y {
        let _ = w.set_position(tauri::PhysicalPosition::new(target_x, ny));
    }
}
