<script lang="ts">
  import { editorSel } from '../lib/stores';
  import { FONT_SIZES, DEFAULT_FONT_SIZE } from '../lib/font-size';
  import type { EditorInstance } from '../lib/editor';

  interface Props {
    editor: EditorInstance | null;
    /** 是否可退回（存在更早的修改记录） */
    canBack: boolean;
    /** 是否可前进（退回后可撤回退回） */
    canForward: boolean;
    /** 退回/前进移动进行中（按钮置灰防连点） */
    historyBusy: boolean;
    snapshotsOpen: boolean;
    onCreateSnapshot: () => void;
    onToggleSnapshots: () => void;
    onExport: (kind: 'md' | 'pdf') => void;
    onHistoryBack: () => void;
    onHistoryForward: () => void;
  }
  let {
    editor,
    canBack,
    canForward,
    historyBusy,
    snapshotsOpen,
    onCreateSnapshot,
    onToggleSnapshots,
    onExport,
    onHistoryBack,
    onHistoryForward,
  }: Props = $props();

  // 直接读 store（runes 模式下的自动订阅），不再手写 subscribe/unsubscribe
  const sel = $derived($editorSel);
  let showCustom = $state(false);
  let customSize = $state('');
  let exportOpen = $state(false);

  /** 标题字号由级别决定，光标在标题内时不允许调字号 */
  const headingLocked = $derived(sel.heading !== 0);
  const isCustom = $derived(
    sel.fontSize != null && !(FONT_SIZES as readonly number[]).includes(sel.fontSize),
  );
  /** 下拉当前值：未设置字号时选中「默认」，自定义字号时选中自定义项 */
  const sizeValue = $derived(
    sel.fontSize == null ? '__default__' : isCustom ? '__custom__' : String(sel.fontSize),
  );

  function onHeadingChange(e: Event): void {
    editor?.setHeading(Number((e.target as HTMLSelectElement).value));
  }

  function onSizeChange(e: Event): void {
    const v = (e.target as HTMLSelectElement).value;
    if (v === '__custom__' || v === '__custom_edit__') {
      showCustom = true;
      return;
    }
    showCustom = false;
    if (v === '__default__') {
      editor?.setFontSize(null);
      return;
    }
    editor?.setFontSize(Number(v));
  }

  function applyCustom(): void {
    const n = Math.round(Number(customSize));
    if (Number.isFinite(n) && n >= 8 && n <= 96) {
      editor?.setFontSize(n);
      showCustom = false;
    }
  }

  function pickExport(kind: 'md' | 'pdf'): void {
    exportOpen = false;
    onExport(kind);
  }
</script>

<!--
  图标按钮统一为 26×26（与全局 .icon-btn 一致），不再带文字：
  文字会随缩放把整行撑宽，高缩放下右侧按钮被挤出可视区；
  说明改由 title 悬停提示 + aria-label 无障碍名称承担。
-->
<div class="toolbar">
  <div class="toolbar-inner">
    <div class="group history-group" role="group" aria-label="修改历史" title="修改历史：最近十次修改（不产生新记录，仅在本会话内）">
      <button
        class="tool-btn"
        type="button"
        aria-label="退回"
        title="退回：回到上一次修改后的内容"
        disabled={!editor || !canBack || historyBusy}
        onclick={onHistoryBack}
      >
        <svg
          viewBox="0 0 24 24"
          width="15"
          height="15"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
          focusable="false"
        >
          <path d="M9 14 4 9l5-5" />
          <path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11" />
        </svg>
      </button>
      <button
        class="tool-btn"
        type="button"
        aria-label="前进"
        title="前进：撤回退回"
        disabled={!editor || !canForward || historyBusy}
        onclick={onHistoryForward}
      >
        <svg
          viewBox="0 0 24 24"
          width="15"
          height="15"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
          focusable="false"
        >
          <path d="m15 14 5-5-5-5" />
          <path d="M20 9H9.5A5.5 5.5 0 0 0 4 14.5 5.5 5.5 0 0 0 9.5 20H13" />
        </svg>
      </button>
    </div>

    <label class="group" title="标题样式（Ctrl+1~6 标题，Ctrl+0 正文）">
    <span class="label">标题</span>
    <select disabled={!editor} value={sel.heading} onchange={onHeadingChange}>
      <option value="0">正文 (Ctrl+0)</option>
      <option value="1">标题一 (Ctrl+1)</option>
      <option value="2">标题二 (Ctrl+2)</option>
      <option value="3">标题三 (Ctrl+3)</option>
      <option value="4">标题四 (Ctrl+4)</option>
      <option value="5">标题五 (Ctrl+5)</option>
      <option value="6">标题六 (Ctrl+6)</option>
    </select>
  </label>

  <label
    class="group"
    title={headingLocked
      ? '标题字号由级别决定，只有正文段落可以调字号'
      : '段落字号（整段生效）：默认＝不写字号，由应用统一样式决定；固定＝把 px 写进文档。Ctrl+Alt+1~6 快捷设置，Ctrl+Alt+0 恢复默认'}
  >
    <span class="label">字号</span>
    <select
      disabled={!editor || headingLocked}
      value={sizeValue}
      onchange={onSizeChange}
      aria-label="段落字号"
    >
      {#if isCustom}
        <option value="__custom__">{sel.fontSize}px（自定义）</option>
        <option value="__custom_edit__">自定义…</option>
      {/if}
      <option value="__default__">默认（16px）</option>
      {#each FONT_SIZES as s (s)}
        <option value={s}>{s}px{s === DEFAULT_FONT_SIZE ? '（固定）' : ''}</option>
      {/each}
      {#if !isCustom}
        <option value="__custom__">自定义…</option>
      {/if}
    </select>
  </label>

  {#if showCustom}
    <span class="custom">
      <input
        type="number"
        min="8"
        max="96"
        placeholder="px"
        bind:value={customSize}
        onkeydown={(e) => {
          if (e.key === 'Enter') applyCustom();
        }}
      />
      <button class="mini-btn" onclick={applyCustom}>确定</button>
    </span>
  {/if}

    <div class="actions" role="group" aria-label="快照与导出">
      <button
        class="tool-btn"
        type="button"
        aria-label="复制快照"
        title="复制快照：保存当前正文为快照并进入对比"
        disabled={!editor}
        onclick={onCreateSnapshot}
      >
        <svg
          viewBox="0 0 24 24"
          width="15"
          height="15"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
          focusable="false"
        >
          <rect x="8" y="8" width="14" height="14" rx="2" />
          <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
          <path d="M15 11v6" />
          <path d="M12 14h6" />
        </svg>
      </button>
      <button
        class="tool-btn"
        class:on={snapshotsOpen}
        type="button"
        aria-label="快照记录"
        aria-pressed={snapshotsOpen}
        title="快照记录：查看/恢复不同版本"
        disabled={!editor}
        onclick={onToggleSnapshots}
      >
        <svg
          viewBox="0 0 24 24"
          width="15"
          height="15"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
          focusable="false"
        >
          <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
          <path d="M3 3v5h5" />
          <path d="M12 7v5l4 2" />
        </svg>
      </button>
      <div class="export-wrap">
        <button
          class="tool-btn"
          type="button"
          aria-label="导出"
          aria-haspopup="menu"
          aria-expanded={exportOpen}
          title="导出（仅正文）"
          disabled={!editor}
          onclick={() => (exportOpen = !exportOpen)}
        >
          <svg
            viewBox="0 0 24 24"
            width="15"
            height="15"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
            focusable="false"
          >
            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
            <path d="m7 10 5 5 5-5" />
            <path d="M12 15V3" />
          </svg>
        </button>
        {#if exportOpen}
          <div class="export-mask" onclick={() => (exportOpen = false)}></div>
          <div class="export-menu">
            <button onclick={() => pickExport('md')}>导出 Markdown (.md)</button>
            <button onclick={() => pickExport('pdf')}>导出 PDF (.pdf)</button>
          </div>
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .toolbar {
    flex: none;
    background: var(--bg-panel);
    border-bottom: 1px solid var(--border);
    user-select: none;
  }
  /* 工具栏固定占满编辑区宽度，不随正文宽度模式变化；左侧内缩仍与正文对齐。
     允许折行：系统/界面整体放大时视口变窄，宁可折成两行也不能把右侧按钮挤出可视区。 */
  .toolbar-inner {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    padding: 5px var(--editor-inset, 16px);
    box-sizing: border-box;
  }
  .group {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .label {
    font-size: 12px;
    color: var(--text-dim);
    white-space: nowrap;
  }
  input {
    font: inherit;
    font-size: 13px;
    padding: 3px 6px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg);
    color: var(--text);
    outline: none;
  }
  /* Linux / Wayland (WebKitGTK) 下，原生 <select> 会无视自定义配色的背景，
     渲染成浅灰色，与深色主题不匹配。这里关闭原生外观，完全交由 CSS 绘制，
     并用主题色 (#text-dim) 画一个随主题自适应的小箭头。 */
  select {
    font: inherit;
    font-size: 13px;
    padding: 3px 24px 3px 6px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg);
    color: var(--text);
    outline: none;
    cursor: pointer;
    -webkit-appearance: none;
    -moz-appearance: none;
    appearance: none;
    background-image: linear-gradient(45deg, transparent 50%, var(--text-dim) 50%),
      linear-gradient(135deg, var(--text-dim) 50%, transparent 50%);
    background-position: calc(100% - 15px) 50%, calc(100% - 10px) 50%;
    background-size: 5px 5px;
    background-repeat: no-repeat;
  }
  select:focus,
  input:focus {
    border-color: var(--accent);
  }
  .custom {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .custom input {
    width: 72px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
  }
  /* 图标按钮：固定 26×26 且不参与收缩，缩放时等比放大，点击区不会变小 */
  .tool-btn {
    font: inherit;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 26px;
    height: 26px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--bg);
    color: var(--text);
    cursor: pointer;
  }
  .tool-btn svg {
    display: block;
  }
  .tool-btn:hover:not(:disabled),
  .tool-btn.on {
    border-color: var(--accent);
    color: var(--accent);
  }
  .tool-btn:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .export-wrap {
    position: relative;
  }
  .export-mask {
    position: fixed;
    inset: 0;
    z-index: 59;
  }
  .export-menu {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 60;
    display: flex;
    flex-direction: column;
    min-width: 190px;
    padding: 4px;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 6px 22px rgba(0, 0, 0, 0.18);
  }
  .export-menu button {
    font: inherit;
    font-size: 13px;
    text-align: left;
    padding: 7px 12px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text);
    cursor: pointer;
    white-space: nowrap;
  }
  .export-menu button:hover {
    background: var(--bg-hover);
    color: var(--accent);
  }
  .mini-btn {
    font-size: 12px;
    padding: 3px 10px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg);
    color: var(--text);
    cursor: pointer;
  }
  .mini-btn:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
</style>
