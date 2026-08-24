<script lang="ts">
import { onMount } from 'svelte';
import { get } from 'svelte/store';
import { activeTab, setTabDirty, saveState, statusMsg } from '../lib/stores';
import { api, errMsg } from '../lib/api';
import { createEditor, type EditorInstance } from '../lib/editor';
import Toolbar from './Toolbar.svelte';

  let container = $state<HTMLDivElement>();
  let editor = $state<EditorInstance | null>(null);
  let activeKey = '';
  let loading = $state(false);

  async function flush(): Promise<void> {
    if (!editor || !activeKey) return;
    const tab = get(activeTab);
    if (!tab || `${tab.boxId}/${tab.noteId}` !== activeKey) return;
    const md = editor.getMarkdown();
    if (md === tab.savedMd) return;
    saveState.set('saving');
    try {
      await api.writeNote(tab.boxId, tab.noteId, md);
      setTabDirty(tab, false, md);
      saveState.set('saved');
    } catch (e) {
      saveState.set('error');
      statusMsg.set(`保存失败: ${errMsg(e)}`);
    }
  }

  async function handleTabChange(tab: { boxId: string; noteId: string } | null): Promise<void> {
    const key = tab ? `${tab.boxId}/${tab.noteId}` : '';
    if (key === activeKey && key !== '') return;
    await flush();
    if (editor) {
      const old = editor;
      editor = null;
      await old.destroy();
    }
    container?.replaceChildren();
    if (!tab) {
      activeKey = '';
      return;
    }
    activeKey = key;
    loading = true;
    try {
      const md = await api.readNote(tab.boxId, tab.noteId);
      if (!container) return;
      editor = await createEditor(container, md, async (newMd) => {
        const t = get(activeTab);
        if (!t || `${t.boxId}/${t.noteId}` !== activeKey) return;
        setTabDirty(t, true);
        saveState.set('saving');
        try {
          await api.writeNote(t.boxId, t.noteId, newMd);
          setTabDirty(t, false, newMd);
          saveState.set('saved');
        } catch (e) {
          saveState.set('error');
          statusMsg.set(`保存失败: ${errMsg(e)}`);
        }
      });
      saveState.set('saved');
    } catch (e) {
      statusMsg.set(`读取花笺失败: ${errMsg(e)}`);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    const unsub = activeTab.subscribe((tab) => {
      void handleTabChange(tab);
    });

    return () => {
      unsub();
      void flush();
      void editor?.destroy();
    };
  });
</script>

<div class="editor-pane">
  <Toolbar {editor} />
  <div class="editor-wrap">
    <div bind:this={container} class="editor-container"></div>
    {#if loading}
      <div class="loading-hint">载入中…</div>
    {/if}
  </div>
</div>

<style>
  .editor-pane {
    flex: 1;
    min-width: 0;
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .editor-wrap {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: auto;
    background: var(--bg);
  }
  .editor-container {
    max-width: 860px;
    margin: 0 auto;
    padding: 28px 40px 80px;
    min-height: 100%;
    user-select: text;
  }
  .loading-hint {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-dim);
    pointer-events: none;
  }
</style>
