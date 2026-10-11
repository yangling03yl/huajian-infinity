//! 用户数据目录布局 + 旧数据一次性迁移（SPEC-huazhong-v2 第 2、6 节）。
//!
//! ```text
//! <config_dir>/huajian/            Linux: ~/.config/huajian
//! ├── 花笺/settings.json            Windows: %APPDATA%\huajian
//! └── 花钟.hz                       macOS: ~/Library/Application Support/huajian
//! ```
//!
//! 根目录**与 app 标识符无关**，整个目录可以直接拷到别的设备继续用。
//!
//! 注意：本文件与花钟 app 的同名文件必须**逐字节相同**，因此这里不依赖
//! `crate::settings`，旧设置文件按 JSON 值原样搬运（只丢掉 `pomodoro`）。

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::huazhong;

/// 数据根目录名（位于系统配置目录下）
pub const APP_DIR_NAME: &str = "huajian";
/// 花笺自己的子目录
pub const NOTE_DIR_NAME: &str = "花笺";
/// 花笺设置文件名
pub const SETTINGS_FILE_NAME: &str = "settings.json";
/// 花钟数据文件名
pub const HUAZHONG_FILE_NAME: &str = "花钟.hz";
/// 旧版数据目录名（app 标识符）
pub const LEGACY_DIR_NAME: &str = "com.huajian.app";
/// 旧版番茄统计文件名
pub const LEGACY_STATS_FILE_NAME: &str = "pomodoro_stats.json";

/// 数据根目录：`<config_dir>/huajian`
pub fn data_root(config_dir: &Path) -> PathBuf {
    config_dir.join(APP_DIR_NAME)
}

/// 花笺子目录：`<root>/花笺`
pub fn note_dir(root: &Path) -> PathBuf {
    root.join(NOTE_DIR_NAME)
}

/// 花笺设置文件：`<root>/花笺/settings.json`
pub fn note_settings_path(root: &Path) -> PathBuf {
    note_dir(root).join(SETTINGS_FILE_NAME)
}

/// 花钟数据文件：`<root>/花钟.hz`
pub fn huazhong_path(root: &Path) -> PathBuf {
    root.join(HUAZHONG_FILE_NAME)
}

/// 花钟写入中的临时文件：`<root>/花钟.hz.tmp`
pub fn huazhong_tmp_path(root: &Path) -> PathBuf {
    root.join(format!("{HUAZHONG_FILE_NAME}.tmp"))
}

/// 花钟互斥锁文件：`<root>/花钟.hz.lock`
pub fn huazhong_lock_path(root: &Path) -> PathBuf {
    root.join(format!("{HUAZHONG_FILE_NAME}.lock"))
}

/// 旧版数据目录：`<config_dir>/com.huajian.app`（只读，仅用于一次性导入）
pub fn legacy_dir(config_dir: &Path) -> PathBuf {
    config_dir.join(LEGACY_DIR_NAME)
}

/// 启动时执行旧数据一次性导入，返回一次性提示文案（没有可导入的东西时为 `None`）。
///
/// 幂等：新文件已存在就整体跳过；出错时不改名任何旧文件，下次启动重试。
pub fn migrate_legacy(root: &Path, config_dir: &Path) -> Option<String> {
    match run_migration(root, config_dir) {
        Ok(true) => Some(format!("已把用户数据迁移到 {}", root.display())),
        Ok(false) => None,
        Err(err) => Some(format!("旧数据迁移失败：{err}（本次使用默认数据，下次启动会重试）")),
    }
}

/// 真正执行迁移；`Ok(true)` 表示发生了导入。
///
/// 幂等短路：`花笺/settings.json` 与 `花钟.hz` 都已存在（或旧文件本就不存在）时**整体跳过**，
/// 不解析任何旧文件、不碰磁盘、不产生提示。
///
/// 两阶段（SPEC-huazhong-v2 第 6.4 节）：
/// - **阶段一（只读）**：把需要的那部分旧文件读进内存并完成**全部**可能失败的解析校验
///   （旧设置 → `Value` + `pomodoro`，旧统计 → `days`，再交给 `huazhong::data_from_value`
///   得到待写数据）。任何失败都在这里返回 `Err`，**不写文件、不改名、连目录都不建**。
/// - **阶段二（提交）**：阶段一全部成功后才写新文件并归档对应旧文件；中途 I/O 失败允许
///   留下"新文件已存在但旧文件未改名"的部分状态，但绝不能因解析失败改名旧文件。
fn run_migration(root: &Path, config_dir: &Path) -> Result<bool, String> {
    let legacy = legacy_dir(config_dir);
    let legacy_settings_path = legacy.join(SETTINGS_FILE_NAME);
    let legacy_stats_path = legacy.join(LEGACY_STATS_FILE_NAME);

    // 先把旧文件读进内存：后面可能改名，改名后仍要能用到这些内容。
    let legacy_settings_raw = read_optional(&legacy_settings_path)?;
    let legacy_stats_raw = read_optional(&legacy_stats_path)?;

    let new_settings_path = note_settings_path(root);
    let new_huazhong_path = huazhong_path(root);

    // ---- 幂等短路：两步都不需要干活就整体跳过，一份旧文件都不解析 ----
    // （契约第 6 节"新文件已存在就整体跳过"的字面语义：不报错、也不产生任何提示。）
    let need_settings = !new_settings_path.exists() && legacy_settings_raw.is_some();
    let need_hz = !new_huazhong_path.exists()
        && (legacy_settings_raw.is_some() || legacy_stats_raw.is_some());
    if !need_settings && !need_hz {
        return Ok(false);
    }

    // ---- 阶段一：只读解析（只解析确实需要的那部分；任何失败都在这里返回，绝不触碰磁盘） ----

    // 旧设置里的番茄钟设置：导入 花钟.hz 时用；花笺设置里不再保留。
    let mut legacy_pomodoro: Option<Value> = None;
    // 待搬运的旧设置内容（已丢弃 `pomodoro`），阶段二原样写出。
    let mut legacy_settings_obj: Option<Map<String, Value>> = None;
    if let Some(raw) = &legacy_settings_raw {
        let value: Value = serde_json::from_slice(raw)
            .map_err(|e| format!("旧设置文件 {} 解析失败: {e}", legacy_settings_path.display()))?;
        let Value::Object(mut obj) = value else {
            return Err(format!("旧设置文件 {} 不是 JSON 对象", legacy_settings_path.display()));
        };
        legacy_pomodoro = obj.remove("pomodoro");
        legacy_settings_obj = Some(obj);
    }

    let mut pending_huazhong: Option<huazhong::HuazhongData> = None;
    if need_hz {
        let days = match &legacy_stats_raw {
            Some(raw) => {
                let value: Value = serde_json::from_slice(raw).map_err(|e| {
                    format!("旧统计数据 {} 解析失败: {e}", legacy_stats_path.display())
                })?;
                // 旧 days 原样交给 huazhong 的宽容解析：有明细的日期升级明细，
                // 只有合计的日期落进 baseline（旧格式兼容逻辑只此一处）。
                value
                    .get("days")
                    .and_then(|d| d.as_array())
                    .cloned()
                    .unwrap_or_default()
            }
            None => Vec::new(),
        };
        let settings_value = legacy_pomodoro
            .take()
            .unwrap_or_else(|| Value::Object(Map::new()));
        let payload = json!({
            "format": huazhong::HUAZHONG_FORMAT,
            "app": huazhong::HUAZHONG_APP,
            "settings": settings_value,
            "days": days,
        });
        pending_huazhong = Some(huazhong::data_from_value(payload));
    }

    // ---- 阶段二：提交（写新文件 → 归档对应旧文件） ----
    let mut migrated = false;

    // 1) 花笺设置（丢弃 pomodoro 字段）
    if let Some(obj) = legacy_settings_obj {
        if !new_settings_path.exists() {
            write_json_atomic(&new_settings_path, &Value::Object(obj))?;
            // 只有新文件确实写成功之后才改名旧文件。
            archive_rename(&legacy_settings_path)?;
            migrated = true;
        }
    }

    // 2) 花钟数据（幂等：新文件已存在则整步跳过，绝不覆盖别人的 花钟.hz）
    if !new_huazhong_path.exists() {
        if let Some(data) = pending_huazhong {
            huazhong::save(root, &data)?;
            if legacy_stats_raw.is_some() {
                archive_rename(&legacy_stats_path)?;
            }
            migrated = true;
        }
    }

    Ok(migrated)
}

/// 读文件；不存在返回 `None`，其他错误上抛。
fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("读取 {} 失败: {e}", path.display())),
    }
}

/// 原子写 JSON：先写 `<file>.tmp` 再 rename。
fn write_json_atomic(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录 {} 失败: {e}", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| format!("序列化设置失败: {e}"))?;
    let tmp = suffix_path(path, ".tmp");
    fs::write(&tmp, bytes).map_err(|e| format!("写入 {} 失败: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("保存 {} 失败: {e}", path.display()))?;
    Ok(())
}

/// 给文件名追加后缀（`settings.json` → `settings.json.tmp`）。
fn suffix_path(path: &Path, suffix: &str) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    path.with_file_name(format!("{name}{suffix}"))
}

/// 旧文件改名归档为 `*.migrated-YYYYMMDD`，重名再追加 `-2`、`-3`…
fn archive_rename(path: &Path) -> Result<PathBuf, String> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| format!("路径无效: {}", path.display()))?;
    let stamp = chrono::Local::now().format("%Y%m%d").to_string();
    for i in 1..=1000u32 {
        let candidate_name = if i == 1 {
            format!("{name}.migrated-{stamp}")
        } else {
            format!("{name}.migrated-{stamp}-{i}")
        };
        let candidate = path.with_file_name(&candidate_name);
        if candidate.exists() {
            continue;
        }
        fs::rename(path, &candidate)
            .map_err(|e| format!("归档旧文件 {} 失败: {e}", path.display()))?;
        return Ok(candidate);
    }
    Err(format!("归档旧文件 {} 失败: 同名归档过多", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::huazhong;

    fn test_dirs(name: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("huajian_paths_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let config_dir = base.join("config");
        fs::create_dir_all(&config_dir).unwrap();
        (data_root(&config_dir), config_dir)
    }

    #[test]
    fn layout_uses_huajian_root_with_chinese_names() {
        let config = Path::new("/home/u/.config");
        let root = data_root(config);
        assert_eq!(root, Path::new("/home/u/.config/huajian"));
        assert_eq!(note_settings_path(&root), root.join("花笺").join("settings.json"));
        assert_eq!(huazhong_path(&root), root.join("花钟.hz"));
        assert_eq!(huazhong_tmp_path(&root), root.join("花钟.hz.tmp"));
        assert_eq!(huazhong_lock_path(&root), root.join("花钟.hz.lock"));
        assert_eq!(legacy_dir(config), Path::new("/home/u/.config/com.huajian.app"));
    }

    #[test]
    fn migrate_without_legacy_data_does_nothing() {
        let (root, config_dir) = test_dirs("no_legacy");
        assert!(migrate_legacy(&root, &config_dir).is_none());
        assert!(!huazhong_path(&root).exists());
        assert!(!note_settings_path(&root).exists());
    }

    #[test]
    fn migrate_imports_settings_and_stats_once() {
        let (root, config_dir) = test_dirs("import");
        let legacy = legacy_dir(&config_dir);
        fs::create_dir_all(&legacy).unwrap();
        fs::write(
            legacy.join("settings.json"),
            br#"{"theme":"bamboo","dark":true,"sidebar_width":360,"full_width":true,
                 "boxes":["/x.hxl"],"pomodoro":{"mode":"forward","focus_minutes":50,"break_minutes":10,"loops":3}}"#,
        )
        .unwrap();
        fs::write(
            legacy.join("pomodoro_stats.json"),
            br#"{"days":[
                 {"date":"2026-02-09","standard_seconds":1500,"standard_count":1,
                  "sessions":[{"at":"09:15","seconds":1500,"mode":"standard"}]},
                 {"date":"2026-02-10","standard_seconds":1800,"standard_count":2,
                  "forward_seconds":600,"forward_count":1}
               ]}"#,
        )
        .unwrap();

        let notice = migrate_legacy(&root, &config_dir).expect("应返回迁移提示");
        assert!(notice.contains("已把用户数据迁移到"), "{notice}");

        // 花笺设置：搬到新位置，丢掉 pomodoro，其他字段原样
        let settings_raw = fs::read_to_string(note_settings_path(&root)).unwrap();
        assert!(!settings_raw.contains("pomodoro"), "{settings_raw}");
        let parsed: Value = serde_json::from_str(&settings_raw).unwrap();
        assert_eq!(parsed["theme"], "bamboo");
        assert_eq!(parsed["dark"], true);
        assert_eq!(parsed["sidebar_width"], 360);
        assert_eq!(parsed["full_width"], true);

        // 花钟数据：设置取自旧 pomodoro；旧汇总进 baseline；明细升级补齐
        let data = huazhong::load(&root);
        assert_eq!(data.settings.mode, huazhong::POMODORO_MODE_FORWARD);
        assert_eq!(data.settings.focus_minutes, 50);
        assert_eq!(data.days.len(), 2);

        let with_detail = data.days.iter().find(|d| d.date == "2026-02-09").unwrap();
        assert_eq!(with_detail.sessions.len(), 1);
        let s = &with_detail.sessions[0];
        assert!(!s.id.is_empty(), "应补发 id");
        assert_eq!(s.at, "09:15");
        assert_eq!(s.end, "09:15:00");
        assert_eq!(s.start, "08:50:00");
        assert_eq!(with_detail.standard_count, 1);
        assert!(with_detail.baseline.is_zero(), "有明细的日期不再用 baseline");

        let baseline_day = data.days.iter().find(|d| d.date == "2026-02-10").unwrap();
        assert!(baseline_day.sessions.is_empty());
        assert_eq!(baseline_day.baseline.standard_seconds, 1800);
        assert_eq!(baseline_day.baseline.standard_count, 2);
        assert_eq!(baseline_day.baseline.forward_seconds, 600);
        assert_eq!(baseline_day.standard_seconds, 1800);
        assert_eq!(baseline_day.forward_seconds, 600);
        assert_eq!(baseline_day.total_count(), 3);

        // 旧文件改名归档（不删除）
        let stamp = chrono::Local::now().format("%Y%m%d").to_string();
        assert!(!legacy.join("settings.json").exists());
        assert!(legacy
            .join(format!("settings.json.migrated-{stamp}"))
            .exists());
        assert!(legacy
            .join(format!("pomodoro_stats.json.migrated-{stamp}"))
            .exists());
    }

    #[test]
    fn migration_is_idempotent_and_appends_suffix() {
        let (root, config_dir) = test_dirs("idempotent");
        let legacy = legacy_dir(&config_dir);
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("settings.json"), br#"{"theme":"ink"}"#).unwrap();

        // 预先占掉第一顺位归档名，验证追加 -2
        let stamp = chrono::Local::now().format("%Y%m%d").to_string();
        fs::write(legacy.join(format!("settings.json.migrated-{stamp}")), b"old").unwrap();

        assert!(migrate_legacy(&root, &config_dir).is_some());
        assert!(legacy
            .join(format!("settings.json.migrated-{stamp}-2"))
            .exists());

        // 第二次：专用文件都在，整体跳过
        assert!(migrate_legacy(&root, &config_dir).is_none());
        // 花钟.hz 已存在，不被空数据覆盖
        let before = fs::read(huazhong_path(&root)).unwrap();
        assert!(migrate_legacy(&root, &config_dir).is_none());
        assert_eq!(fs::read(huazhong_path(&root)).unwrap(), before);
    }

    #[test]
    fn migration_error_keeps_legacy_files() {
        let (root, config_dir) = test_dirs("error");
        let legacy = legacy_dir(&config_dir);
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("settings.json"), br#"{"theme":"ink"}"#).unwrap();
        fs::write(legacy.join("pomodoro_stats.json"), b"{ not json").unwrap();

        let notice = migrate_legacy(&root, &config_dir).expect("应返回失败提示");
        assert!(notice.contains("迁移失败"), "{notice}");

        // 阶段一解析失败 → 零改动：两份旧文件都仍在原位（没有任何 .migrated-* 归档）
        assert!(legacy.join("settings.json").exists(), "旧 settings.json 不该被改名");
        assert!(legacy.join("pomodoro_stats.json").exists(), "旧 stats 不该被改名");
        let left: Vec<String> = fs::read_dir(&legacy)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(left.len(), 2, "旧目录不该多出归档文件: {left:?}");

        // 也不该落下任何新文件
        assert!(!note_settings_path(&root).exists(), "不该落盘新 settings.json");
        assert!(!huazhong_path(&root).exists(), "不该落盘 花钟.hz");
    }

    #[test]
    fn migration_error_on_broken_settings_leaves_everything_untouched() {
        let (root, config_dir) = test_dirs("error_settings");
        let legacy = legacy_dir(&config_dir);
        fs::create_dir_all(&legacy).unwrap();
        // 反向用例：settings 坏、stats 正常 → 同样是零改动
        fs::write(legacy.join("settings.json"), b"{ this is not json").unwrap();
        fs::write(
            legacy.join("pomodoro_stats.json"),
            br#"{"days":[{"date":"2026-02-09","standard_seconds":1500,"standard_count":1}]}"#,
        )
        .unwrap();

        let notice = migrate_legacy(&root, &config_dir).expect("应返回失败提示");
        assert!(notice.contains("迁移失败"), "{notice}");

        assert!(legacy.join("settings.json").exists(), "旧 settings.json 不该被改名");
        assert!(legacy.join("pomodoro_stats.json").exists(), "旧 stats 不该被改名");
        let left: Vec<String> = fs::read_dir(&legacy)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(left.len(), 2, "旧目录不该多出归档文件: {left:?}");

        assert!(!note_settings_path(&root).exists(), "不该落盘新 settings.json");
        assert!(!huazhong_path(&root).exists(), "不该落盘 花钟.hz");
    }

    #[test]
    fn migration_tolerates_broken_legacy_settings() {
        let (root, config_dir) = test_dirs("broken_settings");
        let legacy = legacy_dir(&config_dir);
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("settings.json"), b"[[[").unwrap();

        let notice = migrate_legacy(&root, &config_dir).expect("应返回失败提示");
        assert!(notice.contains("迁移失败"), "{notice}");
        assert!(legacy.join("settings.json").exists());
    }

    #[test]
    fn migration_fully_done_short_circuits_broken_legacy_settings() {
        let (root, config_dir) = test_dirs("done_broken_legacy");
        let legacy = legacy_dir(&config_dir);
        fs::create_dir_all(&legacy).unwrap();
        // 先完整迁移一次：两个新文件就位，两份旧文件被归档
        fs::write(legacy.join("settings.json"), br#"{"theme":"ink"}"#).unwrap();
        fs::write(legacy.join("pomodoro_stats.json"), br#"{"days":[]}"#).unwrap();
        assert!(migrate_legacy(&root, &config_dir).is_some());

        // 迁移完成后旧位置又被塞进一个坏 settings.json（残留文件）
        fs::write(legacy.join("settings.json"), b"{ this is not json").unwrap();
        let before_settings = fs::read(note_settings_path(&root)).unwrap();
        let before_hz = fs::read(huazhong_path(&root)).unwrap();

        // 两个新文件都在 → 整体跳过：不解析坏文件、不报错、零改动
        assert!(
            migrate_legacy(&root, &config_dir).is_none(),
            "已完全迁移后应短路返回 None，而不是报迁移失败"
        );

        assert_eq!(fs::read(note_settings_path(&root)).unwrap(), before_settings);
        assert_eq!(fs::read(huazhong_path(&root)).unwrap(), before_hz);
        assert!(legacy.join("settings.json").exists(), "坏旧文件应原封不动");

        // 旧目录仍只有：归档的 settings + 归档的 stats + 新塞进来的坏 settings
        let mut left: Vec<String> = fs::read_dir(&legacy)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        left.sort();
        assert_eq!(left.len(), 3, "旧目录不该多出文件: {left:?}");
        assert_eq!(
            left.iter().filter(|n| n.contains(".migrated-")).count(),
            2,
            "不该产生新的归档: {left:?}"
        );
    }
}