//! 轮询循环与命令共享的运行时状态。

use crate::config::Config;
use std::time::{Duration, Instant};

/// 拖动中冻结穿透判定的兜底上限：mouseup 事件丢失（拖到窗口外松手）时自动解冻
const DRAG_HOLD_MAX: Duration = Duration::from_secs(30);

pub struct SharedState {
    /// 已见最大帖子 tid（新帖判定水位）
    pub watermark: u64,
    pub cfg: Config,
    /// wqq 通道连败切到 RSS 兜底后置 true（供 conn-state 展示）
    pub via_rss_fallback: bool,
    /// 前端上报的可见 UI 矩形（物理像素，[x,y,w,h] 相对窗口客户区）——方案一' 光标判定用
    pub ui_rects: Vec<[f64; 4]>,
    /// 窗口位置已变（Moved 事件置位），poller 每轮落盘后清除
    pub cfg_pos_dirty: bool,
    /// 前端报告鼠标按下中（mousedown~mouseup）：Some 为按下时刻。
    /// 拖动期间 watcher 必须保持可交互，否则 set_ignore_cursor_events 会打断系统拖动循环
    pub drag_started: Option<Instant>,
    /// 窗口最近一次 Moved 时刻：拖动停止 400ms 后由 watcher 做一次右缘吸附判定
    pub last_moved: Option<Instant>,
    /// 最近一条新帖：前端加载完成后主动拉取（首轮 poll 常早于页面就绪，事件会丢）
    pub latest: Option<crate::source::Thread>,
}

impl SharedState {
    pub fn new(cfg: Config) -> Self {
        Self {
            watermark: 0,
            cfg,
            via_rss_fallback: false,
            ui_rects: Vec::new(),
            cfg_pos_dirty: false,
            drag_started: None,
            last_moved: None,
            latest: None,
        }
    }

    /// 是否处于拖动按下期（超时兜底防 mouseup 丢失导致穿透永久失效）
    pub fn dragging(&self) -> bool {
        self.drag_started
            .map(|t| t.elapsed() < DRAG_HOLD_MAX)
            .unwrap_or(false)
    }

    /// 换论坛/换源后必须走这里：旧水位指向旧站的 tid，若沿用会把新站永远判成"无新帖"
    /// （新站 tid 普遍小于旧站水位，Review Focus 第 3 条）。
    pub fn reset_for_new_forum(&mut self, cfg: Config) {
        self.cfg = cfg;
        self.watermark = 0;
        self.via_rss_fallback = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{default_config, SourceKind};

    #[test]
    fn reset_for_new_forum_clears_watermark_and_fallback() {
        let mut s = SharedState::new(default_config());
        s.watermark = 334535;
        s.via_rss_fallback = true;
        let mut newcfg = default_config();
        newcfg.forum_base = "https://other.example.com".into();
        newcfg.source = SourceKind::Rss;
        s.reset_for_new_forum(newcfg.clone());
        assert_eq!(s.watermark, 0);
        assert!(!s.via_rss_fallback);
        assert_eq!(s.cfg.forum_base, "https://other.example.com");
        assert_eq!(s.cfg.source, SourceKind::Rss);
    }

    #[test]
    fn dragging_holds_until_release_or_timeout() {
        let mut s = SharedState::new(default_config());
        assert!(!s.dragging());
        // 按下 → 冻结
        s.drag_started = Some(Instant::now());
        assert!(s.dragging());
        // mouseup 丢失超时 → 自动解冻（防穿透永久失效）
        s.drag_started = Some(Instant::now() - Duration::from_secs(31));
        assert!(!s.dragging());
        // 正常松开
        s.drag_started = Some(Instant::now());
        s.drag_started = None;
        assert!(!s.dragging());
    }
}
