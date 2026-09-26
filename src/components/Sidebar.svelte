<script lang="ts">
  import { boxes } from '../lib/stores';
  import { api } from '../lib/api';
  import { claimDrag, clickSuppressed, makeGhost, positionGhost, releaseDrag, removeGhost } from '../lib/dnd';
  import BoxItem from './BoxItem.svelte';

  interface Props {
    onNewBox: () => void;
    onOpenBox: () => void;
    onRenameRequest: (boxId: string, noteId: string) => void;
  }
  let { onNewBox, onOpenBox, onRenameRequest }: Props = $props();

  // ---- 花匣排序拖拽（指针事件自实现；Windows WebView2 在 dragDropEnabled 下
  // 禁用 HTML5 DnD，故不能用 draggable 原生拖拽） ----
  let dragIdx = $state(-1); // 正在拖拽的花匣
  let overIdx = $state(-1); // 悬停目标槽位（插入到它之前）
  let boxListEl: HTMLDivElement | undefined = $state();
  let pending: { idx: number; startX: number; startY: number } | null = null;
  let ghost: HTMLElement | null = null;
  let grabOff = { x: 0, y: 0 };
  const dragOwner = {};

  function onSlotPointerDown(e: PointerEvent, idx: number): void {
    if (e.button !== 0) return;
    const t = e.target as HTMLElement;
    // 只有花匣标题栏能发起拖拽；行内按钮、花笺行（由 BoxItem 自己处理）除外
    const head = t.closest('.box-head');
    if (t.closest('button') || !head) return;
    // 捕获在 box-head 上：保证指针移出窗口外也能收到 pointerup，
    // 同时 click 仍落在 head 上，不破坏折叠/展开
    try {
      (head as HTMLElement).setPointerCapture(e.pointerId);
    } catch { /* 捕获失败不影响后续流程 */ }
    pending = { idx, startX: e.clientX, startY: e.clientY };
  }

  function onWindowPointerMove(e: PointerEvent): void {
    if (ghost) {
      positionGhost(ghost, e.clientX, e.clientY, grabOff.x, grabOff.y);
      nudgeList(e.clientY);
      updateOverIdx(e.clientX, e.clientY);
      return;
    }
    if (!pending) return;
    if (Math.hypot(e.clientX - pending.startX, e.clientY - pending.startY) < 6) return;
    beginDrag(e);
  }

  function beginDrag(e: PointerEvent): void {
    const idx = pending!.idx;
    if (!claimDrag(dragOwner)) {
      pending = null;
      return;
    }
    const head = boxListEl?.children[idx]?.querySelector('.box-head') as HTMLElement | null;
    if (head) {
      const rect = head.getBoundingClientRect();
      grabOff = { x: e.clientX - rect.left, y: e.clientY - rect.top };
      ghost = makeGhost(head);
      positionGhost(ghost, e.clientX, e.clientY, grabOff.x, grabOff.y);
    }
    dragIdx = idx;
    pending = null;
  }

  function updateOverIdx(cx: number, cy: number): void {
    const el = document.elementFromPoint(cx, cy)?.closest('.box-slot') as HTMLElement | null;
    if (!el || !boxListEl?.contains(el)) {
      if (overIdx !== -1) overIdx = -1;
      return;
    }
    const idx = Array.prototype.indexOf.call(boxListEl.children, el);
    if (idx === dragIdx) {
      if (overIdx !== -1) overIdx = -1;
    } else if (overIdx !== idx) {
      overIdx = idx;
    }
  }

  /** 指针靠近侧栏上下边缘时自动滚动花匣列表 */
  function nudgeList(clientY: number): void {
    if (!boxListEl) return;
    const rect = boxListEl.getBoundingClientRect();
    const edge = 28;
    if (clientY < rect.top + edge) {
      boxListEl.scrollTop -= Math.max(4, (rect.top + edge - clientY) / 3);
    } else if (clientY > rect.bottom - edge) {
      boxListEl.scrollTop += Math.max(4, (clientY - (rect.bottom - edge)) / 3);
    }
  }

  function endDrag(commit: boolean): void {
    const from = dragIdx;
    const to = overIdx;
    removeGhost(ghost);
    ghost = null;
    dragIdx = -1;
    overIdx = -1;
    pending = null;
    releaseDrag(dragOwner);
    if (!commit || from < 0 || to < 0 || from === to) return;
    const list = $boxes;
    if (from >= list.length || to > list.length) return;
    const next = [...list];
    const [moved] = next.splice(from, 1);
    next.splice(to > from ? to - 1 : to, 0, moved);
    boxes.set(next);
    void api.reorderBoxes(next.map((b) => b.info.id));
  }

  function onWindowPointerUp(): void {
    if (ghost) endDrag(true);
    else pending = null;
  }

  function onWindowPointerCancel(): void {
    if (ghost || pending) endDrag(false);
  }

  function onWindowKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape' && (ghost || pending)) endDrag(false);
  }

  $effect(() => {
    window.addEventListener('pointermove', onWindowPointerMove);
    window.addEventListener('pointerup', onWindowPointerUp);
    window.addEventListener('pointercancel', onWindowPointerCancel);
    window.addEventListener('keydown', onWindowKeyDown);
    return () => {
      window.removeEventListener('pointermove', onWindowPointerMove);
      window.removeEventListener('pointerup', onWindowPointerUp);
      window.removeEventListener('pointercancel', onWindowPointerCancel);
      window.removeEventListener('keydown', onWindowKeyDown);
    };
  });
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

  <div class="box-list" bind:this={boxListEl}>
    {#each $boxes as b, i (b.info.id)}
      <div
        class="box-slot"
        class:dragging={dragIdx === i}
        class:drag-over={overIdx === i && dragIdx !== i}
        onpointerdown={(e) => onSlotPointerDown(e, i)}
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
