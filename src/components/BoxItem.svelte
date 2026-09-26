<script lang="ts">
  import type { BoxInfo, NoteMeta } from '../lib/types';
  import { activeTab, noteById, openNote, removeBoxUI, removeNoteUI, refreshNotes, reorderNotesUI, toggleBox } from '../lib/stores';
  import { api, errMsg } from '../lib/api';
  import { nextColor } from '../lib/colors';
  import { claimDrag, clickSuppressed, makeGhost, positionGhost, releaseDrag, removeGhost } from '../lib/dnd';
  import VirtualList from './VirtualList.svelte';

  interface Props {
    box: BoxInfo;
    expanded: boolean;
    notes: NoteMeta[];
    onRenameRequest: (boxId: string, noteId: string) => void;
  }
  let { box, expanded, notes, onRenameRequest }: Props = $props();

  const activeKey = $derived(($activeTab ? `${$activeTab.boxId}/${$activeTab.noteId}` : null));

  let busy = $state(false);
  let confirmDelete = $state<string | null>(null);
  /** 花笺排序拖拽：拖拽源 + 悬停目标行与插入位置（上缘/下缘） */
  let dragNoteId = $state<string | null>(null);
  let dropTarget = $state<{ id: string; pos: 'above' | 'below' } | null>(null);

  let vl: VirtualList | undefined = $state();
  let notesEl: HTMLDivElement | undefined = $state();
  let pending: { note: NoteMeta; startX: number; startY: number } | null = null;
  let ghost: HTMLElement | null = null;
  let grabOff = { x: 0, y: 0 };
  const dragOwner = {};

  async function createNote() {
    const used = notes.map((n) => n.color);
    const color = nextColor(used);
    try {
      const meta = await api.createNote(box.id, '未命名花笺', color);
      await refreshNotes(box.id);
      openNote(box.id, meta.id);
      onRenameRequest(box.id, meta.id);
    } catch (e) {
      alert(errMsg(e));
    }
  }

  async function delNote(noteId: string) {
    confirmDelete = null;
    try {
      await api.deleteNote(box.id, noteId);
      removeNoteUI(box.id, noteId);
    } catch (e) {
      alert(errMsg(e));
    }
  }

  async function close() {
    busy = true;
    try {
      await removeBoxUI(box.id);
    } finally {
      busy = false;
    }
  }

  async function forceSave() {
    try {
      await api.saveBox(box.id);
    } catch (e) {
      alert(errMsg(e));
    }
  }

  // ---- 侧边栏花笺拖拽排序（指针事件自实现，仅限同一花匣内；Windows WebView2
  // 在 dragDropEnabled 下禁用 HTML5 DnD，故不能用 draggable 原生拖拽） ----

  function onNotePointerDown(e: PointerEvent, note: NoteMeta): void {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest('button')) return; // 行内按钮不发起拖拽
    try {
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    } catch { /* 捕获失败不影响后续流程 */ }
    pending = { note, startX: e.clientX, startY: e.clientY };
  }

  function onWindowPointerMove(e: PointerEvent): void {
    if (ghost) {
      positionGhost(ghost, e.clientX, e.clientY, grabOff.x, grabOff.y);
      vl?.nudgeScroll(e.clientY);
      updateDropTarget(e.clientX, e.clientY);
      return;
    }
    if (!pending) return;
    if (Math.hypot(e.clientX - pending.startX, e.clientY - pending.startY) < 6) return;
    beginDrag(e);
  }

  function beginDrag(e: PointerEvent): void {
    const note = pending!.note;
    if (!claimDrag(dragOwner)) {
      pending = null;
      return;
    }
    const row = notesEl?.querySelector(`[data-note-id="${note.id}"]`) as HTMLElement | null;
    if (row) {
      const rect = row.getBoundingClientRect();
      grabOff = { x: e.clientX - rect.left, y: e.clientY - rect.top };
      ghost = makeGhost(row);
      positionGhost(ghost, e.clientX, e.clientY, grabOff.x, grabOff.y);
    }
    dragNoteId = note.id;
    pending = null;
  }

  function updateDropTarget(cx: number, cy: number): void {
    const src = dragNoteId;
    if (!src) return;
    const el = document.elementFromPoint(cx, cy)?.closest('.note-row') as HTMLElement | null;
    const id = el && notesEl?.contains(el) ? (el.dataset.noteId ?? null) : null;
    if (!id || id === src) {
      if (dropTarget) dropTarget = null;
      return;
    }
    const rect = el!.getBoundingClientRect();
    const pos: 'above' | 'below' = cy < rect.top + rect.height / 2 ? 'above' : 'below';
    if (dropTarget?.id !== id || dropTarget?.pos !== pos) dropTarget = { id, pos };
  }

  function endDrag(commit: boolean): void {
    const src = dragNoteId;
    const target = dropTarget;
    removeGhost(ghost);
    ghost = null;
    dragNoteId = null;
    dropTarget = null;
    pending = null;
    releaseDrag(dragOwner);
    if (!commit || !src || !target) return;
    commitOrder(moveNote(notes, src, target.id, target.pos));
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

  /** 把 fromId 的花笺移动到 targetId 的上/下缘，返回新数组（不改动时返回原数组） */
  function moveNote(
    list: NoteMeta[],
    fromId: string,
    targetId: string,
    pos: 'above' | 'below',
  ): NoteMeta[] {
    const from = list.findIndex((n) => n.id === fromId);
    if (from < 0) return list;
    const next = [...list];
    const [moved] = next.splice(from, 1);
    const to = next.findIndex((n) => n.id === targetId);
    if (to < 0) return list;
    next.splice(pos === 'below' ? to + 1 : to, 0, moved);
    return next;
  }

  function commitOrder(next: NoteMeta[]): void {
    const order = next.map((n) => n.id);
    if (order.every((id, i) => id === notes[i]?.id)) return; // 顺序未变化
    reorderNotesUI(box.id, order);
    void api.reorderNotes(box.id, order).catch((e: unknown) => {
      alert(errMsg(e));
      void refreshNotes(box.id);
    });
  }
</script>

<div class="box">
  <div class="box-head" role="button" tabindex="-1" onclick={() => { if (!clickSuppressed()) toggleBox(box.id); }}>
    <span class="caret {expanded ? 'open' : ''}">▸</span>
    <span class="box-name">{box.name}</span>
    <span class="count">{notes.length}</span>
    <span class="actions" onclick={(e) => e.stopPropagation()}>
      <button class="icon-btn" title="新建花笺" onclick={createNote}>＋</button>
      <button class="icon-btn" title="强制保存花匣" onclick={forceSave}>⤓</button>
      <button class="icon-btn" title="关闭花匣" onclick={close} disabled={busy}>×</button>
    </span>
  </div>

  {#if expanded}
    <div class="notes" bind:this={notesEl}>
      {#if notes.length === 0}
        <div class="empty">空花匣，点 ＋ 新建花笺</div>
      {:else}
        <VirtualList count={notes.length} bind:this={vl}>
          {#snippet children(idx)}
            {@const note = notes[idx]}
            {@const key = `${box.id}/${note.id}`}
            <div
              class="note-row {activeKey === key ? 'active' : ''}"
              class:dragging={dragNoteId === note.id}
              class:drop-above={dropTarget?.id === note.id && dropTarget.pos === 'above'}
              class:drop-below={dropTarget?.id === note.id && dropTarget.pos === 'below'}
              role="button"
              tabindex="-1"
              data-note-id={note.id}
              onclick={() => {
                if (clickSuppressed()) return;
                openNote(box.id, note.id);
              }}
              ondblclick={() => onRenameRequest(box.id, note.id)}
              onpointerdown={(e) => onNotePointerDown(e, note)}
            >
              <span class="dot" style="background:{note.color}"></span>
              <span class="title" title={note.title}>{note.title || '未命名花笺'}</span>
              <span class="actions">
                <button
                  class="icon-btn tiny"
                  title="删除花笺"
                  onclick={(e) => {
                    e.stopPropagation();
                    confirmDelete = note.id;
                  }}
                >🗑</button>
              </span>
            </div>
          {/snippet}
        </VirtualList>
      {/if}
    </div>
  {/if}
</div>

{#if confirmDelete}
  <div class="modal-mask" onclick={() => (confirmDelete = null)}>
    <div class="modal" onclick={(e) => e.stopPropagation()}>
      <h3>删除花笺</h3>
      <p style="margin:0 0 14px">确定删除「{noteById(box.id, confirmDelete)?.title || '未命名花笺'}」？该操作会从花匣中移除。</p>
      <div class="modal-actions">
        <button class="ghost-btn" onclick={() => (confirmDelete = null)}>取消</button>
        <button class="primary-btn" style="background:var(--danger)" onclick={() => confirmDelete && delNote(confirmDelete)}>删除</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .box {
    border-bottom: 1px solid var(--border);
  }
  .box-head {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 34px;
    padding: 0 8px 0 10px;
    cursor: pointer;
    border-radius: 8px;
    margin: 2px 4px 0;
    user-select: none;
  }
  .box-head:hover {
    background: var(--bg-hover);
  }
  .caret {
    color: var(--text-dim);
    font-size: 11px;
    transition: transform 0.12s;
    width: 12px;
    flex: none;
  }
  .caret.open {
    transform: rotate(90deg);
  }
  .box-name {
    font-weight: 600;
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    color: var(--text-dim);
    font-size: 12px;
  }
  .actions {
    display: none;
    gap: 2px;
  }
  .box-head:hover .actions {
    display: inline-flex;
  }
  .notes {
    padding-bottom: 4px;
  }
  .note-row {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 8px 0 30px;
    cursor: pointer;
    border-radius: 8px;
    margin: 0 4px;
    user-select: none;
    position: relative;
  }
  .note-row:hover {
    background: var(--bg-hover);
  }
  .note-row.active {
    background: var(--bg-active);
  }
  .note-row.dragging {
    opacity: 0.45;
  }
  /* 落点指示线：悬停目标行的上/下缘 */
  .note-row.drop-above::before,
  .note-row.drop-below::after {
    content: '';
    position: absolute;
    left: 6px;
    right: 6px;
    height: 2px;
    border-radius: 1px;
    background: var(--accent);
    pointer-events: none;
  }
  .note-row.drop-above::before {
    top: -1px;
  }
  .note-row.drop-below::after {
    bottom: -1px;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex: none;
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.12);
  }
  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .note-row .actions {
    display: none;
  }
  .note-row:hover .actions {
    display: inline-flex;
  }
  .icon-btn.tiny {
    width: 20px;
    height: 20px;
    font-size: 12px;
  }
  .empty {
    color: var(--text-dim);
    font-size: 12px;
    padding: 6px 8px 10px 30px;
  }
</style>
