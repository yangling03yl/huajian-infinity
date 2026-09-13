// 番茄钟纯逻辑：不依赖 Svelte / Tauri，便于用 scripts/test-pomodoro.mjs 直接验证
import type { PomodoroMode, PomodoroSettings } from './types';

/** 短于该时长的番茄不计入统计（正向计时防误触） */
export const MIN_RECORD_SECONDS = 60;
/** 正向计时单次上限（8 小时），避免忘记结束导致计时无限增长 */
export const MAX_FORWARD_SECONDS = 8 * 60 * 60;
/** 设置项范围，与后端 settings.rs 中的常量保持一致 */
export const FOCUS_RANGE = { min: 1, max: 180 } as const;
export const BREAK_RANGE = { min: 0, max: 60 } as const;
export const LOOPS_RANGE = { min: 1, max: 12 } as const;

/** 追帧步数硬上限：休息 0 分钟等极端配置下的死循环防护 */
export const MAX_CATCHUP_STEPS = 1000;

export type PomodoroPhase = 'idle' | 'focus' | 'break';

export interface PomodoroState {
  phase: PomodoroPhase;
  paused: boolean;
  /** 当前设置：运行中修改只影响后续阶段，不打断正在进行的阶段 */
  settings: PomodoroSettings;
  /** 本轮已完成的番茄个数 */
  completed: number;
  /** 当前阶段锚点（epoch ms）；未开始或暂停时为 null */
  startedAt: number | null;
  /** 倒计时阶段终点（epoch ms）；正向计时与暂停时为 null */
  endsAt: number | null;
  /** 当前阶段总时长（秒），用于进度与显示 */
  phaseDuration: number;
  /** 暂停前当前阶段已进行的秒数 */
  pausedElapsed: number;
  /** 本轮会话是否已经跑完 */
  finished: boolean;
}

export interface SessionRecord {
  seconds: number;
  mode: PomodoroMode;
}

export interface AdvanceResult {
  state: PomodoroState;
  /** 本次需要写入统计的番茄 */
  records: SessionRecord[];
  /** 阶段是否发生变化 */
  phaseChanged: boolean;
  /** 本次刚好跑完最后一个番茄 */
  justFinished: boolean;
}

/** 状态栏提示与提示音次数 */
export interface Announcement {
  text: string;
  beeps: number;
}

/** 与后端 PomodoroSettings::default() 保持一致 */
export const DEFAULT_SETTINGS: PomodoroSettings = {
  mode: 'standard',
  focus_minutes: 25,
  break_minutes: 5,
  loops: 4,
};

/** 夹取设置到合法范围，非法值回落到默认值 */
export function clampSettings(settings: PomodoroSettings): PomodoroSettings {
  const clamp = (value: number, min: number, max: number, fallback: number): number => {
    const n = Math.round(Number(value));
    if (!Number.isFinite(n)) return fallback;
    return Math.min(max, Math.max(min, n));
  };
  return {
    mode: settings.mode === 'forward' ? 'forward' : 'standard',
    focus_minutes: clamp(
      settings.focus_minutes,
      FOCUS_RANGE.min,
      FOCUS_RANGE.max,
      DEFAULT_SETTINGS.focus_minutes,
    ),
    break_minutes: clamp(
      settings.break_minutes,
      BREAK_RANGE.min,
      BREAK_RANGE.max,
      DEFAULT_SETTINGS.break_minutes,
    ),
    loops: clamp(settings.loops, LOOPS_RANGE.min, LOOPS_RANGE.max, DEFAULT_SETTINGS.loops),
  };
}

export function idleState(
  settings: PomodoroSettings,
  completed = 0,
  finished = false,
): PomodoroState {
  return {
    phase: 'idle',
    paused: false,
    settings,
    completed,
    startedAt: null,
    endsAt: null,
    phaseDuration: 0,
    pausedElapsed: 0,
    finished,
  };
}

function startFocus(state: PomodoroState, at: number, resetCompleted: boolean): PomodoroState {
  const forward = state.settings.mode === 'forward';
  const seconds = state.settings.focus_minutes * 60;
  return {
    ...state,
    phase: 'focus',
    paused: false,
    completed: resetCompleted ? 0 : state.completed,
    startedAt: at,
    endsAt: forward ? null : at + seconds * 1000,
    phaseDuration: seconds,
    pausedElapsed: 0,
    finished: false,
  };
}

function startBreak(state: PomodoroState, at: number): PomodoroState {
  const seconds = state.settings.break_minutes * 60;
  return {
    ...state,
    phase: 'break',
    paused: false,
    startedAt: at,
    endsAt: at + seconds * 1000,
    phaseDuration: seconds,
    pausedElapsed: 0,
  };
}

/** 结算当前专注阶段：达标时计入统计，并推进循环（休息或结束） */
function completeFocus(
  state: PomodoroState,
  at: number,
  seconds: number,
): { state: PomodoroState; record: SessionRecord | null; justFinished: boolean } {
  const record: SessionRecord | null =
    seconds >= MIN_RECORD_SECONDS ? { seconds, mode: state.settings.mode } : null;
  const completed = state.completed + 1;
  if (completed >= state.settings.loops) {
    return { state: idleState(state.settings, completed, true), record, justFinished: true };
  }
  const progressed = { ...state, completed };
  const next =
    state.settings.break_minutes > 0
      ? startBreak(progressed, at)
      : startFocus(progressed, at, false);
  return { state: next, record, justFinished: false };
}

/** 已进行的秒数（正向计时无上限；倒计时阶段不超过阶段总时长） */
export function elapsedSeconds(state: PomodoroState, now: number): number {
  if (state.phase === 'idle') return 0;
  if (state.paused) return state.pausedElapsed;
  const started = state.startedAt ?? now;
  const raw = Math.max(0, (now - started) / 1000);
  return state.endsAt === null ? raw : Math.min(raw, state.phaseDuration);
}

/** 剩余秒数；正向计时返回 null */
export function remainingSeconds(state: PomodoroState, now: number): number | null {
  if (state.phase === 'idle' || state.endsAt === null) return null;
  if (state.paused) return Math.max(0, state.phaseDuration - state.pausedElapsed);
  return Math.max(0, (state.endsAt - now) / 1000);
}

export function phaseProgress(state: PomodoroState, now: number): number {
  if (state.phase === 'idle' || state.phaseDuration <= 0) return 0;
  return Math.min(1, Math.max(0, elapsedSeconds(state, now) / state.phaseDuration));
}

export function phaseLabel(state: PomodoroState): string {
  if (state.phase === 'break') return '休息中';
  if (state.phase === 'focus') return state.settings.mode === 'forward' ? '正向计时' : '专注中';
  return state.finished ? '本轮已完成' : '待开始';
}

export function formatClock(totalSeconds: number): string {
  const s = Math.max(0, Math.ceil(totalSeconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const pad = (n: number): string => String(n).padStart(2, '0');
  return h > 0 ? `${h}:${pad(m)}:${pad(sec)}` : `${pad(m)}:${pad(sec)}`;
}

/**
 * 根据推进结果生成状态栏提示与提示音次数。
 * 注意必须按推进后的阶段判断：正向计时不足 1 分钟时同样会进入休息，
 * 但那是「番茄结束」而不是「休息结束」。
 */
export function announcement(result: AdvanceResult): Announcement | null {
  const st = result.state;
  if (result.justFinished) {
    return { text: `本轮番茄钟已完成（${st.completed}/${st.settings.loops}）`, beeps: 3 };
  }
  if (result.records.length > 0) {
    return {
      text:
        st.phase === 'break'
          ? `番茄完成，休息 ${st.settings.break_minutes} 分钟`
          : `番茄完成，开始第 ${st.completed + 1}/${st.settings.loops} 个`,
      beeps: 2,
    };
  }
  if (!result.phaseChanged) return null;
  if (st.phase === 'break') {
    return { text: '本次番茄不足 1 分钟，未计入统计', beeps: 1 };
  }
  if (st.phase === 'focus') {
    return {
      text: `休息结束，开始第 ${st.completed + 1}/${st.settings.loops} 个番茄`,
      beeps: 1,
    };
  }
  return null;
}

/**
 * 按墙钟推进状态：把所有已经越过的阶段依次补推（应用被最小化时定时器会被降频，
 * 例如休息早已结束但界面还停在休息中）。循环次数有硬上限，休息为 0 分钟时也不会死循环。
 */
export function advance(state: PomodoroState, now: number): AdvanceResult {
  let next = state;
  const records: SessionRecord[] = [];
  let phaseChanged = false;
  let justFinished = false;

  for (let step = 0; step < MAX_CATCHUP_STEPS; step += 1) {
    if (next.phase === 'idle' || next.paused) break;

    if (next.endsAt === null) {
      // 正向计时：只在超过封顶时长时自动结算
      if (next.phase === 'focus' && elapsedSeconds(next, now) >= MAX_FORWARD_SECONDS) {
        const res = completeFocus(next, now, MAX_FORWARD_SECONDS);
        if (res.record) records.push(res.record);
        next = res.state;
        phaseChanged = true;
        justFinished = res.justFinished;
      }
      break;
    }

    if (now < next.endsAt) break;

    if (next.phase === 'break') {
      // 休息结束，紧接着开始下一个番茄（时间轴锚在上一个阶段终点，补推后时间依然准确）
      next = startFocus(next, next.endsAt, false);
      phaseChanged = true;
      continue;
    }

    const res = completeFocus(next, next.endsAt, next.phaseDuration);
    if (res.record) records.push(res.record);
    next = res.state;
    phaseChanged = true;
    justFinished = res.justFinished;
    if (res.justFinished) break;
  }

  return { state: next, records, phaseChanged, justFinished };
}

export function startSession(state: PomodoroState, now: number): PomodoroState {
  if (state.phase !== 'idle') return state;
  return startFocus(state, now, true);
}

export function pauseSession(state: PomodoroState, now: number): PomodoroState {
  if (state.phase === 'idle' || state.paused) return state;
  return {
    ...state,
    paused: true,
    pausedElapsed: elapsedSeconds(state, now),
    startedAt: null,
    endsAt: null,
  };
}

export function resumeSession(state: PomodoroState, now: number): PomodoroState {
  if (state.phase === 'idle' || !state.paused) return state;
  const startedAt = now - state.pausedElapsed * 1000;
  const forward = state.phase === 'focus' && state.settings.mode === 'forward';
  return {
    ...state,
    paused: false,
    startedAt,
    endsAt: forward ? null : startedAt + state.phaseDuration * 1000,
    pausedElapsed: 0,
  };
}

/** 重置当前阶段（重新计时），不计入统计、不改变已完成个数 */
export function resetPhase(state: PomodoroState, now: number): PomodoroState {
  if (state.phase === 'idle') return state;
  return state.phase === 'focus' ? startFocus(state, now, false) : startBreak(state, now);
}

export function skipBreak(state: PomodoroState, now: number): PomodoroState {
  if (state.phase !== 'break') return state;
  return startFocus(state, now, false);
}

export function stopSession(state: PomodoroState): PomodoroState {
  if (state.phase === 'idle' && !state.finished) return state;
  return idleState(state.settings);
}

/** 正向计时：手动结束当前番茄，按实际时长记录 */
export function finishForward(state: PomodoroState, now: number): AdvanceResult {
  if (state.phase !== 'focus' || state.paused || state.settings.mode !== 'forward') {
    return { state, records: [], phaseChanged: false, justFinished: false };
  }
  const seconds = Math.min(Math.round(elapsedSeconds(state, now)), MAX_FORWARD_SECONDS);
  const res = completeFocus(state, now, seconds);
  return {
    state: res.state,
    records: res.record ? [res.record] : [],
    phaseChanged: true,
    justFinished: res.justFinished,
  };
}
