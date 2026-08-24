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
    <span class="logo">花笺infinity</span>
    <div class="head-actions">
      <button class="icon-btn" title="新建花匣" onclick={onNewBox}>＋</button>
      <button class="icon-btn" title="打开花匣" onclick={onOpenBox}>📂</button>
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
      <div class="hint">没有打开的花匣<br />点击 ＋ 新建，或 📂 打开 .hxl 文件</div>
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
    letter-spacing: 2px;
    color: var(--accent);
  }
  .head-actions {
    display: flex;
    gap: 2px;
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
</style>
