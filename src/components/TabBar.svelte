<script lang="ts">
  import { tabs, activeTab, openNote, closeTab, patchNote, boxById, refreshNotes } from '../lib/stores';
  import { api, errMsg } from '../lib/api';
  import { nextColor } from '../lib/colors';
  import type { Tab } from '../lib/types';
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

  $effect(() => {
    if (renameTarget) {
      editing = { ...renameTarget };
      const box = boxById(renameTarget.boxId);
      editText = box?.notes.find((n) => n.id === renameTarget.noteId)?.title ?? '';
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
    const box = boxById(tab.boxId);
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

  function closeCurrentTab(t: Tab): void {
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
</script>

<div class="tabbar">
  <div class="tabs">
    {#each $tabs as t (t.boxId + t.noteId)}
      {@const isActive = $activeTab?.boxId === t.boxId && $activeTab?.noteId === t.noteId}
      {@const isEditing = editing?.boxId === t.boxId && editing?.noteId === t.noteId}
      <div
        class="tab {isActive ? 'active' : ''}"
        role="button"
        tabindex="-1"
        onclick={() => openNote(t.boxId, t.noteId)}
      >
        <span class="tabs-dot" style="background:{boxById(t.boxId)?.notes.find(n => n.id === t.noteId)?.color || '#888'}"></span>
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
              editText = boxById(t.boxId)?.notes.find((n) => n.id === t.noteId)?.title ?? '';
            }}
          >{boxById(t.boxId)?.notes.find((n) => n.id === t.noteId)?.title || '未命名花笺'}</span>
        {/if}
        {#if t.dirty}<span class="dirty" title="未保存">●</span>{/if}
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
      <button class="icon-btn new-tab" title="新建花笺" onclick={newNote}>＋</button>
    {/if}
  </div>

  {#if colorFor}
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
    padding: 0 6px 0 12px;
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
  .tabs-dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    flex: none;
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.12);
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
  .dirty {
    color: var(--accent);
    font-size: 9px;
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
</style>
