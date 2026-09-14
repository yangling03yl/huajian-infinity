use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 用户设置，存放于系统配置目录（Windows: %APPDATA%\com.huajian.app，Linux: ~/.config/com.huajian.app）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub dark: bool,
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: u32,
    /// 正文宽度模式：left75（靠左 75%，默认）/ full（铺满）/ narrow（专注阅读居中）
    /// 用 Option 以区分「老配置里没写过」与「写过具体值」，供迁移判断
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width_mode: Option<String>,
    /// 旧版字段：正文是否铺满。仅用于读取老配置并迁移到 width_mode，不再写出
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_width: Option<bool>,
    /// 已打开花匣的有序路径（启动时按此顺序自动打开）
    #[serde(default)]
    pub boxes: Vec<String>,
    /// 番茄钟设置
    #[serde(default)]
    pub pomodoro: PomodoroSettings,
}

/// 正文宽度模式：靠左 75%（默认）
pub const WIDTH_MODE_LEFT75: &str = "left75";
/// 正文宽度模式：铺满窗口
pub const WIDTH_MODE_FULL: &str = "full";
/// 正文宽度模式：专注阅读（居中窄栏）
pub const WIDTH_MODE_NARROW: &str = "narrow";

/// 番茄钟计时模式：标准（倒计时）
pub const POMODORO_MODE_STANDARD: &str = "standard";
/// 番茄钟计时模式：正向（正计时，手动结束）
pub const POMODORO_MODE_FORWARD: &str = "forward";

pub const MIN_FOCUS_MINUTES: u32 = 1;
pub const MAX_FOCUS_MINUTES: u32 = 180;
pub const MIN_BREAK_MINUTES: u32 = 0;
pub const MAX_BREAK_MINUTES: u32 = 60;
pub const MIN_LOOPS: u32 = 1;
pub const MAX_LOOPS: u32 = 12;

/// 番茄钟设置：模式、单个番茄时长、循环内相邻番茄之间的休息时长、一次会话的番茄个数
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PomodoroSettings {
    #[serde(default = "default_pomodoro_mode")]
    pub mode: String,
    #[serde(default = "default_focus_minutes")]
    pub focus_minutes: u32,
    #[serde(default = "default_break_minutes")]
    pub break_minutes: u32,
    #[serde(default = "default_loops")]
    pub loops: u32,
}

fn default_theme() -> String {
    "warm-paper".into()
}

fn default_sidebar_width() -> u32 {
    300
}

fn default_width_mode() -> String {
    WIDTH_MODE_LEFT75.into()
}

fn default_pomodoro_mode() -> String {
    POMODORO_MODE_STANDARD.into()
}

fn default_focus_minutes() -> u32 {
    25
}

fn default_break_minutes() -> u32 {
    5
}

fn default_loops() -> u32 {
    4
}

impl Default for PomodoroSettings {
    fn default() -> Self {
        Self {
            mode: default_pomodoro_mode(),
            focus_minutes: default_focus_minutes(),
            break_minutes: default_break_minutes(),
            loops: default_loops(),
        }
    }
}

impl PomodoroSettings {
    /// 把外部传入的值夹取到合法范围，未知模式回落到标准模式
    pub fn sanitized(mut self) -> Self {
        if self.mode != POMODORO_MODE_FORWARD {
            self.mode = POMODORO_MODE_STANDARD.into();
        }
        self.focus_minutes = self.focus_minutes.clamp(MIN_FOCUS_MINUTES, MAX_FOCUS_MINUTES);
        self.break_minutes = self.break_minutes.clamp(MIN_BREAK_MINUTES, MAX_BREAK_MINUTES);
        self.loops = self.loops.clamp(MIN_LOOPS, MAX_LOOPS);
        self
    }
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
            pomodoro: PomodoroSettings::default(),
        }
    }
}

/// 内存中的用户状态
pub struct AppState {
    pub settings: AppSettings,
}

impl AppSettings {
    /// 归一化：未知宽度模式回落到默认；老配置的 full_width 迁移到 width_mode
    ///
    /// 迁移规则：老配置里显式写了 full_width 的（true→铺满，false→专注阅读）尊重原选择；
    /// 完全没写过的用新默认「靠左 75%」，避免升级后布局漂移。
    pub fn sanitized(mut self) -> Self {
        let valid = matches!(
            self.width_mode.as_deref(),
            Some(WIDTH_MODE_LEFT75) | Some(WIDTH_MODE_FULL) | Some(WIDTH_MODE_NARROW)
        );
        if !valid {
            self.width_mode = Some(match self.full_width {
                Some(true) => WIDTH_MODE_FULL.into(),
                Some(false) => WIDTH_MODE_NARROW.into(),
                None => default_width_mode(),
            });
        }
        // 迁移完成后不再保留旧字段
        self.full_width = None;
        self
    }

    /// 取当前宽度模式（字段缺失时按默认）
    pub fn width_mode(&self) -> &str {
        self.width_mode.as_deref().unwrap_or(WIDTH_MODE_LEFT75)
    }
}

fn settings_path(config_dir: &Path) -> PathBuf {
    config_dir.join("settings.json")
}

pub fn load(config_dir: &Path) -> AppSettings {
    match fs::read(settings_path(config_dir))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<AppSettings>(&bytes).ok())
    {
        Some(s) => s.sanitized(),
        None => AppSettings::default(),
    }
}

pub fn save(config_dir: &Path, settings: &AppSettings) -> Result<(), String> {
    fs::create_dir_all(config_dir).map_err(|e| format!("创建配置目录失败: {e}"))?;
    let path = settings_path(config_dir);
    let tmp = config_dir.join("settings.json.tmp");
    let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&tmp, json).map_err(|e| format!("写入配置文件失败: {e}"))?;
    fs::rename(&tmp, &path).map_err(|e| format!("保存配置文件失败: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip() {
        let dir = std::env::temp_dir().join("huajian_settings_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        // 缺失时返回默认值
        let s = load(&dir);
        assert_eq!(s.theme, "warm-paper");
        assert!(!s.dark);
        assert_eq!(s.sidebar_width, 300);
        assert_eq!(s.width_mode(), WIDTH_MODE_LEFT75, "默认应为靠左 75%");
        assert!(s.boxes.is_empty());
        assert_eq!(s.pomodoro, PomodoroSettings::default());
        assert_eq!(s.pomodoro.mode, POMODORO_MODE_STANDARD);
        assert_eq!(s.pomodoro.focus_minutes, 25);
        assert_eq!(s.pomodoro.break_minutes, 5);
        assert_eq!(s.pomodoro.loops, 4);

        // 保存并重新读取
        let mut s2 = AppSettings::default();
        s2.theme = "ink".into();
        s2.dark = true;
        s2.sidebar_width = 420;
        s2.width_mode = Some(WIDTH_MODE_NARROW.into());
        s2.boxes = vec!["/a/one.hxl".into(), "/b/two.hxl".into()];
        s2.pomodoro = PomodoroSettings {
            mode: POMODORO_MODE_FORWARD.into(),
            focus_minutes: 50,
            break_minutes: 10,
            loops: 3,
        };
        save(&dir, &s2).unwrap();

        let s3 = load(&dir);
        assert_eq!(s3.theme, "ink");
        assert!(s3.dark);
        assert_eq!(s3.sidebar_width, 420);
        assert_eq!(s3.width_mode(), WIDTH_MODE_NARROW);
        assert_eq!(s3.boxes, vec!["/a/one.hxl", "/b/two.hxl"]);
        assert_eq!(s3.pomodoro, s2.pomodoro);

        let _ = fs::remove_dir_all(&dir);
    }

    /// 旧版 settings.json（无 pomodoro 字段）必须能读，并补上默认番茄钟设置
    #[test]
    fn legacy_settings_without_pomodoro() {
        let dir = std::env::temp_dir().join("huajian_settings_legacy_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        fs::write(
            settings_path(&dir),
            br#"{"theme":"bamboo","dark":true,"sidebar_width":360,"boxes":["/x.hxl"]}"#,
        )
        .unwrap();

        let s = load(&dir);
        assert_eq!(s.theme, "bamboo");
        assert!(s.dark);
        assert_eq!(s.sidebar_width, 360);
        assert_eq!(s.pomodoro, PomodoroSettings::default());

        let _ = fs::remove_dir_all(&dir);
    }

    /// 全新配置（既无 width_mode 也无 full_width）→ 靠左 75%
    #[test]
    fn legacy_settings_without_width_mode() {
        let dir = std::env::temp_dir().join("huajian_settings_legacy_width_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        fs::write(
            settings_path(&dir),
            br#"{"theme":"bamboo","dark":true,"sidebar_width":360,"boxes":["/x.hxl"]}"#,
        )
        .unwrap();

        let s = load(&dir);
        assert_eq!(s.width_mode(), WIDTH_MODE_LEFT75, "缺字段时应用新默认靠左 75%");

        let _ = fs::remove_dir_all(&dir);
    }

    /// 老配置里显式选过铺满/专注阅读的，升级后要尊重原选择
    #[test]
    fn legacy_full_width_migrates_to_mode() {
        let dir = std::env::temp_dir().join("huajian_settings_migrate_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        // full_width: true → 铺满
        fs::write(settings_path(&dir), br#"{"full_width":true}"#).unwrap();
        assert_eq!(load(&dir).width_mode(), WIDTH_MODE_FULL);

        // full_width: false → 专注阅读
        fs::write(settings_path(&dir), br#"{"full_width":false}"#).unwrap();
        assert_eq!(load(&dir).width_mode(), WIDTH_MODE_NARROW);

        // 已写 width_mode 的优先，不受遗留 full_width 影响
        fs::write(
            settings_path(&dir),
            br#"{"width_mode":"left75","full_width":true}"#,
        )
        .unwrap();
        assert_eq!(load(&dir).width_mode(), WIDTH_MODE_LEFT75);

        // 未知模式回落到默认
        fs::write(settings_path(&dir), br#"{"width_mode":"wat"}"#).unwrap();
        assert_eq!(load(&dir).width_mode(), WIDTH_MODE_LEFT75);

        let _ = fs::remove_dir_all(&dir);
    }

    /// 保存时不再写出旧字段 full_width
    #[test]
    fn save_drops_legacy_full_width() {
        let dir = std::env::temp_dir().join("huajian_settings_drop_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        save(&dir, &AppSettings::default()).unwrap();
        let raw = fs::read_to_string(settings_path(&dir)).unwrap();
        assert!(!raw.contains("full_width"), "不应再写出 full_width: {raw}");
        assert!(raw.contains("width_mode"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn pomodoro_sanitized_clamps_range() {
        let s = PomodoroSettings {
            mode: "unknown".into(),
            focus_minutes: 0,
            break_minutes: 999,
            loops: 0,
        }
        .sanitized();
        assert_eq!(s.mode, POMODORO_MODE_STANDARD);
        assert_eq!(s.focus_minutes, MIN_FOCUS_MINUTES);
        assert_eq!(s.break_minutes, MAX_BREAK_MINUTES);
        assert_eq!(s.loops, MIN_LOOPS);

        let f = PomodoroSettings {
            mode: POMODORO_MODE_FORWARD.into(),
            focus_minutes: 400,
            break_minutes: 0,
            loops: 99,
        }
        .sanitized();
        assert_eq!(f.mode, POMODORO_MODE_FORWARD);
        assert_eq!(f.focus_minutes, MAX_FOCUS_MINUTES);
        assert_eq!(f.break_minutes, 0);
        assert_eq!(f.loops, MAX_LOOPS);
    }
}
