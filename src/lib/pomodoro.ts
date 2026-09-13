// 番茄钟运行时：状态机纯逻辑在 pomodoro-core.ts，这里负责 store、定时刷新、统计写入与提示音
import { get, writable } from 'svelte/store';
import { api, errMsg } from './api';
import { statusMsg } from './stores';
import type { PomodoroSettings } from './types';
import {
  DEFAULT_SETTINGS,
  advance,
  announcement,
  clampSettings,
  finishForward,
  idleState,
  pauseSession,
  resetPhase,
  resumeSession,
  skipBreak,
  startSession,
  stopSession,
  type AdvanceResult,
  type SessionRecord,
} from './pomodoro-core';

export * from './pomodoro-core';

/** 刷新间隔（毫秒）。时间按墙钟计算，节流只会让刷新变稀，不会让时间漂移 */
const TICK_MS = 250;
/** 设置写盘防抖（毫秒），避免数字框连续改动时反复写文件 */
const PERSIST_DEBOUNCE_MS = 400;

export const pomodoro = writable(idleState(DEFAULT_SETTINGS));
/** 每 TICK_MS 更新一次，仅用于驱动时间显示 */
export const pomodoroNow = writable(Date.now());

let timer: ReturnType<typeof setInterval> | null = null;
let persistTimer: ReturnType<typeof setTimeout> | null = null;
let messageTimer: ReturnType<typeof setTimeout> | null = null;
let audioCtx: AudioContext | null = null;

/** 状态栏提示停留时长（毫秒），到点自动清空 */
const MESSAGE_MS = 8000;

/** 状态栏提示位只服务番茄钟：写入后自动淡出，避免旧提示长期占位 */
function notify(text: string): void {
  statusMsg.set(text);
  if (messageTimer !== null) clearTimeout(messageTimer);
  messageTimer = setTimeout(() => {
    messageTimer = null;
    statusMsg.set('');
  }, MESSAGE_MS);
}

function beep(times: number): void {
  try {
    audioCtx ??= new AudioContext();
    const ctx = audioCtx;
    if (ctx.state === 'suspended') void ctx.resume();
    const base = ctx.currentTime + 0.02;
    for (let i = 0; i < times; i += 1) {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = 'sine';
      osc.frequency.value = 720;
      osc.connect(gain);
      gain.connect(ctx.destination);
      const at = base + i * 0.18;
      gain.gain.setValueAtTime(0.0001, at);
      gain.gain.exponentialRampToValueAtTime(0.12, at + 0.02);
      gain.gain.exponentialRampToValueAtTime(0.0001, at + 0.16);
      osc.start(at);
      osc.stop(at + 0.18);
    }
  } catch {
    // 无音频设备时静默失败，不影响计时
  }
}

/** 在用户手势中解锁音频，保证阶段结束时的提示音能播放 */
function primeAudio(): void {
  try {
    audioCtx ??= new AudioContext();
    if (audioCtx.state === 'suspended') void audioCtx.resume();
  } catch {
    // 忽略
  }
}

async function recordSession(record: SessionRecord): Promise<void> {
  try {
    await api.recordPomodoro(Math.round(record.seconds), record.mode);
  } catch (e) {
    notify(`番茄统计写入失败: ${errMsg(e)}`);
  }
}

function announce(result: AdvanceResult): void {
  const info = announcement(result);
  if (info === null) return;
  beep(info.beeps);
  notify(info.text);
}

function applyResult(result: AdvanceResult): void {
  if (result.state !== get(pomodoro)) pomodoro.set(result.state);
  for (const record of result.records) void recordSession(record);
  announce(result);
}

function tick(): void {
  const current = get(pomodoro);
  if (current.phase === 'idle') return;
  const now = Date.now();
  pomodoroNow.set(now);
  applyResult(advance(current, now));
}

/** 启动计时刷新并从后端读取设置；返回清理函数 */
export function initPomodoro(): () => void {
  if (timer !== null) return () => {};
  void (async () => {
    try {
      const settings = await api.getSettings();
      const loaded = clampSettings(settings.pomodoro ?? DEFAULT_SETTINGS);
      pomodoro.update((cur) => (cur.phase === 'idle' ? { ...cur, settings: loaded } : cur));
    } catch (e) {
      notify(`读取番茄钟设置失败: ${errMsg(e)}`);
    }
  })();
  timer = setInterval(tick, TICK_MS);
  return () => {
    if (timer !== null) clearInterval(timer);
    timer = null;
  };
}

function schedulePersist(settings: PomodoroSettings): void {
  if (persistTimer !== null) clearTimeout(persistTimer);
  persistTimer = setTimeout(() => {
    persistTimer = null;
    void api
      .setPomodoroSettings(
        settings.mode,
        settings.focus_minutes,
        settings.break_minutes,
        settings.loops,
      )
      .catch((e) => notify(`保存番茄钟设置失败: ${errMsg(e)}`));
  }, PERSIST_DEBOUNCE_MS);
}

/** 修改设置：运行中改时长只影响后续阶段；运行中切换模式会被忽略 */
export function updatePomodoroSettings(patch: Partial<PomodoroSettings>): PomodoroSettings {
  const current = get(pomodoro);
  const merged = clampSettings({ ...current.settings, ...patch });
  const settings: PomodoroSettings = {
    ...merged,
    mode: current.phase === 'idle' ? merged.mode : current.settings.mode,
  };
  pomodoro.set({ ...current, settings });
  schedulePersist(settings);
  return settings;
}

export function startPomodoro(): void {
  primeAudio();
  pomodoro.set(startSession(get(pomodoro), Date.now()));
}

export function pausePomodoro(): void {
  pomodoro.set(pauseSession(get(pomodoro), Date.now()));
}

export function resumePomodoro(): void {
  primeAudio();
  pomodoro.set(resumeSession(get(pomodoro), Date.now()));
}

export function resetPomodoroPhase(): void {
  pomodoro.set(resetPhase(get(pomodoro), Date.now()));
}

export function skipPomodoroBreak(): void {
  pomodoro.set(skipBreak(get(pomodoro), Date.now()));
}

export function stopPomodoro(): void {
  pomodoro.set(stopSession(get(pomodoro)));
  notify('番茄钟已停止，本轮未完成的番茄不计入统计');
}

export function finishForwardTomato(): void {
  applyResult(finishForward(get(pomodoro), Date.now()));
}
