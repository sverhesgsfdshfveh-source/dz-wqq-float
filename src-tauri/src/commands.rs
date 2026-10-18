//! Tauri command：前端 invoke 的能力入口。

use crate::config::{self, clamp_interval, Config, SourceKind, ThemeKind};
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
    quit_now(&app);
}

/// 托盘菜单与前端命令共用的退出实现：先落盘配置（位置可能还在脏标记里），
/// 再异步强制退出——球右键菜单走 WebView2 IPC 回调，在回调里直接 ExitProcess 会挂起
/// （用户表现：球菜单"退出"无反应，只有托盘退出有效），故挪到独立线程延迟退出
pub fn quit_now(app: &AppHandle) {
    if let (Some(st), Some(cp)) = (
        app.try_state::<Arc<Mutex<SharedState>>>(),
        app.try_state::<CfgPath>(),
    ) {
        if let Ok(s) = st.lock() {
            let _ = config::save(&cp.0, &s.cfg);
        }
    }
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(80));
        std::process::exit(0);
    });
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
    app: AppHandle,
    state: tauri::State<'_, Arc<Mutex<SharedState>>>,
    cfgpath: tauri::State<'_, CfgPath>,
    forum_base: String,
    source: String,
    poll_interval_secs: u64,
    theme: String,
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
    cfg.theme = match theme.as_str() {
        "system" => ThemeKind::System,
        _ => ThemeKind::Dark,
    };
    {
        let mut s = state.lock().map_err(|_| "state poisoned")?;
        // 换论坛/源必须重置水位（Review Focus：旧水位会把新站永远判成无新帖）
        s.reset_for_new_forum(cfg);
    }
    let cfg = state.lock().map_err(|_| "state poisoned")?.cfg.clone();
    config::save(&cfgpath.0, &cfg).map_err(|e| e.to_string())?;
    // 通知悬浮窗刷新（主题等设置变化后前端需重新读取配置）
    let _ = app.emit("settings-changed", ());
    Ok(())
}

/// 前端上报可见 UI 矩形（物理像素，相对窗口客户区），供光标轮询判定 ignore_cursor_events（方案一'）
#[tauri::command]
pub fn report_ui_rects(state: tauri::State<'_, Arc<Mutex<SharedState>>>, rects: Vec<[f64; 4]>) {
    if let Ok(mut s) = state.lock() {
        s.ui_rects = rects;
    }
}

/// 前端加载完成后主动拉取最近新帖：首轮 poll 常早于页面就绪，emit 的事件会丢，
/// 之后水位已推进只发 changed:0，条会永远空白（启动竞态）
#[tauri::command]
pub fn get_latest_thread(
    state: tauri::State<'_, Arc<Mutex<SharedState>>>,
) -> Option<crate::source::Thread> {
    state.lock().ok().and_then(|s| s.latest.clone())
}

/// 正在拖动标志：拖动中 WndProc 忽略 WM_WINDOWPOSCHANGING 的尺寸变化（双保险）
#[tauri::command]
pub fn set_dragging(state: tauri::State<'_, Arc<Mutex<SharedState>>>, active: bool) {
    if let Ok(mut s) = state.lock() {
        s.drag_started = if active { Some(std::time::Instant::now()) } else { None };
    }
}

/// 自定义拖动：按屏幕坐标增量移动窗口。
/// 不用系统 HTCAPTION 拖动——那会触发 Windows Aero Snap（拖到屏幕边缘把窗口压成别的尺寸，
/// 且拖到最顶/最底会"弹回"）；自绘拖动可精确贴到屏幕任意边缘。
#[tauri::command]
pub fn move_window_by(app: AppHandle, dx: i32, dy: i32) -> Result<(), String> {
    let w = app
        .get_webview_window("float")
        .ok_or_else(|| "float window not found".to_string())?;
    let pos = w.outer_position().map_err(|e| e.to_string())?;
    let size = w.outer_size().map_err(|e| e.to_string())?;
    let mut nx = pos.x + dx;
    let mut ny = pos.y + dy;
    // clamp 到所在显示器：允许贴边（这是本方案的目的），但不允许整窗出屏
    if let Ok(Some(m)) = w.current_monitor() {
        let mp = m.position();
        let ms = m.size();
        let max_x = (mp.x + ms.width as i32 - size.width as i32).max(mp.x);
        let max_y = (mp.y + ms.height as i32 - size.height as i32).max(mp.y);
        nx = nx.clamp(mp.x, max_x);
        ny = ny.clamp(mp.y, max_y);
    }
    w.set_position(tauri::PhysicalPosition::new(nx, ny))
        .map_err(|e| e.to_string())
}

/// 悬浮窗自适应尺寸（物理像素）：窗口贴合"球+条"组合，卡片展开时前端加高。
/// 若扩展后超出所在显示器右下缘，向左/上回缩保证组合可见。
#[tauri::command]
pub fn resize_float(app: AppHandle, width: u32, height: u32) -> Result<(), String> {
    let w = app
        .get_webview_window("float")
        .ok_or_else(|| "float window not found".to_string())?;
    let (width, height) = (width.clamp(120, 1400), height.clamp(48, 900));
    w.set_size(tauri::PhysicalSize::new(width, height))
        .map_err(|e| e.to_string())?;
    if let (Ok(pos), Ok(Some(m))) = (w.outer_position(), w.current_monitor()) {
        let mp = m.position();
        let ms = m.size();
        let mut nx = pos.x;
        let mut ny = pos.y;
        if nx + width as i32 > mp.x + ms.width as i32 {
            nx = mp.x + ms.width as i32 - width as i32;
        }
        if ny + height as i32 > mp.y + ms.height as i32 {
            ny = mp.y + ms.height as i32 - height as i32;
        }
        nx = nx.max(mp.x);
        ny = ny.max(mp.y);
        if nx != pos.x || ny != pos.y {
            let _ = w.set_position(tauri::PhysicalPosition::new(nx, ny));
        }
    }
    Ok(())
}

/// 打开/显示设置窗（托盘菜单与球右键菜单共用）
pub fn open_settings_window(app: &AppHandle) -> Result<(), String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
        // WebView2 隐藏后重新显示常不重绘（整窗白屏）→ 强制重载页面
        let _ = w.eval("location.reload()");
    } else {
        WebviewWindowBuilder::new(
            app,
            "settings",
            WebviewUrl::App("settings.html".into()),
        )
        .title("wqq-float 设置")
        // 初值兜底（同 main.rs 预创建）；页面加载后按时测内容高度自适应
        .inner_size(420.0, 620.0)
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

/// 设置窗保存成功后自动收起。hide 而非 close/destroy：
/// 设置窗销毁后运行中重建会白屏（WebView2 动态创建异常），保活是既有约定
/// （main.rs 里 X 按钮的 CloseRequested 同样只 hide）
#[tauri::command]
pub fn hide_settings(app: AppHandle) -> Result<(), String> {
    let w = app
        .get_webview_window("settings")
        .ok_or_else(|| "settings window not found".to_string())?;
    w.hide().map_err(|e| e.to_string())
}

/// 设置窗高度自适应：settings.js 在页面加载后按内容实测高度（逻辑像素）上报。
/// 固定高度靠估算（headless 测量与实际 WebView2 渲染有差）会漏出滚动条，
/// 改为内容驱动后，以后改 settings.html/css 也不用再动窗口尺寸常量。
#[tauri::command]
pub fn resize_settings(app: AppHandle, height: f64) -> Result<(), String> {
    let w = app
        .get_webview_window("settings")
        .ok_or_else(|| "settings window not found".to_string())?;
    let h = height.clamp(320.0, 1600.0);
    w.set_size(tauri::LogicalSize::new(420.0, h))
        .map_err(|e| e.to_string())
}

// ---- 窗口形状裁剪（Windows region 镂空） ----
// colorkey 路线因 LWA_COLORKEY 的像素级 hit-test 与 WebView2 的 mousemove 冲突
// （hover/拖动全失效）废弃，教训记于 main.rs setup 与 release-checklist。
// region 是 1bit 几何裁剪、边缘有锯齿——由前端把每个形状外扩 1 物理像素抵消
// （app.js collectShapes：锯齿像素落在 CSS 形状内被覆盖）。
#[cfg(windows)]
#[tauri::command]
pub fn set_window_region(app: AppHandle, shapes: Vec<Vec<f64>>) -> Result<(), String> {
    #[link(name = "gdi32")]
    extern "system" {
        fn CreateRectRgn(x1: i32, y1: i32, x2: i32, y2: i32) -> isize;
        fn CreateEllipticRgn(x1: i32, y1: i32, x2: i32, y2: i32) -> isize;
        fn CreateRoundRectRgn(x1: i32, y1: i32, x2: i32, y2: i32, w: i32, h: i32) -> isize;
        fn CombineRgn(dst: isize, src1: isize, src2: isize, mode: i32) -> i32;
        fn DeleteObject(obj: isize) -> i32;
    }
    #[link(name = "user32")]
    extern "system" {
        fn SetWindowRgn(hwnd: isize, rgn: isize, redraw: i32) -> i32;
    }

    let w = app
        .get_webview_window("float")
        .ok_or_else(|| "float window not found".to_string())?;
    let hwnd = w.hwnd().map_err(|e| e.to_string())?;
    unsafe {
        let dest = CreateRectRgn(0, 0, 0, 0);
        if dest == 0 {
            return Err("CreateRectRgn failed".into());
        }
        for s in &shapes {
            if s.len() < 5 {
                continue;
            }
            let kind = s[0] as i32;
            let x = s[1].round() as i32;
            let y = s[2].round() as i32;
            let ww = s[3].round() as i32;
            let hh = s[4].round() as i32;
            if ww <= 0 || hh <= 0 {
                continue;
            }
            let rgn = match kind {
                0 => CreateEllipticRgn(x, y, x + ww, y + hh),
                2 => CreateRectRgn(x, y, x + ww, y + hh),
                _ => {
                    let r = s.get(5).copied().unwrap_or(0.0).max(0.0).round() as i32;
                    CreateRoundRectRgn(x, y, x + ww, y + hh, r * 2, r * 2)
                }
            };
            if rgn != 0 {
                CombineRgn(dest, dest, rgn, 2); // RGN_OR
                DeleteObject(rgn);
            }
        }
        // 成功后 region 归系统所有，不能再 DeleteObject
        if SetWindowRgn(hwnd.0 as isize, dest, 1) == 0 {
            DeleteObject(dest);
            return Err("SetWindowRgn failed".into());
        }
    }
    Ok(())
}

#[cfg(not(windows))]
#[tauri::command]
pub fn set_window_region(app: AppHandle, shapes: Vec<Vec<f64>>) -> Result<(), String> {
    let _ = (app, shapes);
    Ok(())
}
