#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;

mod source;
mod config;
mod state;
mod poller;
mod log;

fn main() {
    tauri::Builder::default()
        // single-instance 必须最先注册（官方要求），重复启动时聚焦已有悬浮窗
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("float") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("wqq-float"));
            let cfg = config::load(&data_dir.join("config.json"));
            let st = std::sync::Arc::new(std::sync::Mutex::new(state::SharedState::new(cfg)));
            app.manage(st.clone());
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(poller::run_poller(handle, st, data_dir));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
