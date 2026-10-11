use std::sync::Mutex;

use tauri::{Emitter, Manager};

mod box_store;
mod commands;
/// 花钟数据层：与花钟 app 的同名文件逐字节相同
pub mod huazhong;
mod models;
mod note_doc;
mod paths;
mod settings;

use box_store::BoxStore;
use commands::StartupNotice;
use settings::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let startup_hx = std::env::args()
        .skip(1)
        .find(|a| a.to_lowercase().ends_with(".hxl"));

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // 用户数据根目录与 app 标识符无关：<config_dir>/huajian
            let config_dir = app
                .path()
                .config_dir()
                .unwrap_or_else(|_| std::env::temp_dir());
            let root = paths::data_root(&config_dir);

            // 旧版数据一次性导入（幂等；出错不改名旧文件，下次启动重试）
            let notice = paths::migrate_legacy(&root, &config_dir);

            let state = AppState {
                settings: settings::load(&paths::note_settings_path(&root)),
            };
            app.manage(Mutex::new(state));
            app.manage(Mutex::new(huazhong::load(&root)));
            app.manage(Mutex::new(StartupNotice(notice)));
            Ok(())
        })
        .plugin(
            tauri_plugin_single_instance::init(|app, argv, _cwd| {
                if let Some(path) = argv.iter().find(|a| a.to_lowercase().ends_with(".hxl")) {
                    let _ = app.emit("open-hx", path.clone());
                }
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.set_focus();
                    let _ = w.unminimize();
                }
            }),
        )
        .manage(Mutex::new(BoxStore::new()))
        .manage(Mutex::new(startup_hx))
        .invoke_handler(tauri::generate_handler![
            commands::open_box,
            commands::create_box,
            commands::close_box,
            commands::list_notes,
            commands::read_note,
            commands::write_note,
            commands::create_note,
            commands::rename_note,
            commands::delete_note,
            commands::set_note_color,
            commands::reorder_notes,
            commands::save_box,
            commands::create_snapshot,
            commands::list_snapshots,
            commands::read_snapshot,
            commands::apply_snapshot,
            commands::delete_snapshot,
            commands::write_bytes,
            commands::get_settings,
            commands::set_appearance,
            commands::reorder_boxes,
            commands::get_startup_hx,
            commands::reveal_in_folder,
            commands::get_pomodoro_settings,
            commands::set_pomodoro_settings,
            commands::record_pomodoro,
            commands::get_pomodoro_stats,
            commands::delete_pomodoro_session,
            commands::shorten_pomodoro_session,
            commands::huazhong_path,
            commands::get_startup_notice,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}