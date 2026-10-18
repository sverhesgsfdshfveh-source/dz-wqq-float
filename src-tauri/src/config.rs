//! 配置持久化：`%APPDATA%\wqq-float\config.json`（路径由调用方传入，测试用临时目录）。

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Wqq,
    Rss,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ThemeKind {
    /// 纯黑（默认，同对标图）
    Dark,
    /// 跟随系统浅色/深色
    System,
}

fn default_theme() -> ThemeKind {
    ThemeKind::Dark
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Config {
    /// 论坛根地址（不带尾斜杠），如 https://bbs.weiququ.cn
    pub forum_base: String,
    pub source: SourceKind,
    /// 轮询间隔秒数（3~300）
    pub poll_interval_secs: u64,
    /// 悬浮窗上次位置（屏幕坐标）
    pub window_pos: Option<(i32, i32)>,
    /// 主题：dark 纯黑 / system 跟随系统。
    /// serde default 保证旧配置（无此字段）仍能反序列化，不会因缺字段回落默认配置丢设置
    #[serde(default = "default_theme")]
    pub theme: ThemeKind,
    /// 是否已执行过"首次运行默认开启开机自启"。
    /// 仅首次默认开启：用户手动关掉后不再强制打开（serde default 兼容旧配置）
    #[serde(default)]
    pub autostart_default_applied: bool,
}

pub fn default_config() -> Config {
    Config {
        forum_base: "https://bbs.weiququ.cn".to_string(),
        source: SourceKind::Wqq,
        poll_interval_secs: 5,
        window_pos: None,
        theme: ThemeKind::Dark,
        autostart_default_applied: false,
    }
}

/// 轮询间隔钳制到 [3, 300] 秒。
pub fn clamp_interval(secs: u64) -> u64 {
    secs.clamp(3, 300)
}

/// 读取配置；文件缺失或损坏一律回落默认配置（不 panic，见 spec 错误处理矩阵）。
pub fn load(path: &Path) -> Config {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(default_config)
}

pub fn save(path: &Path, cfg: &Config) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(cfg)?;
    std::fs::write(path, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("wqq-float-test-{tag}-{}.json", std::process::id()))
    }

    #[test]
    fn defaults_match_spec() {
        let c = default_config();
        assert_eq!(c.forum_base, "https://bbs.weiququ.cn");
        assert_eq!(c.source, SourceKind::Wqq);
        assert_eq!(c.poll_interval_secs, 5);
        assert_eq!(c.window_pos, None);
        assert_eq!(c.theme, ThemeKind::Dark);
    }

    #[test]
    fn old_config_without_theme_still_loads() {
        // 旧版 config.json 无 theme 字段：必须能加载且不丢其它设置（serde default 兜底）
        let p = tmp_path("old-no-theme");
        std::fs::write(
            &p,
            r#"{"forum_base":"https://other.example.com","source":"rss","poll_interval_secs":30,"window_pos":[10,20]}"#,
        )
        .unwrap();
        let c = load(&p);
        assert_eq!(c.forum_base, "https://other.example.com");
        assert_eq!(c.source, SourceKind::Rss);
        assert_eq!(c.poll_interval_secs, 30);
        assert_eq!(c.window_pos, Some((10, 20)));
        assert_eq!(c.theme, ThemeKind::Dark);
        assert!(!c.autostart_default_applied);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn clamps_interval() {
        assert_eq!(clamp_interval(0), 3);
        assert_eq!(clamp_interval(2), 3);
        assert_eq!(clamp_interval(5), 5);
        assert_eq!(clamp_interval(299), 299);
        assert_eq!(clamp_interval(999), 300);
    }

    #[test]
    fn save_load_roundtrip() {
        let p = tmp_path("roundtrip");
        let mut c = default_config();
        c.poll_interval_secs = 42;
        c.window_pos = Some((123, 456));
        c.source = SourceKind::Rss;
        save(&p, &c).unwrap();
        let loaded = load(&p);
        assert_eq!(loaded.poll_interval_secs, 42);
        assert_eq!(loaded.window_pos, Some((123, 456)));
        assert_eq!(loaded.source, SourceKind::Rss);
        assert_eq!(loaded.forum_base, "https://bbs.weiququ.cn");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn load_missing_or_corrupt_falls_back_to_default() {
        let c = load(&tmp_path("missing"));
        assert_eq!(c, default_config());
        let p = tmp_path("corrupt");
        std::fs::write(&p, "{not json").unwrap();
        assert_eq!(load(&p), default_config());
        let _ = std::fs::remove_file(&p);
    }
}
