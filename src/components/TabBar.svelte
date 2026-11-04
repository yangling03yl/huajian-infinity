<script lang="ts">
  import { get } from 'svelte/store';
  import { tabs, activeTab, openNote, closeTab, patchNote, boxes, refreshNotes } from '../lib/stores';
  import { api, errMsg } from '../lib/api';
  import { nextColor } from '../lib/colors';
  import type { NoteMeta } from '../lib/types';
  import ColorPicker from './ColorPicker.svelte';

  interface Props {
    renameTarget: { boxId: string; noteId: string } | null;
    onRenameDone: () => void;
  }
  let { renameTarget, onRenameDone }: Props = $props();

  let editing = $state<{ boxId: string; noteId: string } | null>(null);
  let editText = $state('');
  let colorFor = $state<{ boxId: string; noteId: string } | null>(null);
  let titleInput: HTMLInputElement | undefined = $state();

  /**
   * 花笺元信息的响应式索引。
   *
   * 不能再用 `boxById()`（内部是 `get(boxes)`）：`get()` 不参与 Svelte 的依赖收集，
   * 于是改名/改色后标签栏不会重渲染，用户会看到侧栏已更新、标签还是旧标题。
   * 这里显式读 `$boxes`，任何一次 patchNote 都能驱动更新。
   */
  const noteMap = $derived(
    new Map<string, NoteMeta>(
      $boxes.flatMap((b) => b.notes.map((n) => [`${b.info.id}/${n.id}`, n] as const)),
    ),
  );

  function noteOf(boxId: string, noteId: string): NoteMeta | undefined {
    return noteMap.get(`${boxId}/${noteId}`);
  }

  /**
   * 取标题但**不建立响应式依赖**：改名输入框正在编辑时，如果 boxes 因刷新而变化
   * 重新触发这个 effect，就会把用户正在输入的内容重置掉。
   */
  function snapshotTitle(boxId: string, noteId: string): string {
    return (
      get(boxes).find((b) => b.info.id === boxId)?.notes.find((n) => n.id === noteId)?.title ?? ''
    );
  }

  $effect(() => {
    if (renameTarget) {
      editing = { ...renameTarget };
      editText = snapshotTitle(renameTarget.boxId, renameTarget.noteId);
    }
  });

  $effect(() => {
    if (editing) titleInput?.focus();
  });

  async function commitRename(): Promise<void> {
    const target = editing;
    editing = null;
    onRenameDone();
    if (!target) return;
    const title = editText.trim();
    if (!title) return;
    if (title === noteOf(target.boxId, target.noteId)?.title) return;
    try {
      await api.renameNote(target.boxId, target.noteId, title);
      patchNote(target.boxId, target.noteId, { title });
    } catch (e) {
      alert(errMsg(e));
    }
  }

  async function changeColor(boxId: string, noteId: string, color: string): Promise<void> {
    colorFor = null;
    try {
      await api.setNoteColor(boxId, noteId, color);
      patchNote(boxId, noteId, { color });
    } catch (e) {
      alert(errMsg(e));
    }
  }

  async function newNote(): Promise<void> {
    const tab = $activeTab;
    if (!tab) return;
    const box = $boxes.find((b) => b.info.id === tab.boxId);
    if (!box) return;
    const color = nextColor(box.notes.map((n) => n.color));
    try {
      const meta = await api.createNote(tab.boxId, '未命名花笺', color);
      await refreshNotes(tab.boxId);
      openNote(tab.boxId, meta.id);
      onRenameDone();
      editing = { boxId: tab.boxId, noteId: meta.id };
      editText = '未命名花笺';
    } catch (e) {
      alert(errMsg(e));
    }
  }

  function closeCurrentTab(t: { boxId: string; noteId: string }): void {
    if (editing && editing.boxId === t.boxId && editing.noteId === t.noteId) {
      editing = null;
      onRenameDone();
    }
    closeTab(t);
  }

  function pickColor(c: string): void {
    const target = colorFor;
    colorFor = null;
    if (target) void changeColor(target.boxId, target.noteId, c);
  }

  function openColorPicker(boxId: string, noteId: string): void {
    if (!noteOf(boxId, noteId)) return;
    colorFor = { boxId, noteId };
  }
</script>

<div class="tabbar">
  <div class="tabs">
    {#each $tabs as t (t.boxId + t.noteId)}
      {@const isActive = $activeTab?.boxId === t.boxId && $activeTab?.noteId === t.noteId}
      {@const isEditing = editing?.boxId === t.boxId && editing?.noteId === t.noteId}
      {@const note = noteOf(t.boxId, t.noteId)}
      <div
        class="tab {isActive ? 'active' : ''}"
        role="button"
        tabindex="-1"
        onclick={() => openNote(t.boxId, t.noteId)}
      >
        <button
          class="dot-btn"
          type="button"
          title="修改花笺颜色"
          aria-label="修改花笺颜色"
          onclick={(e) => {
            e.stopPropagation();
            openColorPicker(t.boxId, t.noteId);
          }}
        >
          <span class="tabs-dot" style="background:{note?.color || '#888'}"></span>
        </button>
        {#if isEditing}
          <input
            bind:this={titleInput}
            bind:value={editText}
            class="tab-title-input"
            onkeydown={(e) => {
              if (e.key === 'Enter') void commitRename();
              if (e.key === 'Escape') {
                editing = null;
                onRenameDone();
              }
            }}
            onblur={() => void commitRename()}
          />
        {:else}
          <span
            class="tab-title"
            ondblclick={() => {
              editing = { boxId: t.boxId, noteId: t.noteId };
              editText = note?.title ?? '';
            }}
          >{note?.title || '未命名花笺'}</span>
        {/if}
        <button
          class="icon-btn tab-close"
          title="关闭"
          onclick={(e) => {
            e.stopPropagation();
            closeCurrentTab(t);
          }}
        >×</button>
      </div>
    {/each}
    {#if $activeTab}
      <button
        class="icon-btn new-tab"
        title="新建花笺"
        onclick={() => void newNote()}
      >＋</button>
    {/if}
  </div>

  {#if colorFor}
    <div class="color-mask" role="presentation" onclick={() => (colorFor = null)}></div>
    <ColorPicker onPick={pickColor} onClose={() => (colorFor = null)} />
  {/if}
</div>

<style>
  .tabbar {
    position: relative;
    display: flex;
    align-items: stretch;
    height: 38px;
    background: var(--bg-panel);
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .tabs {
    display: flex;
    align-items: stretch;
    overflow-x: auto;
    flex: 1;
    min-width: 0;
    scrollbar-width: thin;
  }
  .tabs::-webkit-scrollbar {
    height: 4px;
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 0 6px 0 8px;
    border-right: 1px solid var(--border);
    color: var(--text-dim);
    cursor: pointer;
    max-width: 220px;
    white-space: nowrap;
    user-select: none;
    position: relative;
  }
  .tab:hover {
    background: var(--bg-hover);
  }
  .tab.active {
    background: var(--bg);
    color: var(--text);
    box-shadow: inset 0 2px 0 var(--accent);
  }
  /* 圆点是「改颜色」的入口：单独包一层按钮，点击不会触发切换标签 */
  .dot-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 16px;
    height: 16px;
    padding: 0;
    border: none;
    background: transparent;
    cursor: pointer;
    border-radius: 50%;
  }
  .dot-btn:hover .tabs-dot {
    transform: scale(1.25);
  }
  .tabs-dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.12);
    transition: transform 0.1s;
  }
  .tab-title {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tab-title-input {
    border: 1px solid var(--accent);
    border-radius: 5px;
    background: var(--bg);
    color: var(--text);
    outline: none;
    padding: 2px 6px;
    font-size: 13px;
    width: 140px;
  }
  .tab-close {
    width: 20px;
    height: 20px;
    font-size: 14px;
  }
  .new-tab {
    margin: 5px 8px;
    align-self: center;
  }
  .color-mask {
    position: fixed;
    inset: 0;
    z-index: 49;
  }
</style>
