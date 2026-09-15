<script lang="ts">
  import { boxes } from '../lib/stores';
  import { api } from '../lib/api';
  import BoxItem from './BoxItem.svelte';

  interface Props {
    onNewBox: () => void;
    onOpenBox: () => void;
    onRenameRequest: (boxId: string, noteId: string) => void;
  }
  let { onNewBox, onOpenBox, onRenameRequest }: Props = $props();

  let dragIdx = $state(-1);
  let overIdx = $state(-1);

  function onDragStart(e: DragEvent, idx: number): void {
    dragIdx = idx;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = 'move';
      e.dataTransfer.setData('text/plain', String(idx));
    }
  }

  function onDragOver(e: DragEvent, idx: number): void {
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
    overIdx = idx;
  }

  function onDrop(e: DragEvent, idx: number): void {
    e.preventDefault();
    const from = dragIdx >= 0 ? dragIdx : Number(e.dataTransfer?.getData('text/plain') ?? -1);
    overIdx = -1;
    dragIdx = -1;
    const list = $boxes;
    if (from < 0 || from >= list.length || from === idx) return;
    const next = [...list];
    const [moved] = next.splice(from, 1);
    next.splice(idx, 0, moved);
    boxes.set(next);
    void api.reorderBoxes(next.map((b) => b.info.id));
  }

  function onDragEnd(): void {
    dragIdx = -1;
    overIdx = -1;
  }
</script>

<aside class="sidebar">
  <header class="sidebar-head">
    <span class="logo">花笺<span class="latin">infinity</span></span>
    <div class="head-actions">
      <button class="icon-btn" title="新建花匣" onclick={onNewBox}>＋</button>
      <button class="icon-btn folder-btn" title="打开花匣" onclick={onOpenBox} aria-label="打开花匣">
        <svg
          class="folder-icon"
          viewBox="0 0 24 24"
          width="18"
          height="18"
          aria-hidden="true"
          focusable="false"
        >
          <path
            class="folder-tab"
            style="fill: var(--folder-tab);"
            d="M4 4.6h4.2l1.8 2.1H20.4v2.9H4z"
          />
          <path
            class="folder-body"
            style="fill: var(--folder-body);"
            d="M4 9.6h16.4v10.6H4z"
          />
        </svg>
      </button>
    </div>
  </header>

  <div class="box-list">
    {#each $boxes as b, i (b.info.id)}
      <div
        class="box-slot"
        class:dragging={dragIdx === i}
        class:drag-over={overIdx === i && dragIdx !== i}
        draggable="true"
        ondragstart={(e) => onDragStart(e, i)}
        ondragover={(e) => onDragOver(e, i)}
        ondrop={(e) => onDrop(e, i)}
        ondragend={onDragEnd}
        ondragleave={() => overIdx === i && (overIdx = -1)}
      >
        <BoxItem
          box={b.info}
          expanded={b.expanded}
          notes={b.notes}
          onRenameRequest={onRenameRequest}
        />
      </div>
    {:else}
      <div class="hint">没有打开的花匣<br />点击 ＋ 新建，或 <svg class="hint-folder" viewBox="0 0 24 24" width="14" height="14" aria-hidden="true" focusable="false"><path d="M3.5 6.4h4.6l1.9 2.2h10.5v9.2a1.8 1.8 0 0 1-1.8 1.8H5.3a1.8 1.8 0 0 1-1.8-1.8z" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" /></svg> 打开 .hxl 文件</div>
    {/each}
  </div>
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-sidebar);
    overflow: hidden;
  }
  .sidebar-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .logo {
    font-size: 17px;
    font-weight: 700;
    letter-spacing: -0.5px;
    color: var(--accent);
    user-select: none;
  }
  /* 拉丁字母字重偏细，收紧一点让「花笺infinity」整体更紧凑 */
  .logo .latin {
    letter-spacing: -0.7px;
  }
  .head-actions {
    display: flex;
    gap: 2px;
  }
  /* ---- 打开花匣：自绘双色文件夹图标 ---- */
  /* 上盖由主题强调色与侧栏底色调出，随主题变化，不再用固定棕色 */
  .folder-btn {
    color: var(--accent);
    --folder-tab: color-mix(in srgb, var(--accent) 60%, var(--bg-sidebar));
    --folder-body: var(--accent);
  }
  .folder-btn:hover {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }
  .folder-icon {
    display: block;
    overflow: visible;
  }
  .folder-tab,
  .folder-body {
    transition: transform 0.16s ease;
  }
  /* 前板描边用侧栏底色，切出前后层次 */
  .folder-body {
    stroke: var(--bg-sidebar);
    stroke-width: 0.7;
    stroke-linejoin: round;
  }
  .folder-btn:hover .folder-body {
    transform: translateY(0.6px);
  }
  .folder-btn:active .folder-body {
    transform: translateY(1.2px);
  }
  .box-list {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0 12px;
  }
  .box-slot {
    border-top: 2px solid transparent;
  }
  .box-slot.dragging {
    opacity: 0.45;
  }
  .box-slot.drag-over {
    border-top-color: var(--accent);
  }
  .hint {
    color: var(--text-dim);
    font-size: 13px;
    line-height: 1.8;
    text-align: center;
    padding: 40px 16px;
  }
  /* 提示文案里的迷你文件夹：跟随文字颜色，保持与主题一致 */
  .hint-folder {
    display: inline-block;
    vertical-align: -2px;
  }
</style>
