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
    /// 已打开花匣的有序路径（启动时按此顺序自动打开）
    #[serde(default)]
    pub boxes: Vec<String>,
}

fn default_theme() -> String {
    "warm-paper".into()
}

fn default_sidebar_width() -> u32 {
    300
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            dark: false,
            sidebar_width: default_sidebar_width(),
            boxes: Vec::new(),
        }
    }
}

/// 内存中的用户状态
pub struct AppState {
    pub settings: AppSettings,
}

fn settings_path(config_dir: &Path) -> PathBuf {
    config_dir.join("settings.json")
}

pub fn load(config_dir: &Path) -> AppSettings {
    match fs::read(settings_path(config_dir))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
    {
        Some(s) => s,
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
        assert!(s.boxes.is_empty());

        // 保存并重新读取
        let mut s2 = AppSettings::default();
        s2.theme = "ink".into();
        s2.dark = true;
        s2.sidebar_width = 420;
        s2.boxes = vec!["/a/one.hxl".into(), "/b/two.hxl".into()];
        save(&dir, &s2).unwrap();

        let s3 = load(&dir);
        assert_eq!(s3.theme, "ink");
        assert!(s3.dark);
        assert_eq!(s3.sidebar_width, 420);
        assert_eq!(s3.boxes, vec!["/a/one.hxl", "/b/two.hxl"]);

        let _ = fs::remove_dir_all(&dir);
    }
}
