#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;

mod source;
mod config;
mod state;
mod poller;
mod log;
mod commands;

fn main() {
    tauri::Builder::default()
        // single-instance 必须最先注册（官方要求），重复启动时聚焦已有悬浮窗
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("float") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("wqq-float"));
            let config_path = data_dir.join("config.json");
            let cfg = config::load(&config_path);
            let st = std::sync::Arc::new(std::sync::Mutex::new(state::SharedState::new(cfg)));
            app.manage(st.clone());
            app.manage(commands::CfgPath(config_path.clone()));

            // 悬浮窗位置：存档有效则恢复，否则主屏右缘垂直居中；初始设为鼠标穿透（方案一'），
            // 等前端上报可见矩形后由光标轮询接管
            if let Some(w) = app.get_webview_window("float") {
                let mons = w.available_monitors().unwrap_or_default();
                let restored = {
                    let s = st.lock().expect("state lock");
                    s.cfg.window_pos.filter(|p| {
                        mons.iter().any(|m| {
                            let mp = m.position();
                            let ms = m.size();
                            mp.x <= p.0
                                && p.0 < mp.x + ms.width as i32
                                && mp.y <= p.1
                                && p.1 < mp.y + ms.height as i32
                        })
                    })
                };
                match restored {
                    Some((x, y)) => {
                        let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
                    }
                    None => {
                        if let Ok(Some(prim)) = w.primary_monitor() {
                            let mp = prim.position();
                            let ms = prim.size();
                            let ws = w.outer_size().unwrap_or(tauri::PhysicalSize::new(700, 520));
                            let x = mp.x + ms.width as i32 - ws.width as i32 - 8;
                            let y = mp.y + (ms.height as i32 - ws.height as i32) / 2;
                            let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
                        }
                    }
                }
                let _ = w.set_ignore_cursor_events(true);
            }

            // 系统托盘：打开论坛 / 设置 / 开机自启 / 退出
            {
                use tauri::menu::{CheckMenuItem, Menu, MenuItem};
                use tauri::tray::TrayIconBuilder;
                use tauri_plugin_autostart::ManagerExt as _;
                use tauri_plugin_opener::OpenerExt as _;

                let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);
                let open_i = MenuItem::with_id(app, "open", "打开论坛", true, None::<&str>)?;
                let settings_i = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
                let autostart_i = CheckMenuItem::with_id(
                    app, "autostart", "开机自启", true, autostart_on, None::<&str>,
                )?;
                let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&open_i, &settings_i, &autostart_i, &quit_i])?;

                let autostart_item = autostart_i.clone();
                TrayIconBuilder::with_id("main")
                    .icon(app.default_window_icon().expect("no window icon").clone())
                    .tooltip("微趣趣线报网 · 桌面通知")
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(move |app, event| {
                        match event.id.as_ref() {
                            "open" => {
                                let base = app
                                    .try_state::<std::sync::Arc<std::sync::Mutex<state::SharedState>>>()
                                    .map(|s| s.lock().expect("state lock").cfg.forum_base.clone())
                                    .unwrap_or_else(|| "https://bbs.weiququ.cn".into());
                                let _ = app.opener().open_url(base, None::<&str>);
                            }
                            "settings" => {
                                let _ = commands::open_settings_window(app);
                            }
                            "autostart" => {
                                let now_on = if app.autolaunch().is_enabled().unwrap_or(false) {
                                    let _ = app.autolaunch().disable();
                                    false
                                } else {
                                    let _ = app.autolaunch().enable();
                                    true
                                };
                                let _ = autostart_item.set_checked(now_on);
                            }
                            "quit" => app.exit(0),
                            _ => {}
                        }
                    })
                    .build(app)?;
            }

            // 两个后台任务：数据轮询 + 光标穿透判定（方案一'）
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(poller::run_poller(
                handle,
                st.clone(),
                data_dir,
                config_path,
            ));
            tauri::async_runtime::spawn(poller::run_cursor_watcher(
                app.handle().clone(),
                st,
            ));
            Ok(())
        })
        .on_window_event(|window, event| {
            // 拖动位置记忆：Moved 更新内存并标脏，poller 每轮落盘（drag region 拖动时前端收不到 mouseup）
            if window.label() == "float" {
                if let tauri::WindowEvent::Moved(pos) = event {
                    let app = window.app_handle();
                    if let Some(st) = app.try_state::<std::sync::Arc<std::sync::Mutex<state::SharedState>>>() {
                        if let Ok(mut s) = st.lock() {
                            s.cfg.window_pos = Some((pos.x, pos.y));
                            s.cfg_pos_dirty = true;
                        }
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::open_url,
            commands::quit_app,
            commands::get_settings,
            commands::save_window_pos,
            commands::save_settings,
            commands::report_ui_rects,
            commands::open_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
