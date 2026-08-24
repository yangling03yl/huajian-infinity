<script lang="ts">
  import { onMount } from 'svelte';
  import { editorSel } from '../lib/stores';
  import { FONT_SIZES, DEFAULT_FONT_SIZE } from '../lib/font-size';
  import type { EditorInstance } from '../lib/editor';

  let { editor }: { editor: EditorInstance | null } = $props();

  let sel = $state({ heading: 0, fontSize: null as number | null });
  let showCustom = $state(false);
  let customSize = $state('');

  const currentSize = $derived(sel.fontSize ?? DEFAULT_FONT_SIZE);
  const isCustom = $derived(sel.fontSize != null && !(FONT_SIZES as readonly number[]).includes(sel.fontSize));

  onMount(() => {
    return editorSel.subscribe((s) => (sel = s));
  });

  function onHeadingChange(e: Event): void {
    editor?.setHeading(Number((e.target as HTMLSelectElement).value));
  }

  function onSizeChange(e: Event): void {
    const v = (e.target as HTMLSelectElement).value;
    if (v === '__custom__') {
      showCustom = true;
      return;
    }
    if (v === '__clear__') {
      editor?.setFontSize(null);
      showCustom = false;
      return;
    }
    editor?.setFontSize(Number(v));
    showCustom = false;
  }

  function applyCustom(): void {
    const n = Math.round(Number(customSize));
    if (Number.isFinite(n) && n >= 8 && n <= 96) {
      editor?.setFontSize(n);
      showCustom = false;
    }
  }
</script>

<div class="toolbar">
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

  <label class="group" title="字号（Ctrl+Alt+1~6 快捷设置，Ctrl+Alt+0 恢复默认）">
    <span class="label">字号</span>
    {#if isCustom}
      <select disabled={!editor} value="__custom__" onchange={onSizeChange}>
        <option value="__custom__">{currentSize}px（自定义）</option>
        <option value="__clear__">恢复默认 (Ctrl+Alt+0)</option>
      </select>
    {:else}
      <select disabled={!editor} value={currentSize} onchange={onSizeChange}>
        {#each FONT_SIZES as s (s)}
          <option value={s}>{s}px{s === DEFAULT_FONT_SIZE ? '（正文）' : ''}</option>
        {/each}
        <option value="__custom__">自定义…</option>
        <option value="__clear__">恢复默认 (Ctrl+Alt+0)</option>
      </select>
    {/if}
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
</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 16px;
    flex: none;
    padding: 6px 16px;
    background: var(--bg-panel);
    border-bottom: 1px solid var(--border);
    user-select: none;
  }
  .group {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .label {
    font-size: 12px;
    color: var(--text-dim);
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
