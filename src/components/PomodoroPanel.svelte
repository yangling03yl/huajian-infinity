<script lang="ts">
  import {
    pomodoro,
    pomodoroNow,
    phaseLabel,
    phaseProgress,
    remainingSeconds,
    elapsedSeconds,
    formatClock,
    startPomodoro,
    pausePomodoro,
    resumePomodoro,
    resetPomodoroPhase,
    skipPomodoroBreak,
    stopPomodoro,
    finishForwardTomato,
    updatePomodoroSettings,
    FOCUS_RANGE,
    BREAK_RANGE,
    LOOPS_RANGE,
  } from '../lib/pomodoro';
  import type { PomodoroSettings } from '../lib/types';

  interface Props {
    onClose: () => void;
    onOpenStats: () => void;
  }
  let { onClose, onOpenStats }: Props = $props();

  let focusInput = $state('25');
  let breakInput = $state('5');
  let loopsInput = $state('4');

  const pomo = $derived($pomodoro);
  const now = $derived($pomodoroNow);
  const running = $derived(pomo.phase !== 'idle');

  // 只在设置对象真正变化时回填输入框，避免阶段切换时打断正在输入的内容
  let lastSettings: PomodoroSettings | null = null;
  $effect(() => {
    const s = pomo.settings;
    if (s === lastSettings) return;
    lastSettings = s;
    focusInput = String(s.focus_minutes);
    breakInput = String(s.break_minutes);
    loopsInput = String(s.loops);
  });

  const clock = $derived.by(() => {
    const remain = remainingSeconds(pomo, now);
    if (remain !== null) return formatClock(remain);
    if (pomo.phase === 'focus') return formatClock(elapsedSeconds(pomo, now));
    return formatClock(pomo.settings.focus_minutes * 60);
  });

  const progress = $derived(pomo.endsAt === null ? null : phaseProgress(pomo, now));

  const statusLine = $derived.by(() => {
    if (pomo.phase === 'idle') {
      return pomo.finished
        ? `${pomo.completed}/${pomo.settings.loops} 个番茄`
        : `${pomo.settings.focus_minutes} 分钟 · ${pomo.settings.loops} 个番茄`;
    }
    if (pomo.phase === 'break') {
      return `第 ${pomo.completed}/${pomo.settings.loops} 个已完成 · 休息 ${pomo.settings.break_minutes} 分钟`;
    }
    return `第 ${pomo.completed + 1}/${pomo.settings.loops} 个 · ${pomo.settings.mode === 'forward' ? '正向计时' : '倒计时'}`;
  });

  function commit(which: 'focus' | 'break' | 'loops', raw: string): void {
    const parsed = Number.parseInt(raw, 10);
    if (!Number.isFinite(parsed)) {
      focusInput = String(pomo.settings.focus_minutes);
      breakInput = String(pomo.settings.break_minutes);
      loopsInput = String(pomo.settings.loops);
      return;
    }
    const next = updatePomodoroSettings(
      which === 'focus'
        ? { focus_minutes: parsed }
        : which === 'break'
          ? { break_minutes: parsed }
          : { loops: parsed },
    );
    focusInput = String(next.focus_minutes);
    breakInput = String(next.break_minutes);
    loopsInput = String(next.loops);
  }
</script>

<div class="panel" role="dialog" aria-label="番茄钟">
  <div class="head">
    <span class="title">🍅 番茄钟</span>
    <button class="icon-btn" title="关闭" onclick={onClose}>✕</button>
  </div>

  <div class="display">
    <div class="clock {pomo.phase}">{clock}</div>
    <div class="status">{phaseLabel(pomo)} · {statusLine}</div>
    <div class="bar">
      <i class:rest={pomo.phase === 'break'} style="width:{Math.round((progress ?? 0) * 100)}%"></i>
    </div>
    <div class="dots">
      {#each Array(pomo.settings.loops) as _, i}
        <span
          class="dot"
          class:done={i < pomo.completed}
          class:active={i === pomo.completed && pomo.phase === 'focus'}
        ></span>
      {/each}
    </div>
  </div>

  <div class="controls">
    {#if pomo.phase === 'idle'}
      <button class="primary-btn grow" onclick={startPomodoro}>
        {pomo.finished ? '再来一轮' : '开始'}
      </button>
    {:else}
      <button class="primary-btn grow" onclick={pomo.paused ? resumePomodoro : pausePomodoro}>
        {pomo.paused ? '继续' : '暂停'}
      </button>
      {#if pomo.phase === 'focus' && pomo.settings.mode === 'forward'}
        <button class="ghost-btn" onclick={finishForwardTomato}>完成番茄</button>
      {/if}
      {#if pomo.phase === 'break'}
        <button class="ghost-btn" onclick={skipPomodoroBreak}>跳过休息</button>
      {/if}
      <button class="ghost-btn" onclick={resetPomodoroPhase}>重置</button>
      <button class="ghost-btn" onclick={stopPomodoro}>停止</button>
    {/if}
  </div>

  <div class="sep"></div>

  <div class="field">
    <span class="k">模式</span>
    <div class="seg">
      <button
        class:active={pomo.settings.mode === 'standard'}
        disabled={running}
        title={running ? '请先停止当前番茄' : '标准：倒计时，结束后自动进入休息'}
        onclick={() => updatePomodoroSettings({ mode: 'standard' })}
      >
        标准
      </button>
      <button
        class:active={pomo.settings.mode === 'forward'}
        disabled={running}
        title={running ? '请先停止当前番茄' : '正向：正计时，手动点「完成番茄」'}
        onclick={() => updatePomodoroSettings({ mode: 'forward' })}
      >
        正向
      </button>
    </div>
  </div>
  <div class="hint">
    {running
      ? '运行中不能切换模式；时长与循环的修改从下一个阶段生效'
      : '标准＝倒计时自动进入休息；正向＝正计时手动完成'}
  </div>

  <div class="field">
    <span class="k">番茄时长</span>
    <span class="num">
      <input
        type="number"
        min={FOCUS_RANGE.min}
        max={FOCUS_RANGE.max}
        value={focusInput}
        title="单个番茄的专注时长（{FOCUS_RANGE.min}-{FOCUS_RANGE.max} 分钟）"
        onchange={(e) => commit('focus', e.currentTarget.value)}
      />
      <em>分钟</em>
    </span>
  </div>

  <div class="field">
    <span class="k">休息时长</span>
    <span class="num">
      <input
        type="number"
        min={BREAK_RANGE.min}
        max={BREAK_RANGE.max}
        value={breakInput}
        title="循环内相邻两个番茄之间的休息时长（{BREAK_RANGE.min}-{BREAK_RANGE.max} 分钟）"
        onchange={(e) => commit('break', e.currentTarget.value)}
      />
      <em>分钟</em>
    </span>
  </div>

  <div class="field">
    <span class="k">循环次数</span>
    <span class="num">
      <input
        type="number"
        min={LOOPS_RANGE.min}
        max={LOOPS_RANGE.max}
        value={loopsInput}
        title="一轮会话包含的番茄个数（{LOOPS_RANGE.min}-{LOOPS_RANGE.max}）"
        onchange={(e) => commit('loops', e.currentTarget.value)}
      />
      <em>个</em>
    </span>
  </div>

  <div class="foot">
    <button class="ghost-btn" onclick={onOpenStats}>番茄记录</button>
  </div>
</div>

<style>
  .panel {
    position: fixed;
    right: 10px;
    bottom: 34px;
    z-index: 70;
    width: 274px;
    max-height: calc(100vh - 60px);
    overflow: auto;
    padding: 12px;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.22);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .title {
    font-size: 12px;
    color: var(--text-dim);
  }
  .display {
    text-align: center;
    padding: 2px 0 8px;
  }
  .clock {
    font-size: 38px;
    line-height: 1.15;
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }
  /* 专注用强调色；休息只降级为次要文字色，避免竹青主题下与强调色同色 */
  .clock.focus {
    color: var(--accent);
  }
  .clock.break {
    color: var(--text-dim);
  }
  .status {
    font-size: 12px;
    color: var(--text-dim);
    margin-top: 2px;
  }
  .bar {
    height: 3px;
    border-radius: 2px;
    background: var(--bg-active);
    margin: 9px 2px 8px;
    overflow: hidden;
  }
  .bar i {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 0.25s linear;
  }
  .bar i.rest {
    background: var(--text-dim);
  }
  .dots {
    display: flex;
    justify-content: center;
    gap: 5px;
  }
  /* 未开始的圆点不能只用 --border（浅色/深色主题下几乎与底色同色而看不见） */
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-dim);
    opacity: 0.35;
  }
  .dot.done {
    background: var(--accent);
    opacity: 1;
  }
  .dot.active {
    background: var(--accent);
    opacity: 0.5;
  }
  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 6px;
  }
  .controls :global(.primary-btn),
  .controls :global(.ghost-btn) {
    padding: 5px 10px;
    font-size: 13px;
    border-radius: 7px;
  }
  /* 深色主题里强调色偏亮，白字对比度不足；用主题底色作前景在明暗两种模式下都清晰 */
  .controls :global(.primary-btn) {
    color: var(--bg);
  }
  .grow {
    flex: 1;
    justify-content: center;
  }
  .sep {
    height: 1px;
    background: var(--border);
    margin: 10px 0 6px;
  }
  .field {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 4px 0;
  }
  .k {
    font-size: 12px;
    color: var(--text-dim);
  }
  .num {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .num input {
    width: 58px;
    padding: 3px 6px;
    text-align: right;
    font-variant-numeric: tabular-nums;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    outline: none;
  }
  .num input:focus {
    border-color: var(--accent);
  }
  .num em {
    font-style: normal;
    font-size: 11px;
    color: var(--text-dim);
  }
  .seg {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: 7px;
    overflow: hidden;
  }
  .seg button {
    padding: 4px 12px;
    font-size: 12px;
    color: var(--text-dim);
    background: var(--bg);
  }
  .seg button.active {
    background: var(--accent);
    color: var(--bg);
  }
  .seg button:disabled {
    cursor: not-allowed;
  }
  .seg button.active:disabled {
    opacity: 0.75;
  }
  .hint {
    font-size: 11px;
    line-height: 1.5;
    color: var(--text-dim);
    padding: 2px 0 4px;
  }
  .foot {
    display: flex;
    margin-top: 8px;
  }
  .foot :global(.ghost-btn) {
    flex: 1;
    justify-content: center;
    font-size: 12px;
    padding: 5px 10px;
  }
</style>
