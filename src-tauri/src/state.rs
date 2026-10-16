//! 轮询循环与命令共享的运行时状态。

use crate::config::Config;

pub struct SharedState {
    /// 已见最大帖子 tid（新帖判定水位）
    pub watermark: u64,
    pub cfg: Config,
    /// wqq 通道连败切到 RSS 兜底后置 true（供 conn-state 展示）
    pub via_rss_fallback: bool,
}

impl SharedState {
    pub fn new(cfg: Config) -> Self {
        Self {
            watermark: 0,
            cfg,
            via_rss_fallback: false,
        }
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
}
