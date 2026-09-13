<script lang="ts">
  import { api, errMsg } from '../lib/api';
  import type { PomodoroDayStats } from '../lib/types';

  interface Props {
    onClose: () => void;
  }
  let { onClose }: Props = $props();

  function pad2(n: number): string {
    return String(n).padStart(2, '0');
  }

  function dateKey(y: number, m: number, d: number): string {
    return `${y}-${pad2(m)}-${pad2(d)}`;
  }

  const today = new Date();
  const todayKey = dateKey(today.getFullYear(), today.getMonth() + 1, today.getDate());
  const currentYear = today.getFullYear();
  const currentMonth = today.getMonth() + 1;

  let year = $state(currentYear);
  let month = $state(currentMonth);
  let days = $state<PomodoroDayStats[]>([]);
  let loading = $state(true);
  let error = $state('');
  /** 选中的日期 YYYY-MM-DD（默认今天） */
  let selectedKey = $state<string | null>(todayKey);

  const byDate = $derived(new Map(days.map((d) => [d.date, d])));

  const totals = $derived.by(() => {
    let seconds = 0;
    let count = 0;
    let standard = 0;
    let forward = 0;
    let activeDays = 0;
    for (const d of days) {
      seconds += d.standard_seconds + d.forward_seconds;
      count += d.standard_count + d.forward_count;
      standard += d.standard_count;
      forward += d.forward_count;
      if (d.standard_count + d.forward_count > 0) activeDays += 1;
    }
    return { seconds, count, standard, forward, activeDays };
  });

  const selectedStat = $derived(selectedKey === null ? undefined : byDate.get(selectedKey));
  const selectedSessions = $derived(selectedStat?.sessions ?? []);

  const selectedLabel = $derived.by(() => {
    if (selectedKey === null) return '';
    const [y, m, d] = selectedKey.split('-').map(Number);
    const weekday = '日一二三四五六'[new Date(y, m - 1, d).getDay()];
    return `${m} 月 ${d} 日 周${weekday}`;
  });

  const selectedSummary = $derived.by(() => {
    if (!selectedStat) return '';
    const count = selectedStat.standard_count + selectedStat.forward_count;
    if (count === 0) return '';
    const seconds = selectedStat.standard_seconds + selectedStat.forward_seconds;
    return `${count} 个番茄 · ${formatDuration(seconds)}`;
  });

  // 周一为一周起点，前后补齐空位
  const cells = $derived.by(() => {
    const offset = (new Date(year, month - 1, 1).getDay() + 6) % 7;
    const total = new Date(year, month, 0).getDate();
    const list: (number | null)[] = [];
    for (let i = 0; i < offset; i += 1) list.push(null);
    for (let d = 1; d <= total; d += 1) list.push(d);
    while (list.length % 7 !== 0) list.push(null);
    return list;
  });

  const atCurrentMonth = $derived(year === currentYear && month === currentMonth);

  let loadedKey = '';
  $effect(() => {
    const key = `${year}-${month}`;
    if (key === loadedKey) return;
    loadedKey = key;
    void load();
  });

  async function load(): Promise<void> {
    loading = true;
    error = '';
    try {
      days = await api.getPomodoroStats(year, month);
    } catch (e) {
      days = [];
      error = errMsg(e);
    } finally {
      loading = false;
    }
  }

  function keyOf(day: number): string {
    return dateKey(year, month, day);
  }

  function statOf(day: number): PomodoroDayStats | undefined {
    return byDate.get(keyOf(day));
  }

  function minutesOf(day: number): number {
    const s = statOf(day);
    if (!s) return 0;
    return Math.round((s.standard_seconds + s.forward_seconds) / 60);
  }

  function isToday(day: number): boolean {
    return keyOf(day) === todayKey;
  }

  function isSelected(day: number): boolean {
    return keyOf(day) === selectedKey;
  }

  function isFuture(day: number): boolean {
    const ahead = year > currentYear || (year === currentYear && month > currentMonth);
    if (ahead) return true;
    return atCurrentMonth && day > today.getDate();
  }

  /**
   * 热力档位（强调色混入百分比）。
   * 上限控制在 40%：再深会让深色主题（如墨韵/竹青）格内文字的对比度不足。
   */
  const HEAT_LEVELS = [12, 20, 28, 34, 40] as const;

  function heatStrength(minutes: number): number {
    if (minutes <= 0) return 0;
    if (minutes >= 240) return HEAT_LEVELS[4];
    if (minutes >= 150) return HEAT_LEVELS[3];
    if (minutes >= 90) return HEAT_LEVELS[2];
    if (minutes >= 45) return HEAT_LEVELS[1];
    return HEAT_LEVELS[0];
  }

  /** 用主题强调色做强度热力：分钟数越多越深，自动适配 4 套主题的明暗两版 */
  function heatStyle(minutes: number): string {
    const strength = heatStrength(minutes);
    if (strength === 0) return '';
    return `background: color-mix(in srgb, var(--accent) ${strength}%, var(--bg))`;
  }

  function formatMinutes(minutes: number): string {
    if (minutes < 60) return `${minutes}m`;
    const h = Math.floor(minutes / 60);
    const m = minutes % 60;
    return m === 0 ? `${h}h` : `${h}h${pad2(m)}`;
  }

  function formatDuration(seconds: number): string {
    const total = Math.round(seconds / 60);
    const h = Math.floor(total / 60);
    const m = total % 60;
    if (h === 0) return `${m} 分钟`;
    return m === 0 ? `${h} 小时` : `${h} 小时 ${m} 分`;
  }

  function cellTitle(day: number): string {
    const s = statOf(day);
    if (!s || s.standard_count + s.forward_count === 0) return `${keyOf(day)} · 无记录（点击查看）`;
    const parts = [keyOf(day)];
    if (s.standard_count > 0) {
      parts.push(`标准 ${s.standard_count} 个（${formatDuration(s.standard_seconds)}）`);
    }
    if (s.forward_count > 0) {
      parts.push(`正向 ${s.forward_count} 个（${formatDuration(s.forward_seconds)}）`);
    }
    return parts.join(' · ');
  }

  function select(day: number): void {
    if (isFuture(day)) return;
    selectedKey = keyOf(day);
  }

  function shiftMonth(delta: number): void {
    const d = new Date(year, month - 1 + delta, 1);
    year = d.getFullYear();
    month = d.getMonth() + 1;
    // 换月后清空选中，避免明细里显示别的月份
    selectedKey = year === currentYear && month === currentMonth ? todayKey : null;
  }
</script>

<aside class="stats-panel" aria-label="番茄记录">
  <div class="panel-head">
    <span class="title">番茄记录</span>
    <button class="icon-btn" title="关闭" onclick={onClose}>✕</button>
  </div>

  <div class="panel-body">
    <div class="nav">
      <button class="icon-btn" title="上一个月" onclick={() => shiftMonth(-1)}>‹</button>
      <span class="ym">{year} 年 {month} 月</span>
      <button
        class="icon-btn"
        title={atCurrentMonth ? '已经是本月' : '下一个月'}
        disabled={atCurrentMonth}
        onclick={() => shiftMonth(1)}
      >
        ›
      </button>
    </div>

    <div class="weekdays">
      {#each ['一', '二', '三', '四', '五', '六', '日'] as w}
        <span>{w}</span>
      {/each}
    </div>

    <div class="grid">
      {#each cells as day}
        {#if day === null}
          <span class="cell empty"></span>
        {:else}
          <button
            type="button"
            class="cell"
            class:today={isToday(day)}
            class:future={isFuture(day)}
            class:selected={isSelected(day)}
            style={heatStyle(minutesOf(day))}
            title={cellTitle(day)}
            disabled={isFuture(day)}
            onclick={() => select(day)}
          >
            <span class="d">{day}</span>
            {#if minutesOf(day) > 0}
              <span class="m">{formatMinutes(minutesOf(day))}</span>
            {/if}
          </button>
        {/if}
      {/each}
    </div>

    <div class="legend">
      <span class="dim">少</span>
      {#each [0, 45, 90, 150, 240] as minutes}
        <i style={heatStyle(minutes) || 'background: var(--bg)'}></i>
      {/each}
      <span class="dim">多</span>
    </div>

    <div class="month-line">
      {#if loading}
        <span class="dim">读取中…</span>
      {:else if error}
        <span class="err">{error}</span>
      {:else if totals.count === 0}
        <span class="dim">{year} 年 {month} 月暂无番茄记录</span>
      {:else}
        <div class="line">
          <b>{formatDuration(totals.seconds)}</b>
          <span class="dim">· {totals.count} 个番茄</span>
        </div>
        <div class="dim small">
          标准 {totals.standard} 个 · 正向 {totals.forward} 个 · 有记录 {totals.activeDays} 天
        </div>
      {/if}
    </div>

    <div class="sep"></div>

    <div class="day">
      {#if selectedKey === null}
        <div class="empty">点击上方任意日期，查看当天记录</div>
      {:else}
        <div class="day-head">
          <span class="day-title">{selectedLabel}</span>
          {#if selectedSummary}<span class="day-sub">{selectedSummary}</span>{/if}
        </div>
        {#if selectedSessions.length > 0}
          <ul class="sessions">
            {#each selectedSessions as s}
              <li>
                <span class="at">{s.at}</span>
                <span class="mode" class:forward={s.mode === 'forward'}>
                  {s.mode === 'forward' ? '正向' : '标准'}
                </span>
                <span class="dur">{formatDuration(s.seconds)}</span>
              </li>
            {/each}
          </ul>
        {:else}
          <div class="empty">当日无记录</div>
        {/if}
      {/if}
    </div>
  </div>
</aside>

<style>
  /* 右侧边栏：让开顶栏（TabBar 38px）与状态栏（26px），不遮挡编辑器 */
  .stats-panel {
    position: fixed;
    top: 38px;
    right: 0;
    bottom: 26px;
    width: 330px;
    display: flex;
    flex-direction: column;
    background: var(--bg-panel);
    border-left: 1px solid var(--border);
    box-shadow: -8px 0 24px rgba(0, 0, 0, 0.12);
    z-index: 70;
  }
  .panel-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 10px 8px 14px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .title {
    font-size: 13px;
    font-weight: 700;
    color: var(--accent);
  }
  .panel-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px 16px;
  }
  .nav {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
  }
  .ym {
    font-size: 13px;
    color: var(--text);
  }
  .weekdays,
  .grid {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 4px;
  }
  .weekdays span {
    text-align: center;
    font-size: 11px;
    color: var(--text-dim);
    padding-bottom: 2px;
  }
  .cell {
    height: 40px;
    border-radius: 7px;
    /* 底色用 --bg（面板本身是 --bg-panel）+ 描边：明暗两版下网格都清晰可见 */
    border: 1px solid var(--border);
    background: var(--bg);
    color: inherit;
    font: inherit;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
    cursor: pointer;
  }
  .cell:not(.future):hover {
    border-color: var(--accent);
  }
  .cell.empty {
    background: none;
    border-color: transparent;
    cursor: default;
  }
  .cell.future {
    opacity: 0.4;
    cursor: default;
  }
  .cell.today {
    border-color: var(--accent);
  }
  .cell.selected {
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .d {
    position: absolute;
    top: 2px;
    left: 5px;
    font-size: 10px;
    color: var(--text);
    opacity: 0.7;
  }
  .m {
    font-size: 11px;
    color: var(--text);
    font-variant-numeric: tabular-nums;
    margin-top: 9px;
  }
  .legend {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 4px;
    margin-top: 8px;
  }
  .legend i {
    display: block;
    width: 12px;
    height: 12px;
    border-radius: 3px;
    border: 1px solid var(--border);
  }
  .dim {
    font-size: 11px;
    color: var(--text-dim);
  }
  .small {
    font-size: 11px;
    margin-top: 2px;
  }
  .month-line {
    margin-top: 10px;
  }
  .line {
    display: flex;
    align-items: baseline;
    gap: 6px;
    font-size: 13px;
  }
  .line b {
    font-size: 16px;
    color: var(--accent);
  }
  .err {
    font-size: 12px;
    color: var(--danger);
  }
  .sep {
    height: 1px;
    background: var(--border);
    margin: 12px 0 10px;
  }
  .day-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
    margin-bottom: 6px;
  }
  .day-title {
    font-size: 13px;
    color: var(--text);
  }
  .day-sub {
    font-size: 11px;
    color: var(--text-dim);
  }
  .sessions {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .sessions li {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 5px 2px;
    font-size: 12px;
    border-bottom: 1px dashed var(--border);
  }
  .sessions li:last-child {
    border-bottom: none;
  }
  .at {
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }
  .mode {
    font-size: 11px;
    padding: 0 5px;
    border-radius: 4px;
    border: 1px solid var(--border);
    color: var(--text-dim);
  }
  .mode.forward {
    color: var(--accent);
    border-color: var(--accent);
  }
  .dur {
    margin-left: auto;
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }
  .empty {
    font-size: 12px;
    color: var(--text-dim);
    padding: 6px 0;
  }
  .icon-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
</style>
