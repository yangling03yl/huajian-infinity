<script lang="ts">
  import type { BoxInfo, NoteMeta } from '../lib/types';
  import { activeTab, noteById, openNote, removeBoxUI, removeNoteUI, refreshNotes, reorderNotesUI, toggleBox } from '../lib/stores';
  import { api, errMsg } from '../lib/api';
  import { nextColor } from '../lib/colors';
  import { NOTE_MIME } from '../lib/dnd';
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
  /** 花笺排序拖拽：悬停目标行 + 插入位置（上缘/下缘） */
  let dropTarget = $state<{ id: string; pos: 'above' | 'below' } | null>(null);

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

  // ---- 侧边栏花笺拖拽排序（仅限同一花匣内） ----

  /** 正在拖拽的花笺 id（只在发起拖拽的本花匣实例中有值，天然禁止跨花匣排序） */
  let dragNoteId = $state<string | null>(null);

  function isNoteDrag(e: DragEvent): boolean {
    return e.dataTransfer?.types.includes(NOTE_MIME) ?? false;
  }

  function onNoteDragStart(e: DragEvent, note: NoteMeta): void {
    // 阻止冒泡到花匣槽位，避免误触发花匣排序
    e.stopPropagation();
    dragNoteId = note.id;
    dropTarget = null;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = 'move';
      e.dataTransfer.setData(NOTE_MIME, JSON.stringify({ boxId: box.id, noteId: note.id }));
      e.dataTransfer.setData('text/plain', note.title);
    }
  }

  function onNoteDragOver(e: DragEvent, note: NoteMeta): void {
    if (!isNoteDrag(e)) return; // 花匣拖拽：不拦截，交给上层槽位处理
    e.stopPropagation();
    if (!dragNoteId || dragNoteId === note.id) {
      if (dropTarget?.id === note.id) dropTarget = null;
      return; // 悬停在拖拽源行上：不给落下指示
    }
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    dropTarget = {
      id: note.id,
      pos: e.clientY < rect.top + rect.height / 2 ? 'above' : 'below',
    };
  }

  function onNoteDragLeave(e: DragEvent, note: NoteMeta): void {
    // 在行内子元素间移动时不清除指示线
    const to = e.relatedTarget as Node | null;
    if (to && (e.currentTarget as Node).contains(to)) return;
    if (dropTarget?.id === note.id) dropTarget = null;
  }

  function onNoteDrop(e: DragEvent, note: NoteMeta): void {
    if (!isNoteDrag(e)) return;
    e.preventDefault();
    e.stopPropagation();
    const src = dragNoteId;
    const target = dropTarget;
    dragNoteId = null;
    dropTarget = null;
    if (!src || !target || target.id !== note.id || src === note.id) return;
    commitOrder(moveNote(notes, src, target.id, target.pos));
  }

  function onNoteDragEnd(): void {
    dragNoteId = null;
    dropTarget = null;
  }

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
  <div class="box-head" role="button" tabindex="-1" onclick={() => toggleBox(box.id)}>
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
    <div class="notes">
      {#if notes.length === 0}
        <div class="empty">空花匣，点 ＋ 新建花笺</div>
      {:else}
        <VirtualList count={notes.length}>
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
              draggable="true"
              onclick={() => openNote(box.id, note.id)}
              ondblclick={() => onRenameRequest(box.id, note.id)}
              ondragstart={(e) => onNoteDragStart(e, note)}
              ondragover={(e) => onNoteDragOver(e, note)}
              ondragleave={(e) => onNoteDragLeave(e, note)}
              ondrop={(e) => onNoteDrop(e, note)}
              ondragend={onNoteDragEnd}
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
