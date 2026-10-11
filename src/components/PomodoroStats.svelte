<script lang="ts">
  import { onDestroy } from 'svelte';
  import { api, errMsg } from '../lib/api';
  import { statusMsg } from '../lib/stores';
  import type { PomodoroBaseline, PomodoroDayStats, PomodoroSession } from '../lib/types';

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

  // ---- 区间并集：合计口径与后端一致（重叠只算一次，跨午夜按前一天处理）----

  /** 一天的总秒数上限，并集裁剪边界 */
  const DAY_SECONDS = 86400;

  const EMPTY_BASELINE: PomodoroBaseline = {
    standard_seconds: 0,
    standard_count: 0,
    forward_seconds: 0,
    forward_count: 0,
  };

  /** "HH:MM:SS"（兼容旧数据 "HH:MM"）→ 当天零点起的秒数；无法解析返回 NaN */
  function hms(text: string): number {
    if (!text) return Number.NaN;
    const parts = text.split(':');
    if (parts.length < 2 || parts.length > 3) return Number.NaN;
    const h = Number(parts[0]);
    const m = Number(parts[1]);
    const sec = parts.length === 3 ? Number(parts[2]) : 0;
    if (!Number.isFinite(h) || !Number.isFinite(m) || !Number.isFinite(sec)) return Number.NaN;
    return h * 3600 + m * 60 + sec;
  }

  function baselineOf(day: PomodoroDayStats): PomodoroBaseline {
    return day.baseline ?? EMPTY_BASELINE;
  }

  function countOf(day: PomodoroDayStats | undefined): number {
    return day ? day.standard_count + day.forward_count : 0;
  }

  /**
   * 单条记录的当天区间 [start, end]：
   * start > end 视为跨午夜（视作前一天开始），整体裁剪到当天 [0, 86400]，
   * 裁剪后为空（end <= start）返回 null。
   */
  function sessionInterval(s: PomodoroSession): [number, number] | null {
    let start = hms(s.start || s.end || '');
    const end = hms(s.end || s.start || '');
    if (!Number.isFinite(start) || !Number.isFinite(end)) return null;
    if (start > end) start -= DAY_SECONDS;
    const a = Math.max(0, start);
    const b = Math.min(DAY_SECONDS, end);
    return b > a ? [a, b] : null;
  }

  /** 明细整体并集秒数（不分模式，重叠只算一次） */
  function unionSeconds(sessions: PomodoroSession[]): number {
    const intervals: [number, number][] = [];
    for (const s of sessions) {
      const iv = sessionInterval(s);
      if (iv !== null) intervals.push(iv);
    }
    if (intervals.length === 0) return 0;
    intervals.sort((p, q) => p[0] - q[0] || p[1] - q[1]);
    let total = 0;
    let [curStart, curEnd] = intervals[0];
    for (let i = 1; i < intervals.length; i += 1) {
      const [s, e] = intervals[i];
      if (s <= curEnd) {
        if (e > curEnd) curEnd = e;
      } else {
        total += curEnd - curStart;
        curStart = s;
        curEnd = e;
      }
    }
    return total + (curEnd - curStart);
  }

  /** 当天总计秒 = baseline 两个分项 + 明细整体并集（旧汇总日无明细，即 baseline 合计） */
  function dayTotalSeconds(day: PomodoroDayStats): number {
    const b = baselineOf(day);
    return b.standard_seconds + b.forward_seconds + unionSeconds(day.sessions ?? []);
  }

  /** 与另一条记录区间相交（严格相交；首尾相接不算）的行标记 */
  function overlapFlags(sessions: PomodoroSession[]): boolean[] {
    const intervals = sessions.map(sessionInterval);
    return intervals.map((a, i) => {
      if (a === null) return false;
      return intervals.some(
        (b, j) => i !== j && b !== null && Math.max(a[0], b[0]) < Math.min(a[1], b[1]),
      );
    });
  }

  const totals = $derived.by(() => {
    let seconds = 0;
    let count = 0;
    let standard = 0;
    let forward = 0;
    let activeDays = 0;
    for (const d of days) {
      seconds += dayTotalSeconds(d);
      count += d.standard_count + d.forward_count;
      standard += d.standard_count;
      forward += d.forward_count;
      if (d.standard_count + d.forward_count > 0) activeDays += 1;
    }
    return { seconds, count, standard, forward, activeDays };
  });

  const selectedStat = $derived(selectedKey === null ? undefined : byDate.get(selectedKey));
  const selectedSessions = $derived(selectedStat?.sessions ?? []);
  const selectedOverlaps = $derived(overlapFlags(selectedSessions));
  const selectedHasOverlap = $derived(selectedOverlaps.some(Boolean));
  /** 旧汇总日：没有明细但计数 > 0（不可编辑/删除） */
  const selectedIsLegacySummary = $derived(
    selectedStat !== undefined && selectedSessions.length === 0 && countOf(selectedStat) > 0,
  );

  const selectedLabel = $derived.by(() => {
    if (selectedKey === null) return '';
    const [y, m, d] = selectedKey.split('-').map(Number);
    const weekday = '日一二三四五六'[new Date(y, m - 1, d).getDay()];
    return `${m} 月 ${d} 日 周${weekday}`;
  });

  const selectedSummary = $derived.by(() => {
    if (!selectedStat) return '';
    const count = countOf(selectedStat);
    if (count === 0) return '';
    return `${count} 个番茄 · ${formatDuration(dayTotalSeconds(selectedStat))}`;
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

  // ---- 状态栏提示（与 pomodoro.ts 的 notify 同款：8 秒后清空）----

  const MESSAGE_MS = 8000;
  let msgTimer: ReturnType<typeof setTimeout> | null = null;

  function notify(text: string): void {
    statusMsg.set(text);
    if (msgTimer !== null) clearTimeout(msgTimer);
    msgTimer = setTimeout(() => {
      msgTimer = null;
      statusMsg.set('');
    }, MESSAGE_MS);
  }

  onDestroy(() => {
    if (msgTimer !== null) clearTimeout(msgTimer);
  });

  // ---- 行内改短 / 删除 ----

  let editingId = $state<string | null>(null);
  let editValue = $state('');
  let confirmingId = $state<string | null>(null);
  let busyId = $state<string | null>(null);

  /** 切换选中日期时收起行内编辑/确认，避免状态串到别的记录 */
  $effect(() => {
    if (selectedKey === null) return;
    editingId = null;
    editValue = '';
    confirmingId = null;
  });

  /** 可改短的分钟上限；< 1 表示原时长不足 1 分钟，禁止编辑 */
  function maxMinutes(s: PomodoroSession): number {
    return Math.floor(s.seconds / 60);
  }

  function canEdit(s: PomodoroSession): boolean {
    return maxMinutes(s) >= 1 && busyId === null;
  }

  function startEdit(s: PomodoroSession): void {
    if (!canEdit(s)) {
      if (maxMinutes(s) < 1) notify('原时长不足 1 分钟，不能改短');
      return;
    }
    confirmingId = null;
    editingId = s.id;
    editValue = String(maxMinutes(s));
  }

  function cancelEdit(): void {
    editingId = null;
    editValue = '';
  }

  function focusInput(node: HTMLInputElement): void {
    node.focus();
    node.select();
  }

  async function submitEdit(s: PomodoroSession): Promise<void> {
    const max = maxMinutes(s);
    if (max < 1) {
      cancelEdit();
      return;
    }
    let minutes = Number.parseInt(editValue, 10);
    if (!Number.isFinite(minutes)) minutes = max;
    let clamped = false;
    if (minutes > max) {
      minutes = max;
      clamped = true;
    } else if (minutes < 1) {
      minutes = 1;
    }
    const seconds = minutes * 60;
    // 没改（例如输入超过上限但原值已是上限）：只提示，不写盘
    if (seconds === s.seconds) {
      if (clamped) notify('只能减短，不能加长');
      cancelEdit();
      return;
    }
    const date = selectedStat?.date;
    if (!date) {
      cancelEdit();
      return;
    }
    busyId = s.id;
    try {
      await api.shortenPomodoroSession(date, s.id, seconds);
      notify(clamped ? '只能减短，不能加长' : `已改为 ${minutes} 分钟`);
    } catch (e) {
      notify(`改短失败: ${errMsg(e)}`);
    } finally {
      busyId = null;
      cancelEdit();
      await load();
    }
  }

  async function doDelete(s: PomodoroSession): Promise<void> {
    const date = selectedStat?.date;
    if (!date) return;
    busyId = s.id;
    try {
      await api.deletePomodoroSession(date, s.id);
      notify('已删除该条记录');
    } catch (e) {
      notify(`删除失败: ${errMsg(e)}`);
    } finally {
      busyId = null;
      confirmingId = null;
      await load();
    }
  }

  async function openDataLocation(): Promise<void> {
    try {
      const path = await api.huazhongPath();
      await api.revealInFolder(path);
    } catch (e) {
      notify(`打开数据位置失败: ${errMsg(e)}`);
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
    return Math.round(dayTotalSeconds(s) / 60);
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

  /** 明细行时长：不足 1 分钟显示秒，避免四舍五入成 "0/1 分钟" */
  function formatSessionDuration(seconds: number): string {
    if (seconds < 60) return `${seconds} 秒`;
    return formatDuration(seconds);
  }

  function cellTitle(day: number): string {
    const s = statOf(day);
    if (!s || countOf(s) === 0) return `${keyOf(day)} · 无记录（点击查看）`;
    const parts = [keyOf(day), `${countOf(s)} 个番茄`, formatDuration(dayTotalSeconds(s))];
    if (s.standard_count > 0) parts.push(`标准 ${s.standard_count} 个`);
    if (s.forward_count > 0) parts.push(`正向 ${s.forward_count} 个`);
    if (overlapFlags(s.sessions ?? []).some(Boolean)) parts.push('含重叠，已去重计时');
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

        {#if selectedHasOverlap}
          <div class="overlap-note" title="同一天内重叠的区间在合计里只算一次">
            含重叠记录：重叠部分不重复计时
          </div>
        {/if}

        {#if selectedSessions.length > 0}
          <ul class="sessions">
            {#each selectedSessions as s, i (s.id)}
              <li class:busy={busyId === s.id}>
                <span class="at">{s.at}</span>
                <span class="mode" class:forward={s.mode === 'forward'}>
                  {s.mode === 'forward' ? '正向' : '标准'}
                </span>
                {#if selectedOverlaps[i]}
                  <span class="overlap" title="与另一条记录时间重叠；合计按区间并集去重，不重复计时">
                    重叠
                  </span>
                {/if}

                <span class="grow"></span>

                {#if editingId === s.id}
                  <span class="edit">
                    <input
                      class="edit-input"
                      type="number"
                      inputmode="numeric"
                      min="1"
                      max={maxMinutes(s)}
                      step="1"
                      value={editValue}
                      aria-label="改短后的分钟数"
                      use:focusInput
                      oninput={(e) => (editValue = e.currentTarget.value)}
                      onkeydown={(e) => {
                        if (e.key === 'Enter') {
                          e.preventDefault();
                          void submitEdit(s);
                        } else if (e.key === 'Escape') {
                          e.preventDefault();
                          cancelEdit();
                        }
                      }}
                    />
                    <span class="unit">分</span>
                    <button class="mini" onclick={() => void submitEdit(s)}>确定</button>
                    <button class="mini" onclick={cancelEdit}>取消</button>
                  </span>
                {:else}
                  <button
                    class="dur-btn"
                    disabled={!canEdit(s)}
                    title={maxMinutes(s) < 1
                      ? '原时长不足 1 分钟，不能改短'
                      : `点击改短（最多 ${maxMinutes(s)} 分钟，只能减短）`}
                    onclick={() => startEdit(s)}
                  >
                    {formatSessionDuration(s.seconds)}
                  </button>
                {/if}

                {#if confirmingId === s.id}
                  <span class="confirm">
                    <button
                      class="mini danger"
                      disabled={busyId !== null}
                      onclick={() => void doDelete(s)}
                    >
                      确认
                    </button>
                    <button class="mini" onclick={() => (confirmingId = null)}>取消</button>
                  </span>
                {:else}
                  <button
                    class="mini del"
                    title="删除这条记录"
                    disabled={busyId !== null}
                    onclick={() => {
                      cancelEdit();
                      confirmingId = s.id;
                    }}
                  >
                    删除
                  </button>
                {/if}
              </li>
            {/each}
          </ul>
        {:else if selectedIsLegacySummary}
          <div class="legacy">
            <span class="legacy-tag">旧版本汇总 · 无明细</span>
            <span class="dim">
              合计 {selectedStat ? formatDuration(dayTotalSeconds(selectedStat)) : '0 分钟'} ·
              此日期无单条记录，不能编辑或删除
            </span>
          </div>
        {:else}
          <div class="empty">当日无记录</div>
        {/if}
      {/if}
    </div>

    <div class="sep"></div>

    <div class="foot">
      <button
        class="ghost-btn"
        title="在文件管理器中显示 花钟.hz"
        onclick={() => void openDataLocation()}
      >
        打开数据位置
      </button>
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
  .overlap-note {
    font-size: 11px;
    color: var(--text-dim);
    border-left: 2px solid var(--accent);
    padding-left: 6px;
    margin-bottom: 6px;
  }
  .sessions {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .sessions li {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 5px;
    padding: 5px 2px;
    font-size: 12px;
    border-bottom: 1px dashed var(--border);
  }
  .sessions li:last-child {
    border-bottom: none;
  }
  .sessions li.busy {
    opacity: 0.55;
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
  .overlap {
    font-size: 10px;
    padding: 0 4px;
    border-radius: 4px;
    border: 1px dashed var(--accent);
    color: var(--accent);
    cursor: help;
  }
  .grow {
    flex: 1;
    min-width: 4px;
  }
  .dur-btn {
    font: inherit;
    font-size: 12px;
    color: var(--text);
    font-variant-numeric: tabular-nums;
    padding: 1px 4px;
    border-radius: 4px;
    border: 1px solid transparent;
    background: transparent;
    cursor: pointer;
  }
  .dur-btn:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--accent);
  }
  .dur-btn:disabled {
    color: var(--text-dim);
    cursor: not-allowed;
  }
  .edit,
  .confirm {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .edit-input {
    width: 46px;
    padding: 1px 4px;
    font: inherit;
    font-size: 12px;
    color: var(--text);
    background: var(--bg);
    border: 1px solid var(--accent);
    border-radius: 4px;
    font-variant-numeric: tabular-nums;
  }
  .unit {
    font-size: 11px;
    color: var(--text-dim);
  }
  .mini {
    font: inherit;
    font-size: 11px;
    padding: 1px 6px;
    border-radius: 4px;
    border: 1px solid var(--border);
    background: var(--bg-panel);
    color: var(--text-dim);
    cursor: pointer;
  }
  .mini:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--accent);
  }
  .mini:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .mini.del:hover:not(:disabled),
  .mini.danger {
    color: var(--danger);
    border-color: var(--danger);
  }
  .legacy {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 2px 0;
  }
  .legacy-tag {
    font-size: 12px;
    color: var(--text);
  }
  .empty {
    font-size: 12px;
    color: var(--text-dim);
    padding: 6px 0;
  }
  .foot {
    display: flex;
    justify-content: flex-end;
  }
  .icon-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
</style>