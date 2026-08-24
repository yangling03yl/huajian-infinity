use std::path::Path;
use std::sync::Mutex;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use tauri::{Manager, State};

use crate::box_store::BoxStore;
use crate::models::{BoxInfo, NoteMeta, SnapshotMeta};
use crate::settings::{self, AppSettings, AppState};

fn store<'a>(state: &'a State<'_, Mutex<BoxStore>>) -> Result<std::sync::MutexGuard<'a, BoxStore>, String> {
    state.lock().map_err(|_| "内部状态锁定失败".to_string())
}

fn save_settings(app: &tauri::AppHandle, app_state: &State<'_, Mutex<AppState>>) {
    if let Ok(s) = app_state.lock() {
        if let Ok(dir) = app.path().app_config_dir() {
            let _ = settings::save(&dir, &s.settings);
        }
    }
}

#[tauri::command]
pub fn open_box(
    app: tauri::AppHandle,
    state: State<Mutex<BoxStore>>,
    app_state: State<Mutex<AppState>>,
    path: String,
) -> Result<BoxInfo, String> {
    let info = store(&state)?.open(Path::new(&path))?;
    if let Ok(mut s) = app_state.lock() {
        if !s.settings.boxes.iter().any(|p| p == &path) {
            s.settings.boxes.push(path);
        }
    }
    save_settings(&app, &app_state);
    Ok(info)
}

#[tauri::command]
pub fn create_box(
    app: tauri::AppHandle,
    state: State<Mutex<BoxStore>>,
    app_state: State<Mutex<AppState>>,
    path: String,
    name: String,
) -> Result<BoxInfo, String> {
    let info = store(&state)?.create(Path::new(&path), name)?;
    if let Ok(mut s) = app_state.lock() {
        if !s.settings.boxes.iter().any(|p| p == &path) {
            s.settings.boxes.push(path);
        }
    }
    save_settings(&app, &app_state);
    Ok(info)
}

#[tauri::command]
pub fn close_box(
    app: tauri::AppHandle,
    state: State<Mutex<BoxStore>>,
    app_state: State<Mutex<AppState>>,
    box_id: String,
) -> Result<(), String> {
    let path;
    {
        let s = store(&state)?;
        let ob = s.get(&box_id)?;
        path = ob.path.display().to_string();
    }
    store(&state)?.close(&box_id)?;
    if let Ok(mut st) = app_state.lock() {
        st.settings.boxes.retain(|p| p != &path);
    }
    save_settings(&app, &app_state);
    Ok(())
}

#[tauri::command]
pub fn list_notes(
    state: State<Mutex<BoxStore>>,
    box_id: String,
) -> Result<Vec<NoteMeta>, String> {
    let s = store(&state)?;
    let b = s.get(&box_id)?;
    Ok(b.index.notes.clone())
}

#[tauri::command]
pub fn read_note(
    state: State<Mutex<BoxStore>>,
    box_id: String,
    note_id: String,
) -> Result<String, String> {
    store(&state)?.read_note(&box_id, &note_id)
}

#[tauri::command]
pub fn write_note(
    state: State<Mutex<BoxStore>>,
    box_id: String,
    note_id: String,
    content: String,
) -> Result<(), String> {
    store(&state)?.write_note(&box_id, &note_id, &content)
}

#[tauri::command]
pub fn create_note(
    state: State<Mutex<BoxStore>>,
    box_id: String,
    title: String,
    color: String,
) -> Result<NoteMeta, String> {
    store(&state)?.create_note(&box_id, title, color)
}

#[tauri::command]
pub fn rename_note(
    state: State<Mutex<BoxStore>>,
    box_id: String,
    note_id: String,
    title: String,
) -> Result<(), String> {
    store(&state)?.rename_note(&box_id, &note_id, title)
}

#[tauri::command]
pub fn delete_note(
    state: State<Mutex<BoxStore>>,
    box_id: String,
    note_id: String,
) -> Result<(), String> {
    store(&state)?.delete_note(&box_id, &note_id)
}

#[tauri::command]
pub fn set_note_color(
    state: State<Mutex<BoxStore>>,
    box_id: String,
    note_id: String,
    color: String,
) -> Result<(), String> {
    store(&state)?.set_note_color(&box_id, &note_id, color)
}

#[tauri::command]
pub fn save_box(state: State<Mutex<BoxStore>>, box_id: String) -> Result<(), String> {
    let s = store(&state)?;
    let b = s.get(&box_id)?;
    BoxStore::repack(b)
}

/// 复制当前正文为快照
#[tauri::command]
pub fn create_snapshot(
    state: State<Mutex<BoxStore>>,
    box_id: String,
    note_id: String,
    label: Option<String>,
) -> Result<SnapshotMeta, String> {
    store(&state)?.create_snapshot(&box_id, &note_id, label)
}

/// 列出全部快照版本
#[tauri::command]
pub fn list_snapshots(
    state: State<Mutex<BoxStore>>,
    box_id: String,
    note_id: String,
) -> Result<Vec<SnapshotMeta>, String> {
    store(&state)?.list_snapshots(&box_id, &note_id)
}

/// 读取某个快照的内容
#[tauri::command]
pub fn read_snapshot(
    state: State<Mutex<BoxStore>>,
    box_id: String,
    note_id: String,
    snapshot_id: String,
) -> Result<String, String> {
    store(&state)?.read_snapshot(&box_id, &note_id, &snapshot_id)
}

/// 用快照内容替换正文（恢复版本），返回新正文
#[tauri::command]
pub fn apply_snapshot(
    state: State<Mutex<BoxStore>>,
    box_id: String,
    note_id: String,
    snapshot_id: String,
) -> Result<String, String> {
    store(&state)?.apply_snapshot(&box_id, &note_id, &snapshot_id)
}

/// 删除某个快照版本
#[tauri::command]
pub fn delete_snapshot(
    state: State<Mutex<BoxStore>>,
    box_id: String,
    note_id: String,
    snapshot_id: String,
) -> Result<(), String> {
    store(&state)?.delete_snapshot(&box_id, &note_id, &snapshot_id)
}

/// 导出：将 base64 编码的字节写入指定路径（md / PDF）
#[tauri::command]
pub fn write_bytes(path: String, data_base64: String) -> Result<(), String> {
    let bytes = B64
        .decode(data_base64.as_bytes())
        .map_err(|e| format!("解码数据失败: {e}"))?;
    std::fs::write(&path, bytes).map_err(|e| format!("写入文件失败: {e}"))
}

/// 读取用户设置（主题、明暗、侧栏宽度、上次会话花匣列表）
#[tauri::command]
pub fn get_settings(app_state: State<Mutex<AppState>>) -> AppSettings {
    app_state
        .lock()
        .map(|s| s.settings.clone())
        .unwrap_or_default()
}

/// 保存界面外观（主题、明暗、侧栏宽度），不涉及花匣列表
#[tauri::command]
pub fn set_appearance(
    app: tauri::AppHandle,
    app_state: State<Mutex<AppState>>,
    theme: String,
    dark: bool,
    sidebar_width: u32,
) -> Result<(), String> {
    if let Ok(mut s) = app_state.lock() {
        s.settings.theme = theme;
        s.settings.dark = dark;
        s.settings.sidebar_width = sidebar_width;
    }
    save_settings(&app, &app_state);
    Ok(())
}

/// 按新顺序保存花匣列表（order 为花匣 id 的有序列表）
#[tauri::command]
pub fn reorder_boxes(
    app: tauri::AppHandle,
    state: State<Mutex<BoxStore>>,
    app_state: State<Mutex<AppState>>,
    order: Vec<String>,
) -> Result<(), String> {
    let mut paths: Vec<String> = Vec::new();
    {
        let s = store(&state)?;
        for id in &order {
            if let Some(ob) = s.boxes.get(id) {
                paths.push(ob.path.display().to_string());
            }
        }
    }
    if let Ok(mut st) = app_state.lock() {
        let mut merged = paths;
        for p in &st.settings.boxes {
            if !merged.contains(p) {
                merged.push(p.clone());
            }
        }
        st.settings.boxes = merged;
    }
    save_settings(&app, &app_state);
    Ok(())
}

/// 启动时由文件关联传入的 .hxl 路径（Linux 命令行参数）
#[tauri::command]
pub fn get_startup_hx(startup: State<Mutex<Option<String>>>) -> Option<String> {
    startup.lock().ok().and_then(|s| s.clone())
}

#[tauri::command]
pub fn reveal_in_folder(app: tauri::AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| format!("打开所在文件夹失败: {e}"))
}
