use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use tauri::{Manager, State};

use crate::box_store::BoxStore;
use crate::huazhong::{self, HuazhongData, PomodoroDayStats, PomodoroSettings};
use crate::models::{BoxInfo, NoteMeta, SnapshotMeta};
use crate::paths;
use crate::settings::{self, AppSettings, AppState, WIDTH_MODE_FULL, WIDTH_MODE_LEFT75};

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
    if let Ok(s) = app_state.lock() {
        let _ = settings::save(&paths::note_settings_path(&data_root(app)), &s.settings);
    }
}

/// 把磁盘上的最新结果同步回进程内状态
fn sync_huazhong(state: &State<'_, Mutex<HuazhongData>>, data: HuazhongData) {
    if let Ok(mut guard) = state.lock() {
        *guard = data;
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

#[tauri::command]
pub fn reveal_in_folder(app: tauri::AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| format!("打开所在文件夹失败: {e}"))
}

// ---- 番茄钟 / 花钟数据（全部读写 花钟.hz，不再碰 settings.json 的 pomodoro） ----

/// 花钟数据文件绝对路径（供「打开数据位置」）
#[tauri::command]
pub fn huazhong_path(app: tauri::AppHandle) -> String {
    let root = data_root(&app);
    let file = paths::huazhong_path(&root);
    // 还没记录过任何番茄时数据文件不存在：这时退化为返回数据目录本身，
    // 保证「打开数据位置」总能把用户带到正确的地方，而不是静默失败。
    if file.exists() {
        return file.to_string_lossy().to_string();
    }
    let _ = std::fs::create_dir_all(&root);
    root.to_string_lossy().to_string()
}

/// 读取番茄钟设置（来自 花钟.hz；只读，直接 load 不需要锁）
#[tauri::command]
pub fn get_pomodoro_settings(
    app: tauri::AppHandle,
    hz_state: State<Mutex<HuazhongData>>,
) -> PomodoroSettings {
    let data = huazhong::load(&data_root(&app));
    let settings = data.settings.clone();
    sync_huazhong(&hz_state, data);
    settings
}

/// 保存番茄钟设置（模式、番茄时长、休息时长、循环次数），越界值自动夹取
#[tauri::command]
pub fn set_pomodoro_settings(
    app: tauri::AppHandle,
    hz_state: State<Mutex<HuazhongData>>,
    mode: String,
    focus_minutes: u32,
    break_minutes: u32,
    loops: u32,
) -> Result<(), String> {
    let data = huazhong::set_settings(
        &data_root(&app),
        PomodoroSettings {
            mode,
            focus_minutes,
            break_minutes,
            loops,
        },
    )?;
    sync_huazhong(&hz_state, data);
    Ok(())
}

/// 记录一个完成的番茄（秒）。`end` 由后端按本地时间取秒级时刻，跨午夜归入完成当日。
#[tauri::command]
pub fn record_pomodoro(
    app: tauri::AppHandle,
    hz_state: State<Mutex<HuazhongData>>,
    seconds: u64,
    mode: String,
) -> Result<(), String> {
    let data = huazhong::record(&data_root(&app), seconds, &mode)?;
    sync_huazhong(&hz_state, data);
    Ok(())
}

/// 读取某年某月（month 为 1-12）的番茄统计（只返回该月有记录的日期）
#[tauri::command]
pub fn get_pomodoro_stats(
    app: tauri::AppHandle,
    hz_state: State<Mutex<HuazhongData>>,
    year: i32,
    month: u32,
) -> Vec<PomodoroDayStats> {
    let data = huazhong::load(&data_root(&app));
    let stats = data.month(year, month);
    sync_huazhong(&hz_state, data);
    stats
}

/// 删除任意日期任意一条番茄记录
#[tauri::command]
pub fn delete_pomodoro_session(
    app: tauri::AppHandle,
    hz_state: State<Mutex<HuazhongData>>,
    date: String,
    id: String,
) -> Result<(), String> {
    let data = huazhong::delete_session(&data_root(&app), &date, &id)?;
    sync_huazhong(&hz_state, data);
    Ok(())
}

/// 缩短一条已完成番茄的时长（只能减短，不能加长）
#[tauri::command]
pub fn shorten_pomodoro_session(
    app: tauri::AppHandle,
    hz_state: State<Mutex<HuazhongData>>,
    date: String,
    id: String,
    seconds: u64,
) -> Result<(), String> {
    let data = huazhong::shorten_session(&data_root(&app), &date, &id, seconds)?;
    sync_huazhong(&hz_state, data);
    Ok(())
}

/// 一次性启动提示（迁移成功/失败），读取后清空
#[tauri::command]
pub fn get_startup_notice(notice: State<Mutex<StartupNotice>>) -> Option<String> {
    notice.lock().ok().and_then(|mut n| n.0.take())
}
