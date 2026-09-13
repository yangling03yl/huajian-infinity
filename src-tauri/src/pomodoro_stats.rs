use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::settings::POMODORO_MODE_FORWARD;

/// 单次番茄记录的最长时长（24 小时），超出视为异常数据直接丢弃
pub const MAX_SESSION_SECONDS: u64 = 24 * 60 * 60;

/// 单日累计时长上限（24 小时），防御性上限，避免脏数据（例如系统时间被改）撑爆统计
const MAX_DAY_SECONDS: u64 = 24 * 60 * 60;

/// 单日保留的单次记录上限（正常一天不会超过几十次，超出只累加合计不再存明细）
const MAX_SESSIONS_PER_DAY: usize = 200;

/// 单次番茄记录（本地时间 HH:MM）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PomodoroSession {
    /// 完成时刻（本地时间 HH:MM）
    pub at: String,
    pub seconds: u64,
    /// standard | forward
    pub mode: String,
}

/// 单日番茄统计：按本地日期聚合的合计 + 当日单次记录
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PomodoroDayStats {
    /// 本地日期 YYYY-MM-DD
    pub date: String,
    #[serde(default)]
    pub standard_seconds: u64,
    #[serde(default)]
    pub standard_count: u32,
    #[serde(default)]
    pub forward_seconds: u64,
    #[serde(default)]
    pub forward_count: u32,
    /// 当日的单次番茄记录（旧数据文件没有该字段，默认为空）
    #[serde(default)]
    pub sessions: Vec<PomodoroSession>,
}

impl PomodoroDayStats {
    fn new(date: &str) -> Self {
        Self {
            date: date.to_string(),
            standard_seconds: 0,
            standard_count: 0,
            forward_seconds: 0,
            forward_count: 0,
            sessions: Vec::new(),
        }
    }

    fn add(&mut self, at: &str, seconds: u64, mode: &str) {
        if mode == POMODORO_MODE_FORWARD {
            self.forward_seconds = (self.forward_seconds + seconds).min(MAX_DAY_SECONDS);
            self.forward_count = self.forward_count.saturating_add(1);
        } else {
            self.standard_seconds = (self.standard_seconds + seconds).min(MAX_DAY_SECONDS);
            self.standard_count = self.standard_count.saturating_add(1);
        }
        if self.sessions.len() < MAX_SESSIONS_PER_DAY {
            self.sessions.push(PomodoroSession {
                at: at.to_string(),
                seconds,
                mode: mode.to_string(),
            });
        }
    }
}

/// 全部番茄统计，days 按日期升序排列
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PomodoroStats {
    #[serde(default)]
    pub days: Vec<PomodoroDayStats>,
}

fn stats_path(config_dir: &Path) -> PathBuf {
    config_dir.join("pomodoro_stats.json")
}

/// 当前本地日期与时间（YYYY-MM-DD, HH:MM）。
/// 统计按用户感知的本地日期分桶，因此这里用 Local 而非 Utc。
pub fn now_local() -> (String, String) {
    let now = chrono::Local::now();
    (
        now.format("%Y-%m-%d").to_string(),
        now.format("%H:%M").to_string(),
    )
}

pub fn load(config_dir: &Path) -> PomodoroStats {
    match fs::read(stats_path(config_dir))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
    {
        Some(s) => s,
        None => PomodoroStats::default(),
    }
}

pub fn save(config_dir: &Path, stats: &PomodoroStats) -> Result<(), String> {
    fs::create_dir_all(config_dir).map_err(|e| format!("创建配置目录失败: {e}"))?;
    let path = stats_path(config_dir);
    let tmp = config_dir.join("pomodoro_stats.json.tmp");
    let json = serde_json::to_vec_pretty(stats).map_err(|e| e.to_string())?;
    fs::write(&tmp, json).map_err(|e| format!("写入统计文件失败: {e}"))?;
    fs::rename(&tmp, &path).map_err(|e| format!("保存统计文件失败: {e}"))?;
    Ok(())
}

impl PomodoroStats {
    /// 记录一个完成的番茄：同一天累加合计并追加单次记录，新日期按日期升序插入。
    /// 时长非法（0 或超过 24 小时）时返回 false 且不记录。
    pub fn record(&mut self, date: &str, at: &str, seconds: u64, mode: &str) -> bool {
        if seconds == 0 || seconds > MAX_SESSION_SECONDS {
            return false;
        }
        match self.days.binary_search_by(|d| d.date.as_str().cmp(date)) {
            Ok(i) => self.days[i].add(at, seconds, mode),
            Err(i) => {
                let mut day = PomodoroDayStats::new(date);
                day.add(at, seconds, mode);
                self.days.insert(i, day);
            }
        }
        true
    }

    /// 某年某月（month 为 1-12）的全部记录
    pub fn month(&self, year: i32, month: u32) -> Vec<PomodoroDayStats> {
        let prefix = format!("{year:04}-{month:02}-");
        self.days
            .iter()
            .filter(|d| d.date.starts_with(&prefix))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::POMODORO_MODE_STANDARD;

    #[test]
    fn missing_file_returns_default() {
        let dir = std::env::temp_dir().join("huajian_stats_missing_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let s = load(&dir);
        assert!(s.days.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_falls_back_to_default() {
        let dir = std::env::temp_dir().join("huajian_stats_corrupt_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        fs::write(stats_path(&dir), b"{ this is not json").unwrap();
        assert!(load(&dir).days.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    /// 旧版统计文件（只有合计、没有 sessions 字段）必须能读，且明细为空
    #[test]
    fn legacy_day_without_sessions_field() {
        let dir = std::env::temp_dir().join("huajian_stats_legacy_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        fs::write(
            stats_path(&dir),
            br#"{"days":[{"date":"2026-02-10","standard_seconds":1500,"standard_count":1,"forward_seconds":600,"forward_count":1}]}"#,
        )
        .unwrap();

        let s = load(&dir);
        assert_eq!(s.days.len(), 1);
        assert_eq!(s.days[0].standard_seconds, 1500);
        assert_eq!(s.days[0].forward_count, 1);
        assert!(s.days[0].sessions.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn record_aggregates_and_keeps_sorted() {
        let mut stats = PomodoroStats::default();
        assert!(stats.record("2026-02-10", "09:15", 1500, POMODORO_MODE_STANDARD));
        assert!(stats.record("2026-02-10", "10:05", 1500, POMODORO_MODE_STANDARD));
        assert!(stats.record("2026-02-10", "11:00", 600, POMODORO_MODE_FORWARD));
        assert!(stats.record("2026-02-09", "20:30", 900, POMODORO_MODE_FORWARD));
        assert!(stats.record("2026-02-11", "08:00", 1200, POMODORO_MODE_STANDARD));

        // 日期升序
        let dates: Vec<&str> = stats.days.iter().map(|d| d.date.as_str()).collect();
        assert_eq!(dates, vec!["2026-02-09", "2026-02-10", "2026-02-11"]);

        let d = &stats.days[1];
        assert_eq!(d.standard_seconds, 3000);
        assert_eq!(d.standard_count, 2);
        assert_eq!(d.forward_seconds, 600);
        assert_eq!(d.forward_count, 1);
        assert_eq!(d.standard_seconds + d.forward_seconds, 3600);
        assert_eq!(d.standard_count + d.forward_count, 3);

        // 单次记录按发生顺序保留，含时刻与模式
        assert_eq!(
            d.sessions,
            vec![
                PomodoroSession {
                    at: "09:15".into(),
                    seconds: 1500,
                    mode: POMODORO_MODE_STANDARD.into()
                },
                PomodoroSession {
                    at: "10:05".into(),
                    seconds: 1500,
                    mode: POMODORO_MODE_STANDARD.into()
                },
                PomodoroSession {
                    at: "11:00".into(),
                    seconds: 600,
                    mode: POMODORO_MODE_FORWARD.into()
                },
            ]
        );

        // 重新保存后读取一致（含明细）
        let dir = std::env::temp_dir().join("huajian_stats_roundtrip_test");
        let _ = fs::remove_dir_all(&dir);
        save(&dir, &stats).unwrap();
        let reloaded = load(&dir);
        assert_eq!(reloaded.days, stats.days);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn record_rejects_invalid_seconds() {
        let mut stats = PomodoroStats::default();
        assert!(!stats.record("2026-02-10", "09:00", 0, POMODORO_MODE_STANDARD));
        assert!(!stats.record(
            "2026-02-10",
            "09:00",
            MAX_SESSION_SECONDS + 1,
            POMODORO_MODE_STANDARD
        ));
        assert!(stats.days.is_empty());
    }

    /// 明细有上限，超出只累加合计（避免单日记录无限膨胀）
    #[test]
    fn sessions_are_capped_per_day() {
        let mut stats = PomodoroStats::default();
        for i in 0..(MAX_SESSIONS_PER_DAY + 5) {
            assert!(stats.record("2026-02-10", "09:00", 60, POMODORO_MODE_STANDARD));
            let _ = i;
        }
        let d = &stats.days[0];
        assert_eq!(d.sessions.len(), MAX_SESSIONS_PER_DAY);
        assert_eq!(d.standard_count as usize, MAX_SESSIONS_PER_DAY + 5);
    }

    #[test]
    fn month_filter_matches_only_that_month() {
        let mut stats = PomodoroStats::default();
        stats.record("2025-12-31", "23:00", 60, POMODORO_MODE_STANDARD);
        stats.record("2026-01-01", "09:00", 60, POMODORO_MODE_STANDARD);
        stats.record("2026-01-31", "21:00", 60, POMODORO_MODE_FORWARD);
        stats.record("2026-02-01", "10:00", 60, POMODORO_MODE_STANDARD);
        stats.record("2027-01-01", "11:00", 60, POMODORO_MODE_STANDARD);

        let jan = stats.month(2026, 1);
        assert_eq!(jan.len(), 2);
        assert_eq!(jan[0].date, "2026-01-01");
        assert_eq!(jan[1].date, "2026-01-31");
        assert_eq!(jan[1].forward_count, 1);
        assert_eq!(jan[1].sessions[0].at, "21:00");

        assert_eq!(stats.month(2026, 2).len(), 1);
        assert_eq!(stats.month(2026, 3).len(), 0);
        // 年份不同不能混入
        assert_eq!(stats.month(2025, 12).len(), 1);
    }

    #[test]
    fn local_date_and_time_have_expected_shape() {
        let (date, time) = now_local();
        assert_eq!(date.len(), 10);
        assert_eq!(date.as_bytes()[4], b'-');
        assert_eq!(date.as_bytes()[7], b'-');
        assert_eq!(time.len(), 5);
        assert_eq!(time.as_bytes()[2], b':');
    }

    #[test]
    fn unknown_mode_counts_as_standard() {
        let mut stats = PomodoroStats::default();
        assert!(stats.record("2026-02-10", "12:00", 60, "whatever"));
        assert_eq!(stats.days[0].standard_count, 1);
        assert_eq!(stats.days[0].forward_count, 0);
        assert_eq!(stats.days[0].sessions.len(), 1);
    }
}
