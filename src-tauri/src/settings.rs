use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 花笺自己的用户设置。
///
/// 存放位置由调用方传入（见 `crate::paths::note_settings_path`）；
/// 正常是 `<config_dir>/huajian/花笺/settings.json`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub dark: bool,
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: u32,
    /// 正文宽度模式：left75（靠左 75%，默认）/ full（铺满）
    /// 用 Option 以区分「老配置里没写过」与「写过具体值」，供迁移判断
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width_mode: Option<String>,
    /// 旧版字段：正文是否铺满。仅用于读取老配置并迁移到 width_mode，不再写出
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_width: Option<bool>,
    /// 已打开花匣的有序路径（启动时按此顺序自动打开）
    #[serde(default)]
    pub boxes: Vec<String>,
}

/// 正文宽度模式：靠左 75%（默认）
pub const WIDTH_MODE_LEFT75: &str = "left75";
/// 正文宽度模式：铺满窗口
pub const WIDTH_MODE_FULL: &str = "full";

fn default_theme() -> String {
    "warm-paper".into()
}

fn default_sidebar_width() -> u32 {
    300
}

fn default_width_mode() -> String {
    WIDTH_MODE_LEFT75.into()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            dark: false,
            sidebar_width: default_sidebar_width(),
            width_mode: Some(default_width_mode()),
            full_width: None,
            boxes: Vec::new(),
        }
    }
}

/// 内存中的用户状态
pub struct AppState {
    pub settings: AppSettings,
}

impl AppSettings {
    /// 归一化：未知宽度模式（含已删除的 narrow）回落到默认；老配置的 full_width 迁移到 width_mode
    ///
    /// 迁移规则：老配置里显式写了 full_width 的（true→铺满）尊重原选择；
    /// false（旧的「专注阅读」档）与完全没写过的都回落到「靠左 75%」，
    /// 因为专注阅读模式已删除，靠左 75% 是新的默认布局。
    pub fn sanitized(mut self) -> Self {
        let valid = matches!(
            self.width_mode.as_deref(),
            Some(WIDTH_MODE_LEFT75) | Some(WIDTH_MODE_FULL)
        );
        if !valid {
            self.width_mode = Some(match self.full_width {
                Some(true) => WIDTH_MODE_FULL.into(),
                _ => default_width_mode(),
            });
        }
        // 迁移完成后不再保留旧字段
        self.full_width = None;
        self
    }

    /// 取当前宽度模式（字段缺失时按默认）
    #[allow(dead_code)] // 仅测试与调试使用；生产代码直接读字段
    pub fn width_mode(&self) -> &str {
        self.width_mode.as_deref().unwrap_or(WIDTH_MODE_LEFT75)
    }
}

/// 读取设置文件；缺失返回默认值，**损坏时先备份再回落默认值**。
///
/// 不能只是静默回落：下一次 `save` 会把默认值写回去，用户原配置就此永久消失。
pub fn load(path: &Path) -> AppSettings {
    let Ok(bytes) = fs::read(path) else {
        return AppSettings::default();
    };
    match serde_json::from_slice::<AppSettings>(&bytes) {
        Ok(s) => s.sanitized(),
        Err(_) => {
            let _ = backup_broken(path);
            AppSettings::default()
        }
    }
}

/// 把损坏的设置文件改名归档为 `settings.json.broken-YYYYMMDD`，重名再追加 `-2`、`-3`…
fn backup_broken(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_string_lossy().to_string();
    let stamp = chrono::Local::now().format("%Y%m%d").to_string();
    for i in 1..=100u32 {
        let candidate = path.with_file_name(if i == 1 {
            format!("{name}.broken-{stamp}")
        } else {
            format!("{name}.broken-{stamp}-{i}")
        });
        if !candidate.exists() {
            return fs::rename(path, &candidate).ok().map(|()| candidate);
        }
    }
    None
}

/// 原子保存设置文件（先写 `<path>.tmp` 再 rename），自动创建父目录。
pub fn save(path: &Path, settings: &AppSettings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败: {e}"))?;
    }
    let tmp = tmp_path(path);
    let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&tmp, json).map_err(|e| format!("写入配置文件失败: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| format!("保存配置文件失败: {e}"))?;
    Ok(())
}

/// `settings.json` → `settings.json.tmp`
fn tmp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    path.with_file_name(format!("{name}.tmp"))
}

/// 把有序路径列表里的 `old` **原位替换**为 `new`（保持顺序），并顺手去重。
///
/// 花匣改名后必须用它：`open_box` 的 push 会让顺序错乱，`close_box` 的 retain 会直接丢条目。
pub fn replace_box_path(list: &mut Vec<String>, old: &str, new: &str) {
    for p in list.iter_mut() {
        if p == old {
            *p = new.to_string();
        }
    }
    let mut seen = std::collections::HashSet::new();
    list.retain(|p| seen.insert(p.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths;

    /// 测试里把 `<dir>` 当作数据根目录
    fn settings_path(dir: &Path) -> PathBuf {
        paths::note_settings_path(dir)
    }

    #[test]
    fn settings_roundtrip() {
        let dir = std::env::temp_dir().join("huajian_settings_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        // 缺失时返回默认值
        let s = load(&settings_path(&dir));
        assert_eq!(s.theme, "warm-paper");
        assert!(!s.dark);
        assert_eq!(s.sidebar_width, 300);
        assert_eq!(s.width_mode(), WIDTH_MODE_LEFT75, "默认应为靠左 75%");
        assert!(s.boxes.is_empty());

        // 保存并重新读取
        let mut s2 = AppSettings::default();
        s2.theme = "ink".into();
        s2.dark = true;
        s2.sidebar_width = 420;
        s2.width_mode = Some(WIDTH_MODE_FULL.into());
        s2.boxes = vec!["/a/one.hxl".into(), "/b/two.hxl".into()];
        save(&settings_path(&dir), &s2).unwrap();

        let s3 = load(&settings_path(&dir));
        assert_eq!(s3.theme, "ink");
        assert!(s3.dark);
        assert_eq!(s3.sidebar_width, 420);
        assert_eq!(s3.width_mode(), WIDTH_MODE_FULL);
        assert_eq!(s3.boxes, vec!["/a/one.hxl", "/b/two.hxl"]);

        // 落在 花笺/settings.json 里
        assert!(paths::note_settings_path(&dir).exists());

        let _ = fs::remove_dir_all(&dir);
    }

    /// 旧版 settings.json 里的未知字段必须能读，多余字段直接忽略
    #[test]
    fn legacy_settings_with_unknown_field() {
        let dir = std::env::temp_dir().join("huajian_settings_legacy_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(paths::note_dir(&dir)).unwrap();

        fs::write(
            settings_path(&dir),
            br#"{"theme":"bamboo","dark":true,"sidebar_width":360,"boxes":["/x.hxl"],
                 "unknown_field":{"mode":"forward","focus_minutes":50}}"#,
        )
        .unwrap();

        let s = load(&settings_path(&dir));
        assert_eq!(s.theme, "bamboo");
        assert!(s.dark);
        assert_eq!(s.sidebar_width, 360);
        assert_eq!(s.boxes, vec!["/x.hxl"]);

        // 保存后不再写出未知字段
        save(&settings_path(&dir), &s).unwrap();
        let raw = fs::read_to_string(settings_path(&dir)).unwrap();
        assert!(!raw.contains("unknown_field"), "{raw}");

        let _ = fs::remove_dir_all(&dir);
    }

    /// 全新配置（既无 width_mode 也无 full_width）→ 靠左 75%
    #[test]
    fn legacy_settings_without_width_mode() {
        let dir = std::env::temp_dir().join("huajian_settings_legacy_width_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(paths::note_dir(&dir)).unwrap();

        fs::write(
            settings_path(&dir),
            br#"{"theme":"bamboo","dark":true,"sidebar_width":360,"boxes":["/x.hxl"]}"#,
        )
        .unwrap();

        let s = load(&settings_path(&dir));
        assert_eq!(s.width_mode(), WIDTH_MODE_LEFT75, "缺字段时应用新默认靠左 75%");

        let _ = fs::remove_dir_all(&dir);
    }

    /// 老配置里显式选过铺满/专注阅读的，升级后要尊重原选择
    #[test]
    fn legacy_full_width_migrates_to_mode() {
        let dir = std::env::temp_dir().join("huajian_settings_migrate_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(paths::note_dir(&dir)).unwrap();

        // full_width: true → 铺满
        fs::write(settings_path(&dir), br#"{"full_width":true}"#).unwrap();
        assert_eq!(load(&settings_path(&dir)).width_mode(), WIDTH_MODE_FULL);

        // full_width: false（旧的「专注阅读」档）→ 模式已删除，回落到靠左 75%
        fs::write(settings_path(&dir), br#"{"full_width":false}"#).unwrap();
        assert_eq!(load(&settings_path(&dir)).width_mode(), WIDTH_MODE_LEFT75);

        // 老配置里已保存的 narrow 同样回落到靠左 75%
        fs::write(settings_path(&dir), br#"{"width_mode":"narrow"}"#).unwrap();
        assert_eq!(load(&settings_path(&dir)).width_mode(), WIDTH_MODE_LEFT75);

        // 已写 width_mode 的优先，不受遗留 full_width 影响
        fs::write(
            settings_path(&dir),
            br#"{"width_mode":"left75","full_width":true}"#,
        )
        .unwrap();
        assert_eq!(load(&settings_path(&dir)).width_mode(), WIDTH_MODE_LEFT75);

        // 未知模式回落到默认
        fs::write(settings_path(&dir), br#"{"width_mode":"wat"}"#).unwrap();
        assert_eq!(load(&settings_path(&dir)).width_mode(), WIDTH_MODE_LEFT75);

        let _ = fs::remove_dir_all(&dir);
    }

    /// 保存时不再写出旧字段 full_width
    #[test]
    fn save_drops_legacy_full_width() {
        let dir = std::env::temp_dir().join("huajian_settings_drop_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(paths::note_dir(&dir)).unwrap();

        save(&settings_path(&dir), &AppSettings::default()).unwrap();
        let raw = fs::read_to_string(settings_path(&dir)).unwrap();
        assert!(!raw.contains("full_width"), "不应再写出 full_width: {raw}");
        assert!(raw.contains("width_mode"));

        let _ = fs::remove_dir_all(&dir);
    }

    /// 保存到还存在的多级目录时自动建目录
    #[test]
    fn save_creates_parent_directory() {
        let dir = std::env::temp_dir().join("huajian_settings_mkdir_test");
        let _ = fs::remove_dir_all(&dir);

        save(&settings_path(&dir), &AppSettings::default()).unwrap();
        assert!(paths::note_settings_path(&dir).exists());

        let _ = fs::remove_dir_all(&dir);
    }

    /// 花匣改名：旧路径原位替换为新路径，顺序不变，重复项被去掉
    #[test]
    fn replace_box_path_keeps_order_and_dedupes() {
        let mut list = vec![
            "/a.hxl".to_string(),
            "/b.hxl".to_string(),
            "/c.hxl".to_string(),
        ];
        replace_box_path(&mut list, "/b.hxl", "/B.hxl");
        assert_eq!(list, vec!["/a.hxl", "/B.hxl", "/c.hxl"]);

        // 替换后与已有条目重名 → 只保留第一个，顺序仍不乱
        let mut dup = vec![
            "/a.hxl".to_string(),
            "/b.hxl".to_string(),
            "/c.hxl".to_string(),
        ];
        replace_box_path(&mut dup, "/c.hxl", "/a.hxl");
        assert_eq!(dup, vec!["/a.hxl", "/b.hxl"]);

        // 旧路径不在列表里：只去重，不改内容顺序
        let mut missing = vec!["/a.hxl".to_string(), "/a.hxl".to_string(), "/b.hxl".to_string()];
        replace_box_path(&mut missing, "/nope.hxl", "/new.hxl");
        assert_eq!(missing, vec!["/a.hxl", "/b.hxl"]);
    }
}