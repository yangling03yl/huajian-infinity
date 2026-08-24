<script lang="ts">
  import { marked } from 'marked';
  import { computeDiff, diffStats } from '../lib/diff';
  import type { SnapshotMeta } from '../lib/types';

  interface Props {
    meta: SnapshotMeta;
    /** 快照内容（基准） */
    snapshotMd: string;
    /** 当前正文（实时变化） */
    currentMd: string;
    /** 是否为“复制快照”会话（显示合并/放弃） */
    canMerge: boolean;
    onMerge: () => void;
    onDiscard: () => void;
    onClose: () => void;
  }
  let { meta, snapshotMd, currentMd, canMerge, onMerge, onDiscard, onClose }: Props = $props();

  let mode = $state<'diff' | 'preview'>('diff');

  const lines = $derived(computeDiff(snapshotMd, currentMd));
  const stats = $derived(diffStats(lines));
  const previewHtml = $derived(
    marked.parse(snapshotMd, { gfm: true, breaks: false, async: false }) as string,
  );

  function fmtTime(iso: string): string {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return iso;
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }
</script>

<div class="diff-pane">
  <header class="diff-head">
    <div class="meta">
      <span class="ver">快照 v{meta.version}</span>
      <span class="label">{meta.label}</span>
      <span class="time">{fmtTime(meta.created_at)}</span>
    </div>
    <div class="tabs">
      <button class="tab" class:on={mode === 'diff'} onclick={() => (mode = 'diff')}>差异</button>
      <button class="tab" class:on={mode === 'preview'} onclick={() => (mode = 'preview')}>快照预览</button>
    </div>
  </header>

  <div class="stats">
    {#if stats.added || stats.removed}
      <span class="add">+{stats.added} 行</span>
      <span class="del">−{stats.removed} 行</span>
      <span class="hint">相对当前正文</span>
    {:else}
      <span class="hint">与当前正文完全一致</span>
    {/if}
  </div>

  {#if mode === 'diff'}
    <div class="diff-body">
      {#each lines as line, i (i)}
        {#if line.type === 'add'}
          <div class="diff-line add"><span class="sign">+</span>{line.text}</div>
        {:else if line.type === 'del'}
          <div class="diff-line del"><span class="sign">−</span>{line.text}</div>
        {:else}
          <div class="diff-line ctx">{line.text}</div>
        {/if}
      {:else}
        <div class="empty">（空快照）</div>
      {/each}
    </div>
  {:else}
    <div class="preview-body">
      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
      {@html previewHtml}
    </div>
  {/if}

  <footer class="diff-foot">
    {#if canMerge}
      <button class="btn primary" title="左侧当前内容即为合并后的正文" onclick={onMerge}>合并（保留左侧修改）</button>
      <button class="btn" title="放弃左侧修改，恢复为快照 v{meta.version} 的内容" onclick={onDiscard}>放弃修改</button>
    {:else}
      <button class="btn" title="关闭对比" onclick={onClose}>关闭对比</button>
    {/if}
  </footer>
</div>

<style>
  .diff-pane {
    flex: none;
    width: 44%;
    min-width: 320px;
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-panel);
    border-left: 1px solid var(--border);
  }
  .diff-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .meta {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
    overflow: hidden;
  }
  .ver {
    color: var(--accent);
    font-weight: 700;
    font-size: 13px;
    white-space: nowrap;
  }
  .label {
    font-size: 12px;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .time {
    font-size: 11px;
    color: var(--text-dim);
    white-space: nowrap;
  }
  .tabs {
    display: flex;
    gap: 2px;
    flex: none;
  }
  .tab {
    font-size: 12px;
    padding: 2px 10px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg);
    color: var(--text-dim);
    cursor: pointer;
  }
  .tab.on {
    border-color: var(--accent);
    color: var(--accent);
  }
  .stats {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    font-size: 12px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .stats .add {
    color: #3f8f4f;
    font-weight: 600;
  }
  .stats .del {
    color: #c0504d;
    font-weight: 600;
  }
  .stats .hint {
    color: var(--text-dim);
  }
  .diff-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 8px 0 16px;
    font-family: ui-monospace, 'Cascadia Mono', 'Noto Sans Mono CJK SC', Consolas, monospace;
    font-size: 12.5px;
    line-height: 1.7;
    background: var(--bg);
  }
  .diff-line {
    display: flex;
    gap: 8px;
    padding: 0 12px;
    white-space: pre-wrap;
    word-break: break-all;
  }
  .diff-line .sign {
    flex: none;
    width: 14px;
    text-align: center;
    user-select: none;
  }
  .diff-line.add {
    background: rgba(63, 143, 79, 0.14);
    color: var(--text);
  }
  .diff-line.add .sign {
    color: #3f8f4f;
    font-weight: 700;
  }
  .diff-line.del {
    background: rgba(192, 80, 77, 0.13);
    color: var(--text);
    text-decoration: none;
  }
  .diff-line.del .sign {
    color: #c0504d;
    font-weight: 700;
  }
  .diff-line.ctx {
    color: var(--text-dim);
  }
  .empty {
    padding: 16px;
    color: var(--text-dim);
    text-align: center;
  }
  .preview-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 16px 20px 24px;
    background: var(--bg);
  }
  .preview-body :global(h1),
  .preview-body :global(h2),
  .preview-body :global(h3) {
    margin: 0.7em 0 0.4em;
  }
  .preview-body :global(p) {
    margin: 0.35em 0;
  }
  .preview-body :global(pre) {
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 10px;
    overflow: auto;
  }
  .preview-body :global(code) {
    font-family: ui-monospace, 'Noto Sans Mono CJK SC', Consolas, monospace;
    font-size: 0.9em;
  }
  .preview-body :global(blockquote) {
    border-left: 3px solid var(--border);
    margin: 0.4em 0;
    padding-left: 12px;
    color: var(--text-dim);
  }
  .preview-body :global(table) {
    border-collapse: collapse;
  }
  .preview-body :global(th),
  .preview-body :global(td) {
    border: 1px solid var(--border);
    padding: 4px 10px;
  }
  .diff-foot {
    display: flex;
    gap: 8px;
    padding: 8px 12px;
    border-top: 1px solid var(--border);
    flex: none;
  }
  .btn {
    font: inherit;
    font-size: 12.5px;
    padding: 5px 14px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--bg);
    color: var(--text);
    cursor: pointer;
  }
  .btn:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
    font-weight: 600;
  }
  .btn.primary:hover {
    filter: brightness(1.08);
    color: #fff;
  }
</style>
