use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Mutex;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use tauri::{Emitter, Manager, State};

use crate::box_store::BoxStore;
use crate::models::{BoxInfo, NoteMeta, SnapshotMeta};
use crate::paths;
use crate::settings::{self, AppSettings, AppState, WIDTH_MODE_FULL, WIDTH_MODE_LEFT75};
use crate::FlushAck;

/// 一次性启动提示（迁移成功/失败），`get_startup_notice` 读取后清空。
///
/// 用独立类型而不是 `Option<String>`：`get_startup_hx` 已经占了 `Mutex<Option<String>>`，
/// Tauri 的 `manage` 按类型索引，同类型会互相覆盖。
#[derive(Debug, Default)]
pub struct StartupNotice(pub Option<String>);

fn store<'a>(state: &'a State<'_, Mutex<BoxStore>>) -> Result<std::sync::MutexGuard<'a, BoxStore>, String> {
    state.lock().map_err(|_| "内部状态锁定失败".to_string())
}

/// 用户数据根目录：`<config_dir>/huajian`（与 app 标识符无关）
fn data_root(app: &tauri::AppHandle) -> PathBuf {
    let config_dir = app
        .path()
        .config_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    paths::data_root(&config_dir)
}

fn save_settings(app: &tauri::AppHandle, app_state: &State<'_, Mutex<AppState>>) {
    let result = match app_state.lock() {
        Ok(s) => settings::save(&paths::note_settings_path(&data_root(app)), &s.settings),
        Err(_) => Err("内部状态锁定失败".to_string()),
    };
    if let Err(e) = result {
        // 设置写不进去（磁盘满/无权限）必须让用户看见：主题、侧栏宽度，尤其是
        // 「已打开的花匣列表」否则会静默丢失，重启后一个花匣都不剩
        let _ = app.emit("settings-error", e);
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
    // 用后端返回的路径去重后入表：同一文件已打开时（后端按路径去重）记的是原路径，
    // 不会因为大小写/写法不同在 settings.boxes 里留下两条
    let stored = info.path.clone();
    if let Ok(mut s) = app_state.lock() {
        if !s.settings.boxes.iter().any(|p| p == &stored) {
            s.settings.boxes.push(stored);
        }
    }
    save_settings(&app, &app_state);
    Ok(info)
}

/// 新建花匣：名字由后端从文件名派生，前端不再传 name
#[tauri::command]
pub fn create_box(
    app: tauri::AppHandle,
    state: State<Mutex<BoxStore>>,
    app_state: State<Mutex<AppState>>,
    path: String,
) -> Result<BoxInfo, String> {
    let info = store(&state)?.create(Path::new(&path))?;
    if let Ok(mut s) = app_state.lock() {
        if !s.settings.boxes.iter().any(|p| p == &path) {
            s.settings.boxes.push(path);
        }
    }
    save_settings(&app, &app_state);
    Ok(info)
}

/// 重命名花匣：真改磁盘上的 .hxl 文件名。
/// settings.boxes 里旧路径必须**原位替换**（不能像 open 那样 push、也不能像 close 那样 retain，
/// 否则重启后的打开顺序会乱或丢）
#[tauri::command]
pub fn rename_box(
    app: tauri::AppHandle,
    state: State<Mutex<BoxStore>>,
    app_state: State<Mutex<AppState>>,
    box_id: String,
    new_name: String,
) -> Result<BoxInfo, String> {
    // 一次取锁内完成「读旧路径 + 改名」：分两次取锁的话，两个并发 rename 会各自
    // 读到已经失效的旧路径，导致 settings.boxes 残留失效条目、重启后花匣不再自动打开
    let (old_path, info) = {
        let mut s = store(&state)?;
        let old_path = s.get(&box_id)?.path.display().to_string();
        let info = s.rename_box(&box_id, &new_name)?;
        (old_path, info)
    };
    if let Ok(mut st) = app_state.lock() {
        settings::replace_box_path(&mut st.settings.boxes, &old_path, &info.path);
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

/// 按新顺序保存花匣内花笺列表（order 为花笺 id 的有序列表）
#[tauri::command]
pub fn reorder_notes(
    state: State<Mutex<BoxStore>>,
    box_id: String,
    order: Vec<String>,
) -> Result<(), String> {
    store(&state)?.reorder_notes(&box_id, order)
}

#[tauri::command]
pub fn save_box(state: State<Mutex<BoxStore>>, box_id: String) -> Result<(), String> {
    let mut s = store(&state)?;
    let b = s.get_mut(&box_id)?;
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

/// 保存界面外观（主题、明暗、侧栏宽度、正文宽度模式），不涉及花匣列表
#[tauri::command]
pub fn set_appearance(
    app: tauri::AppHandle,
    app_state: State<Mutex<AppState>>,
    theme: String,
    dark: bool,
    sidebar_width: u32,
    width_mode: String,
) -> Result<(), String> {
    if let Ok(mut s) = app_state.lock() {
        s.settings.theme = theme;
        s.settings.dark = dark;
        s.settings.sidebar_width = sidebar_width;
        // 只接受现存两档（left75/full），其他值（含已删除的 narrow）一律回落到默认
        s.settings.width_mode = Some(if width_mode == WIDTH_MODE_FULL {
            WIDTH_MODE_FULL.into()
        } else {
            WIDTH_MODE_LEFT75.into()
        });
        // 迁移完成后不再保留旧字段
        s.settings.full_width = None;
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

/// 启动时某个花匣打不开（被移动/删除/U 盘拔出）：把它从"下次自动打开"里移除，
/// 否则每次启动都会再弹一次错误
#[tauri::command]
pub fn drop_box_path(
    app: tauri::AppHandle,
    app_state: State<Mutex<AppState>>,
    path: String,
) -> Result<(), String> {
    if let Ok(mut st) = app_state.lock() {
        st.settings.boxes.retain(|p| p != &path);
    }
    save_settings(&app, &app_state);
    Ok(())
}

/// 前端已完成「关窗前最后一次保存」，允许主窗口销毁
#[tauri::command]
pub fn flush_ack(ack: State<FlushAck>) {
    ack.0.store(true, Ordering::SeqCst);
}

/// 一次性启动提示（迁移成功/失败），读取后清空
#[tauri::command]
pub fn get_startup_notice(notice: State<Mutex<StartupNotice>>) -> Option<String> {
    notice.lock().ok().and_then(|mut n| n.0.take())
}
