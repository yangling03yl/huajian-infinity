//! 花钟用户数据层：`<root>/花钟.hz`（无压缩 tar 套一个 `huazhong.json`）。
//!
//! 规格见 `SPEC-huazhong-v2.md` 第 3～7 节。要点：
//! - 单文件、可整体拷到别的设备；时间只存本地墙上时钟，不存时区/时间戳；
//! - 两边（花笺 / 花钟）可能同时计时写同一份文件：写操作一律
//!   `取锁 → 重新 load → 变更 → 重算 → 原子写 → 释放锁`；
//! - 合计一律由 `baseline + 明细并集` 重算，文件里的派生缓存被手改也不会错账。
//!
//! 注意：本文件与花钟 app 的同名文件必须**逐字节相同**，任何改动都要两边同步。

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::paths;

/// `花钟.hz` 的格式版本，恒为 2
pub const HUAZHONG_FORMAT: u32 = 2;
/// `花钟.hz` 里的标识
pub const HUAZHONG_APP: &str = "huazhong";
/// tar 内唯一条目名
pub const HUAZHONG_JSON_ENTRY: &str = "huazhong.json";

/// 单次番茄记录的最长时长（24 小时），超出视为异常数据直接丢弃
pub const MAX_SESSION_SECONDS: u64 = 24 * 60 * 60;

/// 一天的秒数
const SECONDS_PER_DAY: i64 = 86_400;
/// 抢锁时每次重试的间隔
const LOCK_RETRY_INTERVAL: Duration = Duration::from_millis(50);
/// 抢锁总超时
const LOCK_TIMEOUT: Duration = Duration::from_secs(2);
/// 锁文件 mtime 早于当前时间这么久，视为陈旧锁
const LOCK_STALE_AFTER: Duration = Duration::from_secs(10);

// ---- 番茄钟设置（自 settings.rs 搬来，属于花钟数据） ----

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

// ---- 每日数据 ----

/// 旧版"只有合计、没有明细"的日期使用的原始合计（新记录一律为 0）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PomodoroBaseline {
    #[serde(default)]
    pub standard_seconds: u64,
    #[serde(default)]
    pub standard_count: u32,
    #[serde(default)]
    pub forward_seconds: u64,
    #[serde(default)]
    pub forward_count: u32,
}

impl PomodoroBaseline {
    pub fn is_zero(&self) -> bool {
        self.standard_seconds == 0
            && self.standard_count == 0
            && self.forward_seconds == 0
            && self.forward_count == 0
    }

    fn merge(&mut self, other: Self) {
        self.standard_seconds = self.standard_seconds.saturating_add(other.standard_seconds);
        self.standard_count = self.standard_count.saturating_add(other.standard_count);
        self.forward_seconds = self.forward_seconds.saturating_add(other.forward_seconds);
        self.forward_count = self.forward_count.saturating_add(other.forward_count);
    }

    fn seconds(&self) -> u64 {
        self.standard_seconds.saturating_add(self.forward_seconds)
    }
}

/// 单次番茄记录
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PomodoroSession {
    /// uuid v4 字符串；旧文件缺失时读取阶段补发
    pub id: String,
    /// 完成时刻 `HH:MM`（展示用，由 `end` 派生）
    pub at: String,
    /// 时长秒；合法区间 `1..=86400`，越界记录读取时丢弃
    pub seconds: u64,
    /// `standard` | `forward`
    pub mode: String,
    /// 本地墙上时钟 `HH:MM:SS`；完成时 `end = 当前本地时刻`，`start = end − seconds`
    pub start: String,
    /// 本地墙上时钟 `HH:MM:SS`
    pub end: String,
}

/// 单日番茄统计
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PomodoroDayStats {
    /// 本地日期 `YYYY-MM-DD`
    pub date: String,
    /// 仅旧版汇总日使用；为 0 时也会写出（保证对端解析不缺字段）
    #[serde(default)]
    pub baseline: PomodoroBaseline,
    /// 派生缓存：`baseline.standard_count + 标准明细条数`
    #[serde(default)]
    pub standard_count: u32,
    /// 派生缓存：`baseline.forward_count + 正向明细条数`
    #[serde(default)]
    pub forward_count: u32,
    /// 派生缓存：`baseline.standard_seconds + 标准明细并集秒`
    #[serde(default)]
    pub standard_seconds: u64,
    /// 派生缓存：`baseline.forward_seconds + 正向明细并集秒`
    #[serde(default)]
    pub forward_seconds: u64,
    /// 当日单次记录（按发生顺序）
    #[serde(default)]
    pub sessions: Vec<PomodoroSession>,
}

impl PomodoroDayStats {
    fn empty(date: &str) -> Self {
        Self {
            date: date.to_string(),
            baseline: PomodoroBaseline::default(),
            standard_count: 0,
            forward_count: 0,
            standard_seconds: 0,
            forward_seconds: 0,
            sessions: Vec::new(),
        }
    }

    /// 由 `baseline + 明细` 重算派生缓存
    pub fn recompute(&mut self) {
        let mut standard_intervals = Vec::new();
        let mut forward_intervals = Vec::new();
        for session in &self.sessions {
            if is_forward(&session.mode) {
                if let Some(interval) = session_interval(session) {
                    forward_intervals.push(interval);
                }
            } else if let Some(interval) = session_interval(session) {
                standard_intervals.push(interval);
            }
        }
        // 次数按**原始条数**（空区间/跨午夜都算一个番茄），时长才有并集去重
        self.standard_count = self
            .baseline
            .standard_count
            .saturating_add(self.sessions.iter().filter(|s| !is_forward(&s.mode)).count() as u32);
        self.forward_count = self
            .baseline
            .forward_count
            .saturating_add(self.sessions.iter().filter(|s| is_forward(&s.mode)).count() as u32);
        self.standard_seconds = self
            .baseline
            .standard_seconds
            .saturating_add(union_seconds(standard_intervals));
        self.forward_seconds = self
            .baseline
            .forward_seconds
            .saturating_add(union_seconds(forward_intervals));
    }

    /// 当天番茄个数 = `standard_count + forward_count`（按原始条数）
    pub fn total_count(&self) -> u32 {
        self.standard_count.saturating_add(self.forward_count)
    }

    /// 当天总计秒 = `baseline 秒 + 全部明细的整体并集`（跨模式重叠只算一次）
    pub fn total_seconds(&self) -> u64 {
        let intervals: Vec<(i64, i64)> = self.sessions.iter().filter_map(session_interval).collect();
        self.baseline
            .seconds()
            .saturating_add(union_seconds(intervals))
    }

    /// 无任何记录（既无合计也无明细）的空日期
    fn is_empty_day(&self) -> bool {
        self.baseline.is_zero() && self.sessions.is_empty()
    }
}

/// 花钟全部用户数据
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HuazhongData {
    /// 恒为 2
    #[serde(default = "default_format")]
    pub format: u32,
    #[serde(default = "default_app")]
    pub app: String,
    #[serde(default)]
    pub settings: PomodoroSettings,
    /// 按日期升序
    #[serde(default)]
    pub days: Vec<PomodoroDayStats>,
}

fn default_format() -> u32 {
    HUAZHONG_FORMAT
}

fn default_app() -> String {
    HUAZHONG_APP.into()
}

impl Default for HuazhongData {
    fn default() -> Self {
        Self {
            format: HUAZHONG_FORMAT,
            app: HUAZHONG_APP.into(),
            settings: PomodoroSettings::default(),
            days: Vec::new(),
        }
    }
}

impl HuazhongData {
    /// 某年某月（month 为 1-12）有记录的日期（升序）
    pub fn month(&self, year: i32, month: u32) -> Vec<PomodoroDayStats> {
        let prefix = format!("{year:04}-{month:02}-");
        self.days
            .iter()
            .filter(|d| d.date.starts_with(&prefix))
            .cloned()
            .collect()
    }

    fn day_mut(&mut self, date: &str) -> &mut PomodoroDayStats {
        match self.days.binary_search_by(|d| d.date.as_str().cmp(date)) {
            Ok(i) => &mut self.days[i],
            Err(i) => {
                self.days.insert(i, PomodoroDayStats::empty(date));
                &mut self.days[i]
            }
        }
    }

    /// 归一化：修版本字段、夹取设置、按日期排序、合并重复日期、去重 id、重算合计、丢弃空日期
    pub fn normalize(&mut self) {
        self.format = HUAZHONG_FORMAT;
        self.app = HUAZHONG_APP.into();
        self.settings = self.settings.clone().sanitized();

        self.days.sort_by(|a, b| a.date.cmp(&b.date));
        let mut merged: Vec<PomodoroDayStats> = Vec::with_capacity(self.days.len());
        for mut day in std::mem::take(&mut self.days) {
            if let Some(last) = merged.last_mut() {
                if last.date == day.date {
                    last.baseline.merge(day.baseline);
                    for session in day.sessions.drain(..) {
                        if !last.sessions.iter().any(|s| s.id == session.id) {
                            last.sessions.push(session);
                        }
                    }
                    continue;
                }
            }
            let mut seen: HashSet<String> = HashSet::new();
            day.sessions.retain(|s| seen.insert(s.id.clone()));
            merged.push(day);
        }
        self.days = merged;

        for day in &mut self.days {
            day.recompute();
        }
        self.days.retain(|d| !d.is_empty_day());
    }
}

// ---- 时间与并集 ----

/// 当前本地日期与墙上时钟（`YYYY-MM-DD`, `HH:MM:SS`）。
///
/// 数据按用户感知的本地日期分桶、只存钟点，因此用 `Local` 而不是 `Utc`，也不存时区。
pub fn now_local() -> (String, String) {
    let now = chrono::Local::now();
    (
        now.format("%Y-%m-%d").to_string(),
        now.format("%H:%M:%S").to_string(),
    )
}

/// `HH:MM:SS`（也容忍 `HH:MM`）→ `0..86399`
fn parse_clock(text: &str) -> Option<i64> {
    let parts: Vec<&str> = text.trim().split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    let hour: i64 = parts[0].parse().ok()?;
    let minute: i64 = parts[1].parse().ok()?;
    let second: i64 = if parts.len() == 3 {
        parts[2].parse().ok()?
    } else {
        0
    };
    if !(0..24).contains(&hour) || !(0..60).contains(&minute) || !(0..60).contains(&second) {
        return None;
    }
    Some(hour * 3600 + minute * 60 + second)
}

fn format_hms(seconds: i64) -> String {
    let s = seconds.rem_euclid(SECONDS_PER_DAY);
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

fn format_hhmm(seconds: i64) -> String {
    format_hms(seconds)[..5].to_string()
}

/// 单条记录的时间区间：裁剪到当天 `[0, 86400]`，空区间返回 `None`。
///
/// `start > end` 视为跨午夜的正向番茄，把 `start` 往前挪一天。
fn session_interval(session: &PomodoroSession) -> Option<(i64, i64)> {
    let start = parse_clock(&session.start)?;
    let end = parse_clock(&session.end)?;
    let start = if start > end { start - SECONDS_PER_DAY } else { start };
    let start = start.max(0);
    let end = end.min(SECONDS_PER_DAY);
    if end <= start {
        None
    } else {
        Some((start, end))
    }
}

/// 区间并集总长度：重叠只算一次，相邻照常合并
fn union_seconds(mut intervals: Vec<(i64, i64)>) -> u64 {
    if intervals.is_empty() {
        return 0;
    }
    intervals.sort_unstable();
    let mut total: i64 = 0;
    let (mut cursor_start, mut cursor_end) = intervals[0];
    for (start, end) in intervals.into_iter().skip(1) {
        if start <= cursor_end {
            cursor_end = cursor_end.max(end);
        } else {
            total += cursor_end - cursor_start;
            cursor_start = start;
            cursor_end = end;
        }
    }
    total += cursor_end - cursor_start;
    total.max(0) as u64
}

fn is_forward(mode: &str) -> bool {
    mode == POMODORO_MODE_FORWARD
}

fn normalize_mode(mode: &str) -> &'static str {
    if is_forward(mode) {
        POMODORO_MODE_FORWARD
    } else {
        POMODORO_MODE_STANDARD
    }
}

fn is_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[8..].iter().all(u8::is_ascii_digit)
}

// ---- 读取 ----

/// 读取 `花钟.hz`；缺失/损坏一律返回空数据 + 默认设置（绝不 panic，也不写盘）
pub fn load(root: &Path) -> HuazhongData {
    match fs::read(paths::huazhong_path(root)) {
        Ok(bytes) => parse_hz(&bytes).unwrap_or_default(),
        Err(_) => HuazhongData::default(),
    }
}

/// 从 tar 字节解析；不是合法 tar / 没有 `huazhong.json` / JSON 坏掉都返回 `None`
pub fn parse_hz(bytes: &[u8]) -> Option<HuazhongData> {
    let mut archive = tar::Archive::new(bytes);
    let entries = archive.entries().ok()?;
    for entry in entries {
        let Ok(mut entry) = entry else { continue };
        if !is_target_entry(&entry.path_bytes()) {
            continue;
        }
        let mut buf = Vec::new();
        if entry.read_to_end(&mut buf).is_err() {
            return None;
        }
        let value: Value = serde_json::from_slice(&buf).ok()?;
        return Some(data_from_value(value));
    }
    None
}

fn is_target_entry(raw: &[u8]) -> bool {
    let raw = raw.strip_prefix(b"./").unwrap_or(raw);
    raw == HUAZHONG_JSON_ENTRY.as_bytes()
}

/// 宽容解析：字段缺失/类型不对时补默认值或丢弃该条，最后统一重算合计。
pub fn data_from_value(value: Value) -> HuazhongData {
    let mut data = HuazhongData::default();
    let Value::Object(mut obj) = value else {
        return data;
    };
    if let Some(settings) = obj.remove("settings") {
        if let Ok(parsed) = serde_json::from_value::<PomodoroSettings>(settings) {
            data.settings = parsed;
        }
    }
    if let Some(Value::Array(days)) = obj.remove("days") {
        for day in days {
            if let Some(parsed) = day_from_value(day) {
                data.days.push(parsed);
            }
        }
    }
    data.normalize();
    data
}

fn day_from_value(value: Value) -> Option<PomodoroDayStats> {
    let Value::Object(obj) = &value else {
        return None;
    };
    let date = obj.get("date")?.as_str()?.to_string();
    if !is_date(&date) {
        return None;
    }
    let mut sessions = Vec::new();
    if let Some(Value::Array(items)) = obj.get("sessions") {
        for item in items {
            if let Some(session) = session_from_value(item) {
                sessions.push(session);
            }
        }
    }
    // 旧 `pomodoro_stats.json` 没有 baseline 对象，合计写在日期对象顶层；
    // 只有在「没有明细」时才把它当作 baseline，否则会和明细重复计数。
    let baseline = match obj.get("baseline") {
        Some(b) => serde_json::from_value::<PomodoroBaseline>(b.clone()).unwrap_or_default(),
        None if sessions.is_empty() => PomodoroBaseline {
            standard_seconds: u64_field(obj, "standard_seconds"),
            standard_count: u64_field(obj, "standard_count") as u32,
            forward_seconds: u64_field(obj, "forward_seconds"),
            forward_count: u64_field(obj, "forward_count") as u32,
        },
        None => PomodoroBaseline::default(),
    };
    let mut day = PomodoroDayStats::empty(&date);
    day.baseline = baseline;
    day.sessions = sessions;
    Some(day)
}

fn u64_field(obj: &serde_json::Map<String, Value>, key: &str) -> u64 {
    obj.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}

/// 读取单条记录时用的宽松结构：旧文件可能只有 `at`、没有 id/start/end
#[derive(Debug, Default, Deserialize)]
struct RawSession {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    at: Option<String>,
    #[serde(default)]
    start: Option<String>,
    #[serde(default)]
    end: Option<String>,
    #[serde(default)]
    seconds: Option<u64>,
    #[serde(default)]
    mode: Option<String>,
}

fn session_from_value(value: &Value) -> Option<PomodoroSession> {
    let raw: RawSession = serde_json::from_value(value.clone()).ok()?;
    let seconds = raw.seconds?;
    if seconds == 0 || seconds > MAX_SESSION_SECONDS {
        return None;
    }
    // end 优先取显式值，其次由 at 补秒，再次由 start + seconds 推；都没有则 00:00:00
    let end = raw
        .end
        .as_deref()
        .and_then(parse_clock)
        .or_else(|| raw.at.as_deref().and_then(parse_clock))
        .or_else(|| {
            raw.start
                .as_deref()
                .and_then(parse_clock)
                .map(|start| start + seconds as i64)
        })
        .unwrap_or(0);
    let start = raw
        .start
        .as_deref()
        .and_then(parse_clock)
        .unwrap_or_else(|| end - seconds as i64);
    let id = raw
        .id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    Some(PomodoroSession {
        id,
        at: format_hhmm(end),
        seconds,
        mode: normalize_mode(raw.mode.as_deref().unwrap_or_default()).to_string(),
        start: format_hms(start),
        end: format_hms(end),
    })
}

// ---- 写入 ----

/// 整份写入 `花钟.hz`：无压缩 tar + 原子替换。
///
/// 这是**覆盖式**写入（不合并磁盘上已有的记录），只用于迁移时的初次落盘与测试；
/// 常规变更请用 `record` / `delete_session` / `shorten_session` / `set_settings`。
pub fn save(root: &Path, data: &HuazhongData) -> Result<(), String> {
    let _guard = acquire_lock(root)?;
    let mut normalized = data.clone();
    normalized.normalize();
    write_atomic(root, &normalized)
}

/// 真实落盘：temp 文件 → rename（原子替换）
fn write_atomic(root: &Path, data: &HuazhongData) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|e| format!("创建数据目录失败: {e}"))?;
    let json = serde_json::to_vec_pretty(data).map_err(|e| format!("序列化花钟数据失败: {e}"))?;
    let pack = pack_tar(&json)?;

    let tmp = paths::huazhong_tmp_path(root);
    let target = paths::huazhong_path(root);
    {
        let mut file = fs::File::create(&tmp).map_err(|e| format!("写入数据文件失败: {e}"))?;
        file.write_all(&pack)
            .map_err(|e| format!("写入数据文件失败: {e}"))?;
        file.flush().map_err(|e| format!("写入数据文件失败: {e}"))?;
        let _ = file.sync_all();
    }
    fs::rename(&tmp, &target).map_err(|e| format!("保存数据文件失败: {e}"))?;
    Ok(())
}

/// 打包成无压缩 tar：单条目 `huazhong.json`，header 固定（同样内容 → 同样字节）
fn pack_tar(json: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut out);
        builder.mode(tar::HeaderMode::Deterministic);
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_mode(0o644);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_size(json.len() as u64);
        builder
            .append_data(&mut header, HUAZHONG_JSON_ENTRY, json)
            .map_err(|e| format!("打包数据文件失败: {e}"))?;
        builder
            .finish()
            .map_err(|e| format!("打包数据文件失败: {e}"))?;
    }
    Ok(out)
}

// ---- 互斥锁（手写，不引入新 crate） ----

struct LockGuard {
    path: PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn acquire_lock(root: &Path) -> Result<LockGuard, String> {
    fs::create_dir_all(root).map_err(|e| format!("创建数据目录失败: {e}"))?;
    let path = paths::huazhong_lock_path(root);
    let started = Instant::now();
    loop {
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let _ = file.write_all(b"huazhong\n");
                return Ok(LockGuard { path });
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                if is_stale_lock(&path) {
                    let _ = fs::remove_file(&path);
                    continue;
                }
                if started.elapsed() >= LOCK_TIMEOUT {
                    return Err("数据文件正被占用，请稍后重试".to_string());
                }
                std::thread::sleep(LOCK_RETRY_INTERVAL);
            }
            Err(e) => return Err(format!("创建锁文件失败: {e}")),
        }
    }
}

fn is_stale_lock(path: &Path) -> bool {
    match fs::metadata(path).and_then(|meta| meta.modified()) {
        Ok(modified) => SystemTime::now()
            .duration_since(modified)
            .map(|age| age >= LOCK_STALE_AFTER)
            .unwrap_or(false),
        Err(_) => false,
    }
}

// ---- 变更流程：取锁 → 重新 load → 变更 → 重算 → 原子写 → 释放锁 ----

fn transact<F>(root: &Path, apply: F) -> Result<HuazhongData, String>
where
    F: FnOnce(&mut HuazhongData) -> Result<(), String>,
{
    let _guard = acquire_lock(root)?;
    let mut data = load(root);
    apply(&mut data)?;
    data.normalize();
    write_atomic(root, &data)?;
    Ok(data)
}

/// 记录一个完成的番茄：`end = 当前本地时刻（秒级）`，`start = end − seconds`。
///
/// 秒数非法（0 或 > 86400）返回错误，不落盘。
pub fn record(root: &Path, seconds: u64, mode: &str) -> Result<HuazhongData, String> {
    if seconds == 0 || seconds > MAX_SESSION_SECONDS {
        return Err("番茄时长不合法，未计入统计".to_string());
    }
    let (date, end_text) = now_local();
    let mode = normalize_mode(mode);
    transact(root, move |data| {
        let end = parse_clock(&end_text).ok_or_else(|| "系统时间读取失败".to_string())?;
        let start = end - seconds as i64;
        let session = PomodoroSession {
            id: uuid::Uuid::new_v4().to_string(),
            at: format_hhmm(end),
            seconds,
            mode: mode.to_string(),
            start: format_hms(start),
            end: format_hms(end),
        };
        data.day_mut(&date).sessions.push(session);
        Ok(())
    })
}

/// 删除任意日期任意一条记录
pub fn delete_session(root: &Path, date: &str, id: &str) -> Result<HuazhongData, String> {
    transact(root, |data| {
        let day = data
            .days
            .iter_mut()
            .find(|d| d.date == date)
            .ok_or_else(|| "未找到该番茄记录".to_string())?;
        let before = day.sessions.len();
        day.sessions.retain(|s| s.id != id);
        if day.sessions.len() == before {
            return Err("未找到该番茄记录".to_string());
        }
        Ok(())
    })
}

/// 缩短一条已完成番茄的时长：`end` 不变，`start = end − seconds`，模式不变。
///
/// 要求 `1 ≤ seconds ≤ 原 seconds`，否则返回「只能减短，不能加长」。
pub fn shorten_session(root: &Path, date: &str, id: &str, seconds: u64) -> Result<HuazhongData, String> {
    transact(root, |data| {
        let day = data
            .days
            .iter_mut()
            .find(|d| d.date == date)
            .ok_or_else(|| "未找到该番茄记录".to_string())?;
        let session = day
            .sessions
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| "未找到该番茄记录".to_string())?;
        if seconds == 0 || seconds > session.seconds {
            return Err("只能减短，不能加长".to_string());
        }
        let end = parse_clock(&session.end).ok_or_else(|| "记录时间无效".to_string())?;
        session.seconds = seconds;
        session.start = format_hms(end - seconds as i64);
        Ok(())
    })
}

/// 保存番茄钟设置（越界值自动夹取）
pub fn set_settings(root: &Path, settings: PomodoroSettings) -> Result<HuazhongData, String> {
    let settings = settings.sanitized();
    transact(root, |data| {
        data.settings = settings;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths;

    fn test_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("huajian_hz_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn session(id: &str, start: &str, end: &str, mode: &str) -> PomodoroSession {
        let start_sec = parse_clock(start).unwrap();
        let end_sec = parse_clock(end).unwrap();
        PomodoroSession {
            id: id.into(),
            at: format_hhmm(end_sec),
            seconds: (end_sec - start_sec).rem_euclid(SECONDS_PER_DAY) as u64,
            mode: mode.into(),
            start: start.into(),
            end: end.into(),
        }
    }

    fn day_with(date: &str, sessions: Vec<PomodoroSession>) -> PomodoroDayStats {
        let mut day = PomodoroDayStats::empty(date);
        day.sessions = sessions;
        day.recompute();
        day
    }

    #[test]
    fn missing_file_returns_default() {
        let root = test_root("missing");
        let data = load(&root);
        assert!(data.days.is_empty());
        assert_eq!(data.format, HUAZHONG_FORMAT);
        assert_eq!(data.settings, PomodoroSettings::default());
        assert!(!paths::huazhong_path(&root).exists(), "读不应创建文件");
    }

    #[test]
    fn tar_roundtrip_is_deterministic() {
        let root = test_root("roundtrip");
        let mut data = HuazhongData::default();
        data.settings.focus_minutes = 50;
        data.days
            .push(day_with("2026-02-10", vec![session("a", "09:00:00", "09:25:00", "standard")]));
        save(&root, &data).unwrap();

        let reloaded = load(&root);
        assert_eq!(reloaded, data);

        // tar 结构：只有一个 huazhong.json 条目
        let bytes = fs::read(paths::huazhong_path(&root)).unwrap();
        let mut archive = tar::Archive::new(&bytes[..]);
        let names: Vec<String> = archive
            .entries()
            .unwrap()
            .map(|e| e.unwrap().path().unwrap().display().to_string())
            .collect();
        assert_eq!(names, vec!["huazhong.json"]);

        // 同样内容 → 同样字节（mtime/uid/gid 固定）
        save(&root, &data).unwrap();
        assert_eq!(fs::read(paths::huazhong_path(&root)).unwrap(), bytes);

        // 正常写入后不该残留锁/临时文件
        assert!(!paths::huazhong_tmp_path(&root).exists());
        assert!(!paths::huazhong_lock_path(&root).exists());
    }

    #[test]
    fn corrupt_or_foreign_files_fall_back_and_are_not_overwritten() {
        let root = test_root("corrupt");

        // 不是合法 tar
        fs::write(paths::huazhong_path(&root), b"definitely not a tar").unwrap();
        let before = fs::read(paths::huazhong_path(&root)).unwrap();
        assert!(load(&root).days.is_empty());
        assert_eq!(fs::read(paths::huazhong_path(&root)).unwrap(), before);

        // 合法 tar，但没有 huazhong.json
        let mut out = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut out);
            let mut header = tar::Header::new_gnu();
            header.set_size(3);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, "other.json", &b"abc"[..]).unwrap();
            builder.finish().unwrap();
        }
        fs::write(paths::huazhong_path(&root), &out).unwrap();
        assert!(load(&root).days.is_empty());
        assert_eq!(fs::read(paths::huazhong_path(&root)).unwrap(), out);

        // tar + huazhong.json，但 JSON 坏掉
        let mut out2 = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut out2);
            let mut header = tar::Header::new_gnu();
            let bad = b"{ this is not json";
            header.set_size(bad.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, "huazhong.json", &bad[..]).unwrap();
            builder.finish().unwrap();
        }
        fs::write(paths::huazhong_path(&root), &out2).unwrap();
        assert!(load(&root).days.is_empty());
        assert_eq!(load(&root).settings, PomodoroSettings::default());
        assert_eq!(fs::read(paths::huazhong_path(&root)).unwrap(), out2);
    }

    #[test]
    fn record_persists_sorted_and_reloads() {
        let root = test_root("record");
        record(&root, 1500, POMODORO_MODE_STANDARD).unwrap();
        record(&root, 600, POMODORO_MODE_FORWARD).unwrap();
        record(&root, 900, "whatever").unwrap();

        let data = load(&root);
        assert_eq!(data.days.len(), 1);
        let day = &data.days[0];
        assert_eq!(day.sessions.len(), 3);
        assert_eq!(day.standard_count, 2, "未知模式按标准计");
        assert_eq!(day.forward_count, 1);
        assert_eq!(day.total_count(), 3);

        let (today, _) = now_local();
        assert_eq!(day.date, today);
        for s in &day.sessions {
            assert_eq!(s.start.len(), 8);
            assert_eq!(s.end.len(), 8);
            assert_eq!(s.at, s.end[..5]);
            assert!(!s.id.is_empty());
        }
        // start = end − seconds
        let first = &day.sessions[0];
        let minutes = (parse_clock(&first.end).unwrap() - parse_clock(&first.start).unwrap())
            .rem_euclid(SECONDS_PER_DAY);
        assert_eq!(minutes, 1500);
    }

    #[test]
    fn record_rejects_invalid_seconds() {
        let root = test_root("invalid_seconds");
        assert!(record(&root, 0, POMODORO_MODE_STANDARD).is_err());
        assert!(record(&root, MAX_SESSION_SECONDS + 1, POMODORO_MODE_STANDARD).is_err());
        assert!(!paths::huazhong_path(&root).exists(), "非法秒数不应落盘");
        assert!(record(&root, MAX_SESSION_SECONDS, POMODORO_MODE_STANDARD).is_ok());
    }

    #[test]
    fn union_merges_overlap_adjacent_and_cross_midnight() {
        // 重叠：09:00-10:00 + 09:30-10:30 → 1.5h
        let d = day_with(
            "2026-02-10",
            vec![
                session("a", "09:00:00", "10:00:00", POMODORO_MODE_STANDARD),
                session("b", "09:30:00", "10:30:00", POMODORO_MODE_STANDARD),
            ],
        );
        assert_eq!(d.standard_seconds, 5400);
        assert_eq!(d.total_seconds(), 5400);

        // 相邻：09:00-10:00 + 10:00-11:00 → 2h
        let d = day_with(
            "2026-02-10",
            vec![
                session("a", "09:00:00", "10:00:00", POMODORO_MODE_STANDARD),
                session("b", "10:00:00", "11:00:00", POMODORO_MODE_STANDARD),
            ],
        );
        assert_eq!(d.standard_seconds, 7200);

        // 跨午夜：23:30-00:15 → 裁剪到当天 [0, 900] = 15min
        let d = day_with(
            "2026-02-10",
            vec![session("a", "23:30:00", "00:15:00", POMODORO_MODE_FORWARD)],
        );
        assert_eq!(d.forward_seconds, 900);

        // 完全不重叠：两段相加
        let d = day_with(
            "2026-02-10",
            vec![
                session("a", "09:00:00", "09:25:00", POMODORO_MODE_STANDARD),
                session("b", "10:00:00", "10:25:00", POMODORO_MODE_STANDARD),
            ],
        );
        assert_eq!(d.standard_seconds, 3000);
    }

    #[test]
    fn cross_mode_overlap_counts_once_in_total() {
        let mut d = day_with(
            "2026-02-10",
            vec![
                session("a", "09:00:00", "10:00:00", POMODORO_MODE_STANDARD),
                session("b", "09:30:00", "10:30:00", POMODORO_MODE_FORWARD),
            ],
        );
        // 分模式缓存各自算，跨模式重叠不在此体现
        assert_eq!(d.standard_seconds, 3600);
        assert_eq!(d.forward_seconds, 3600);
        // 当天总计是整体并集：09:00-10:30（重叠的半小时只算一次）
        assert_eq!(d.total_seconds(), 5400);
        // 次数按原始条数，两边同时计时就是 2 个
        assert_eq!(d.standard_count, 1);
        assert_eq!(d.forward_count, 1);
        assert_eq!(d.total_count(), 2);

        d.baseline.standard_seconds = 600;
        assert_eq!(d.total_seconds(), 6000);
    }

    #[test]
    fn derived_caches_in_file_are_ignored_and_recomputed() {
        let value = serde_json::json!({
            "format": 2,
            "days": [{
                "date": "2026-02-10",
                "standard_count": 99,
                "forward_count": 99,
                "standard_seconds": 99999,
                "forward_seconds": 99999,
                "sessions": [{"id": "a", "at": "09:15", "seconds": 1500,
                              "mode": "standard", "start": "09:00:00", "end": "09:25:00"}]
            }]
        });
        let data = data_from_value(value);
        let day = &data.days[0];
        assert_eq!(day.standard_count, 1);
        assert_eq!(day.standard_seconds, 1500);
        assert_eq!(day.forward_count, 0);
        assert_eq!(day.forward_seconds, 0);
    }

    #[test]
    fn duplicate_ids_keep_first_and_duplicate_days_merge() {
        let value = serde_json::json!({
            "format": 2,
            "days": [
                {"date": "2026-02-10", "sessions": [
                    {"id": "same", "seconds": 60, "mode": "standard", "start": "09:00:00", "end": "09:01:00"},
                    {"id": "same", "seconds": 60, "mode": "standard", "start": "10:00:00", "end": "10:01:00"}
                ]},
                {"date": "2026-02-10", "sessions": [
                    {"id": "other", "seconds": 60, "mode": "forward", "start": "11:00:00", "end": "11:01:00"}
                ]}
            ]
        });
        let data = data_from_value(value);
        assert_eq!(data.days.len(), 1);
        let day = &data.days[0];
        assert_eq!(day.sessions.len(), 2);
        assert_eq!(day.total_count(), 2);
        assert_eq!(day.sessions[0].id, "same");
        assert_eq!(day.sessions[0].start, "09:00:00");
    }

    #[test]
    fn legacy_sessions_get_ids_and_times() {
        let value = serde_json::json!({
            "days": [{
                "date": "2026-02-10",
                "sessions": [{"at": "09:15", "seconds": 1500, "mode": "forward"}]
            }]
        });
        let data = data_from_value(value);
        let s = &data.days[0].sessions[0];
        assert!(!s.id.is_empty());
        assert_eq!(s.at, "09:15");
        assert_eq!(s.end, "09:15:00");
        assert_eq!(s.start, "08:50:00");
        assert_eq!(s.mode, POMODORO_MODE_FORWARD);
        assert_eq!(data.days[0].forward_seconds, 1500);
    }

    #[test]
    fn legacy_day_without_sessions_becomes_baseline() {
        let value = serde_json::json!({
            "days": [{
                "date": "2026-02-10",
                "standard_seconds": 1500,
                "standard_count": 1,
                "forward_seconds": 600,
                "forward_count": 1
            }]
        });
        let data = data_from_value(value);
        let day = &data.days[0];
        assert!(day.sessions.is_empty());
        assert_eq!(day.baseline.standard_seconds, 1500);
        assert_eq!(day.baseline.forward_seconds, 600);
        assert_eq!(day.standard_seconds, 1500);
        assert_eq!(day.forward_seconds, 600);
        assert_eq!(day.total_seconds(), 2100);
        assert_eq!(day.total_count(), 2);
    }

    #[test]
    fn invalid_sessions_are_dropped_on_load() {
        let value = serde_json::json!({
            "days": [{
                "date": "2026-02-10",
                "sessions": [
                    {"id": "zero", "seconds": 0, "mode": "standard", "start": "09:00:00", "end": "09:00:00"},
                    {"id": "huge", "seconds": 86401, "mode": "standard", "start": "09:00:00", "end": "09:00:01"},
                    {"id": "ok", "seconds": 60, "mode": "standard", "start": "09:00:00", "end": "09:01:00"},
                    {"id": "missing", "mode": "standard", "start": "09:00:00", "end": "09:01:00"}
                ]
            }]
        });
        let data = data_from_value(value);
        let day = &data.days[0];
        assert_eq!(day.sessions.len(), 1);
        assert_eq!(day.sessions[0].id, "ok");
        assert_eq!(day.standard_count, 1);
    }

    #[test]
    fn empty_intervals_are_dropped_from_union() {
        // start == end（含 24h 的退化情形）→ 空区间，丢弃
        let d = day_with(
            "2026-02-10",
            vec![session("a", "09:00:00", "09:00:00", POMODORO_MODE_STANDARD)],
        );
        assert_eq!(d.standard_seconds, 0);
        assert_eq!(d.total_seconds(), 0);
        // 但次数仍按条数计
        assert_eq!(d.total_count(), 1);
    }

    #[test]
    fn delete_session_removes_and_recomputes() {
        let root = test_root("delete");
        let mut data = HuazhongData::default();
        data.days.push(day_with(
            "2026-02-10",
            vec![
                session("a", "09:00:00", "09:25:00", POMODORO_MODE_STANDARD),
                session("b", "10:00:00", "10:25:00", POMODORO_MODE_STANDARD),
            ],
        ));
        save(&root, &data).unwrap();

        let after = delete_session(&root, "2026-02-10", "a").unwrap();
        let day = &after.days[0];
        assert_eq!(day.sessions.len(), 1);
        assert_eq!(day.sessions[0].id, "b");
        assert_eq!(day.standard_count, 1);
        assert_eq!(day.standard_seconds, 1500);
        assert_eq!(load(&root), after);

        // 删光明细后整天消失
        let after = delete_session(&root, "2026-02-10", "b").unwrap();
        assert!(after.days.is_empty());

        // 找不到记录
        assert!(delete_session(&root, "2026-02-10", "b").is_err());
        assert!(delete_session(&root, "2026-02-11", "a").is_err());
    }

    #[test]
    fn shorten_updates_start_and_rejects_lengthen() {
        let root = test_root("shorten");
        let mut data = HuazhongData::default();
        data.days.push(day_with(
            "2026-02-10",
            vec![session("a", "09:00:00", "09:25:00", POMODORO_MODE_STANDARD)],
        ));
        save(&root, &data).unwrap();

        // 加长 / 0 / 超出原值都拒绝，且不落盘
        let before = fs::read(paths::huazhong_path(&root)).unwrap();
        assert_eq!(
            shorten_session(&root, "2026-02-10", "a", 1501).unwrap_err(),
            "只能减短，不能加长"
        );
        assert_eq!(
            shorten_session(&root, "2026-02-10", "a", 0).unwrap_err(),
            "只能减短，不能加长"
        );
        assert_eq!(
            shorten_session(&root, "2026-02-10", "a", 99999).unwrap_err(),
            "只能减短，不能加长"
        );
        assert_eq!(fs::read(paths::huazhong_path(&root)).unwrap(), before);

        // 等长允许（无操作）
        assert!(shorten_session(&root, "2026-02-10", "a", 1500).is_ok());

        let after = shorten_session(&root, "2026-02-10", "a", 600).unwrap();
        let s = &after.days[0].sessions[0];
        assert_eq!(s.seconds, 600);
        assert_eq!(s.end, "09:25:00", "end 不变");
        assert_eq!(s.start, "09:15:00", "start = end − seconds");
        assert_eq!(s.mode, POMODORO_MODE_STANDARD);
        assert_eq!(after.days[0].standard_seconds, 600);

        assert!(shorten_session(&root, "2026-02-11", "a", 60).is_err());
        assert!(shorten_session(&root, "2026-02-10", "nope", 60).is_err());
    }

    #[test]
    fn set_settings_clamps_and_persists() {
        let root = test_root("settings");
        let data = set_settings(
            &root,
            PomodoroSettings {
                mode: "nonsense".into(),
                focus_minutes: 0,
                break_minutes: 999,
                loops: 0,
            },
        )
        .unwrap();
        assert_eq!(data.settings.mode, POMODORO_MODE_STANDARD);
        assert_eq!(data.settings.focus_minutes, MIN_FOCUS_MINUTES);
        assert_eq!(data.settings.break_minutes, MAX_BREAK_MINUTES);
        assert_eq!(data.settings.loops, MIN_LOOPS);
        assert_eq!(load(&root).settings, data.settings);
    }

    #[test]
    fn month_filter_matches_only_that_month() {
        let mut data = HuazhongData::default();
        for (date, mode) in [
            ("2025-12-31", POMODORO_MODE_STANDARD),
            ("2026-01-01", POMODORO_MODE_STANDARD),
            ("2026-01-31", POMODORO_MODE_FORWARD),
            ("2026-02-01", POMODORO_MODE_STANDARD),
            ("2027-01-01", POMODORO_MODE_STANDARD),
        ] {
            data.days.push(day_with(
                date,
                vec![session("a", "09:00:00", "09:10:00", mode)],
            ));
        }
        data.days.sort_by(|a, b| a.date.cmp(&b.date));

        let jan = data.month(2026, 1);
        assert_eq!(jan.len(), 2);
        assert_eq!(jan[0].date, "2026-01-01");
        assert_eq!(jan[1].date, "2026-01-31");
        assert_eq!(jan[1].forward_count, 1);
        assert_eq!(data.month(2026, 2).len(), 1);
        assert_eq!(data.month(2026, 3).len(), 0);
        assert_eq!(data.month(2025, 12).len(), 1);
    }

    #[test]
    fn now_local_has_expected_shape() {
        let (date, time) = now_local();
        assert!(is_date(&date), "{date}");
        assert_eq!(time.len(), 8);
        assert_eq!(time.as_bytes()[2], b':');
        assert_eq!(time.as_bytes()[5], b':');
    }

    #[test]
    fn concurrent_records_are_not_lost() {
        let root = test_root("concurrent");
        let r1 = root.clone();
        let r2 = root.clone();
        let t1 = std::thread::spawn(move || record(&r1, 1500, POMODORO_MODE_STANDARD));
        let t2 = std::thread::spawn(move || record(&r2, 600, POMODORO_MODE_FORWARD));
        t1.join().unwrap().unwrap();
        t2.join().unwrap().unwrap();

        let data = load(&root);
        assert_eq!(data.days.len(), 1);
        let day = &data.days[0];
        assert_eq!(day.sessions.len(), 2, "两次追加都不能丢");
        assert_eq!(day.total_count(), 2);
        let ids: HashSet<&str> = day.sessions.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids.len(), 2);
        assert!(!paths::huazhong_lock_path(&root).exists());
        assert!(!paths::huazhong_tmp_path(&root).exists());
    }

    #[test]
    fn stale_lock_is_taken_over() {
        let root = test_root("stale_lock");
        let lock_path = paths::huazhong_lock_path(&root);
        fs::write(&lock_path, b"stale").unwrap();
        let stale = SystemTime::now() - Duration::from_secs(30);
        fs::File::options()
            .write(true)
            .open(&lock_path)
            .unwrap()
            .set_modified(stale)
            .unwrap();

        record(&root, 60, POMODORO_MODE_STANDARD).unwrap();
        assert_eq!(load(&root).days[0].sessions.len(), 1);
        assert!(!lock_path.exists());
    }

    #[test]
    fn fresh_lock_times_out_with_message() {
        let root = test_root("busy_lock");
        fs::write(paths::huazhong_lock_path(&root), b"held").unwrap();

        let err = record(&root, 60, POMODORO_MODE_STANDARD).unwrap_err();
        assert_eq!(err, "数据文件正被占用，请稍后重试");
        assert!(paths::huazhong_lock_path(&root).exists(), "别人的锁不能被删掉");
    }
}