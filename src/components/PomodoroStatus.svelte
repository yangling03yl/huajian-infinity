<script lang="ts">
  import { onMount } from 'svelte';
  import {
    pomodoro,
    pomodoroNow,
    initPomodoro,
    phaseLabel,
    remainingSeconds,
    elapsedSeconds,
    formatClock,
  } from '../lib/pomodoro';
  import PomodoroPanel from './PomodoroPanel.svelte';
  import PomodoroStats from './PomodoroStats.svelte';

  let open = $state(false);
  let statsOpen = $state(false);

  onMount(() => initPomodoro());

  const pomo = $derived($pomodoro);
  const now = $derived($pomodoroNow);

  const kind = $derived(
    pomo.phase === 'break'
      ? 'break'
      : pomo.phase === 'focus'
        ? 'focus'
        : pomo.finished
          ? 'done'
          : 'idle',
  );

  const clock = $derived.by(() => {
    const remain = remainingSeconds(pomo, now);
    if (remain !== null) return formatClock(remain);
    if (pomo.phase === 'focus') return formatClock(elapsedSeconds(pomo, now));
    return formatClock(pomo.settings.focus_minutes * 60);
  });

  const title = $derived.by(() => {
    if (pomo.phase === 'idle') {
      return pomo.finished
        ? `本轮已完成 ${pomo.completed}/${pomo.settings.loops} 个番茄 · 点击打开番茄钟`
        : `番茄钟待开始（${pomo.settings.focus_minutes} 分钟 · ${pomo.settings.loops} 个）· 点击打开`;
    }
    const mode = pomo.settings.mode === 'forward' ? '正向计时' : '倒计时';
    const pos =
      pomo.phase === 'break'
        ? `第 ${pomo.completed}/${pomo.settings.loops} 个已完成`
        : `第 ${pomo.completed + 1}/${pomo.settings.loops} 个`;
    return `${phaseLabel(pomo)} · ${pos} · ${mode}${pomo.paused ? ' · 已暂停' : ''}`;
  });
</script>

<button class="chip {kind}" {title} onclick={() => (open = !open)}>
  {#if pomo.phase === 'idle' && pomo.finished}
    <span class="mark">✓</span>
    <span class="clock">{pomo.completed}/{pomo.settings.loops}</span>
  {:else}
    <span class="icon">{pomo.phase === 'break' ? '☕️' : '🍅'}</span>
    <span class="clock">{clock}</span>
    {#if pomo.phase === 'focus' && pomo.settings.mode === 'forward'}
      <span class="mark">↑</span>
    {/if}
  {/if}
  {#if pomo.paused}
    <span class="mark">⏸</span>
  {/if}
</button>

{#if open}
  <div class="panel-mask" role="button" tabindex="-1" onclick={() => (open = false)}></div>
  <PomodoroPanel
    onClose={() => (open = false)}
    onOpenStats={() => {
      open = false;
      statsOpen = true;
    }}
  />
{/if}

{#if statsOpen}
  <PomodoroStats onClose={() => (statsOpen = false)} />
{/if}

<style>
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 20px;
    padding: 0 7px;
    border-radius: 5px;
    font-size: 12px;
    color: var(--text-dim);
    transition: background 0.12s, color 0.12s;
  }
  .chip:hover {
    background: var(--bg-hover);
  }
  /* 专注与完成用主题强调色；休息与待开始只用次要文字色（避免在竹青主题里与强调色撞色） */
  .chip.focus,
  .chip.done {
    color: var(--accent);
  }
  .icon {
    font-size: 11px;
    line-height: 1;
  }
  .clock {
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.02em;
  }
  .mark {
    font-size: 10px;
    opacity: 0.85;
  }
  .panel-mask {
    position: fixed;
    inset: 0;
    z-index: 65;
  }
</style>
