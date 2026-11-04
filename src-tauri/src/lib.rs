use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{Emitter, Manager, WindowEvent};

mod box_store;
mod commands;
mod models;
mod note_doc;
mod paths;
mod settings;

use box_store::BoxStore;
use commands::StartupNotice;
use settings::AppState;

/// 关窗握手：前端把最后一次输入落盘后置位。用 `Arc` 是为了能移进等待线程。
#[derive(Clone, Default)]
pub struct FlushAck(pub Arc<AtomicBool>);

/// 已经进入「关窗前的保存」流程：连点 × 时不重复触发，直接放行
#[derive(Clone, Default)]
pub struct ClosingGate(pub Arc<AtomicBool>);

/// 关闭窗口前等待前端落盘的最长时间
const CLOSE_FLUSH_TIMEOUT: Duration = Duration::from_millis(500);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let startup_hx = std::env::args()
        .skip(1)
        .find(|a| a.to_lowercase().ends_with(".hxl"));

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
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
        .manage(FlushAck::default())
        .manage(ClosingGate::default())
        .on_window_event(|window, event| {
            let WindowEvent::CloseRequested { api, .. } = event else {
                return;
            };
            // 已在关闭流程中（用户连点 ×）：不再拦截，直接让它关
            let gate = window.state::<ClosingGate>().inner().0.clone();
            if gate.swap(true, Ordering::SeqCst) {
                return;
            }
            // 自动保存是 1.2s 去抖，直接关窗会丢掉最后一次输入：
            // 先请前端 flush，最多等 CLOSE_FLUSH_TIMEOUT，超时也一定要能关掉
            api.prevent_close();
            let acked = window.state::<FlushAck>().inner().0.clone();
            let _ = window.emit("flush-request", ());
            let app = window.app_handle().clone();
            let win = window.clone();
            std::thread::spawn(move || {
                let deadline = Instant::now() + CLOSE_FLUSH_TIMEOUT;
                while Instant::now() < deadline {
                    if acked.load(Ordering::SeqCst) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                let _ = app.run_on_main_thread(move || {
                    let _ = win.destroy();
                });
            });
        })
        .invoke_handler(tauri::generate_handler![
            commands::open_box,
            commands::create_box,
            commands::rename_box,
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
            commands::get_startup_notice,
            commands::drop_box_path,
            commands::flush_ack,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
