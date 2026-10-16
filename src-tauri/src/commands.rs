//! Tauri command：前端 invoke 的能力入口。

use crate::config::{self, clamp_interval, Config, SourceKind};
use crate::state::SharedState;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

/// 配置文件路径（setup 时 manage 注入）
pub struct CfgPath(pub PathBuf);

#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("仅允许打开 http(s) 链接".into());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn get_settings(state: tauri::State<'_, Arc<Mutex<SharedState>>>) -> Config {
    state.lock().expect("state lock poisoned").cfg.clone()
}

#[tauri::command]
pub fn save_window_pos(
    state: tauri::State<'_, Arc<Mutex<SharedState>>>,
    x: i32,
    y: i32,
) -> Result<(), String> {
    {
        let mut s = state.lock().map_err(|_| "state poisoned")?;
        s.cfg.window_pos = Some((x, y));
        s.cfg_pos_dirty = true; // poller 每轮落盘
    }
    Ok(())
}

#[tauri::command]
pub fn save_settings(
    state: tauri::State<'_, Arc<Mutex<SharedState>>>,
    cfgpath: tauri::State<'_, CfgPath>,
    forum_base: String,
    source: String,
    poll_interval_secs: u64,
) -> Result<(), String> {
    let mut cfg = state.lock().map_err(|_| "state poisoned")?.cfg.clone();
    cfg.forum_base = forum_base.trim().trim_end_matches('/').to_string();
    if !(cfg.forum_base.starts_with("http://") || cfg.forum_base.starts_with("https://")) {
        return Err("论坛地址必须以 http:// 或 https:// 开头".into());
    }
    cfg.source = match source.as_str() {
        "rss" => SourceKind::Rss,
        _ => SourceKind::Wqq,
    };
    cfg.poll_interval_secs = clamp_interval(poll_interval_secs);
    {
        let mut s = state.lock().map_err(|_| "state poisoned")?;
        // 换论坛/源必须重置水位（Review Focus：旧水位会把新站永远判成无新帖）
        s.reset_for_new_forum(cfg);
    }
    let cfg = state.lock().map_err(|_| "state poisoned")?.cfg.clone();
    config::save(&cfgpath.0, &cfg).map_err(|e| e.to_string())?;
    let _ = app_handle_emit_settings(&cfgpath);
    Ok(())
}

fn app_handle_emit_settings(_cfgpath: &tauri::State<'_, CfgPath>) -> Result<(), String> {
    // 预留：设置变更通知（当前 poller 每轮读共享状态，无需事件）
    Ok(())
}

/// 前端上报可见 UI 矩形（物理像素，相对窗口客户区），供光标轮询判定 ignore_cursor_events（方案一'）
#[tauri::command]
pub fn report_ui_rects(state: tauri::State<'_, Arc<Mutex<SharedState>>>, rects: Vec<[f64; 4]>) {
    if let Ok(mut s) = state.lock() {
        s.ui_rects = rects;
    }
}

/// 打开/显示设置窗（托盘菜单与球右键菜单共用）
pub fn open_settings_window(app: &AppHandle) -> Result<(), String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
    } else {
        WebviewWindowBuilder::new(
            app,
            "settings",
            WebviewUrl::App("settings.html".into()),
        )
        .title("wqq-float 设置")
        .inner_size(420.0, 410.0)
        .resizable(false)
        .build()
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn open_settings(app: AppHandle) -> Result<(), String> {
    open_settings_window(&app)
}
