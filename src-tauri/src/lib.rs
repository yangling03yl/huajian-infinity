use std::sync::Mutex;

use tauri::{Emitter, Manager};

mod box_store;
mod commands;
mod models;
mod note_doc;
mod pomodoro_stats;
mod settings;

use box_store::BoxStore;
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
            let config_dir = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("huajian-config"));
            let state = AppState {
                settings: settings::load(&config_dir),
            };
            app.manage(Mutex::new(state));
            app.manage(Mutex::new(pomodoro_stats::load(&config_dir)));
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
            commands::set_pomodoro_settings,
            commands::record_pomodoro,
            commands::get_pomodoro_stats,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
