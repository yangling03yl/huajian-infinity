//! 用户数据目录布局 + 旧数据一次性迁移。
//!
//! ```text
//! <config_dir>/huajian/            Linux: ~/.config/huajian
//! └── 花笺/settings.json            Windows: %APPDATA%\huajian
//!                                   macOS: ~/Library/Application Support/huajian
//! ```
//!
//! 根目录**与 app 标识符无关**，整个目录可以直接拷到别的设备继续用。
//!
//! 注意：本文件不依赖 `crate::settings`，旧设置文件按 JSON 值原样搬运。

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// 数据根目录名（位于系统配置目录下）
pub const APP_DIR_NAME: &str = "huajian";
/// 花笺自己的子目录
pub const NOTE_DIR_NAME: &str = "花笺";
/// 花笺设置文件名
pub const SETTINGS_FILE_NAME: &str = "settings.json";
/// 旧版数据目录名（app 标识符）
pub const LEGACY_DIR_NAME: &str = "com.huajian.app";

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
/// 幂等短路：`花笺/settings.json` 已存在（或旧文件本就不存在）时**整体跳过**，
/// 不解析任何旧文件、不碰磁盘、不产生提示。
///
/// 两阶段：
/// - **阶段一（只读）**：把旧设置读进内存并完成全部可能失败的解析校验。
///   任何失败都在这里返回 `Err`，**不写文件、不改名、连目录都不建**。
/// - **阶段二（提交）**：阶段一全部成功后才写新文件并归档旧文件；中途 I/O 失败允许
///   留下"新文件已存在但旧文件未改名"的部分状态，但绝不能因解析失败改名旧文件。
fn run_migration(root: &Path, config_dir: &Path) -> Result<bool, String> {
    let legacy_settings_path = legacy_dir(config_dir).join(SETTINGS_FILE_NAME);
    let new_settings_path = note_settings_path(root);

    // ---- 幂等短路：新设置已存在就没必要干活，一份旧文件都不解析 ----
    if new_settings_path.exists() {
        return Ok(false);
    }

    // ---- 阶段一：只读解析（任何失败都在这里返回，绝不触碰磁盘） ----
    let Some(raw) = read_optional(&legacy_settings_path)? else {
        return Ok(false);
    };
    let value: Value = serde_json::from_slice(&raw)
        .map_err(|e| format!("旧设置文件 {} 解析失败: {e}", legacy_settings_path.display()))?;
    let Value::Object(obj) = value else {
        return Err(format!("旧设置文件 {} 不是 JSON 对象", legacy_settings_path.display()));
    };

    // ---- 阶段二：提交（写新文件 → 归档旧文件） ----
    write_json_atomic(&new_settings_path, &Value::Object(obj))?;
    // 只有新文件确实写成功之后才改名旧文件。
    archive_rename(&legacy_settings_path)?;
    Ok(true)
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
        assert_eq!(legacy_dir(config), Path::new("/home/u/.config/com.huajian.app"));
    }

    #[test]
    fn migrate_without_legacy_data_does_nothing() {
        let (root, config_dir) = test_dirs("no_legacy");
        assert!(migrate_legacy(&root, &config_dir).is_none());
        assert!(!note_settings_path(&root).exists());
    }

    #[test]
    fn migrate_imports_settings_once() {
        let (root, config_dir) = test_dirs("import");
        let legacy = legacy_dir(&config_dir);
        fs::create_dir_all(&legacy).unwrap();
        fs::write(
            legacy.join("settings.json"),
            br#"{"theme":"bamboo","dark":true,"sidebar_width":360,"full_width":true,
                 "boxes":["/x.hxl"]}"#,
        )
        .unwrap();

        let notice = migrate_legacy(&root, &config_dir).expect("应返回迁移提示");
        assert!(notice.contains("已把用户数据迁移到"), "{notice}");

        // 花笺设置：搬到新位置，字段原样保留
        let settings_raw = fs::read_to_string(note_settings_path(&root)).unwrap();
        let parsed: Value = serde_json::from_str(&settings_raw).unwrap();
        assert_eq!(parsed["theme"], "bamboo");
        assert_eq!(parsed["dark"], true);
        assert_eq!(parsed["sidebar_width"], 360);
        assert_eq!(parsed["full_width"], true);

        // 旧文件改名归档（不删除）
        let stamp = chrono::Local::now().format("%Y%m%d").to_string();
        assert!(!legacy.join("settings.json").exists());
        assert!(legacy
            .join(format!("settings.json.migrated-{stamp}"))
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

        // 第二次：新文件已在，整体跳过
        assert!(migrate_legacy(&root, &config_dir).is_none());
        let before = fs::read(note_settings_path(&root)).unwrap();
        assert!(migrate_legacy(&root, &config_dir).is_none());
        assert_eq!(fs::read(note_settings_path(&root)).unwrap(), before);
    }

    #[test]
    fn migration_error_keeps_legacy_files() {
        let (root, config_dir) = test_dirs("error");
        let legacy = legacy_dir(&config_dir);
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("settings.json"), b"{ not json").unwrap();

        let notice = migrate_legacy(&root, &config_dir).expect("应返回失败提示");
        assert!(notice.contains("迁移失败"), "{notice}");

        // 解析失败 → 零改动：旧文件仍在原位（没有任何 .migrated-* 归档）
        assert!(legacy.join("settings.json").exists(), "旧 settings.json 不该被改名");
        let left: Vec<String> = fs::read_dir(&legacy)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(left.len(), 1, "旧目录不该多出归档文件: {left:?}");

        // 也不该落下任何新文件
        assert!(!note_settings_path(&root).exists(), "不该落盘新 settings.json");
    }

    #[test]
    fn migration_fully_done_short_circuits_broken_legacy_settings() {
        let (root, config_dir) = test_dirs("done_broken_legacy");
        let legacy = legacy_dir(&config_dir);
        fs::create_dir_all(&legacy).unwrap();
        // 先完整迁移一次：新设置就位，旧文件被归档
        fs::write(legacy.join("settings.json"), br#"{"theme":"ink"}"#).unwrap();
        assert!(migrate_legacy(&root, &config_dir).is_some());

        // 迁移完成后旧位置又被塞进一个坏 settings.json（残留文件）
        fs::write(legacy.join("settings.json"), b"{ this is not json").unwrap();
        let before_settings = fs::read(note_settings_path(&root)).unwrap();

        // 新设置已存在 → 整体跳过：不解析坏文件、不报错、零改动
        assert!(
            migrate_legacy(&root, &config_dir).is_none(),
            "已完全迁移后应短路返回 None，而不是报迁移失败"
        );

        assert_eq!(fs::read(note_settings_path(&root)).unwrap(), before_settings);
        assert!(legacy.join("settings.json").exists(), "坏旧文件应原封不动");

        // 旧目录仍只有：归档的 settings + 新塞进来的坏 settings
        let mut left: Vec<String> = fs::read_dir(&legacy)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        left.sort();
        assert_eq!(left.len(), 2, "旧目录不该多出文件: {left:?}");
        assert_eq!(
            left.iter().filter(|n| n.contains(".migrated-")).count(),
            1,
            "不该产生新的归档: {left:?}"
        );
    }
}
