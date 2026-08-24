<script lang="ts">
  import { onMount } from 'svelte';
  import { getVersion } from '@tauri-apps/api/app';
  import { activeTab, boxById, saveState, statusMsg, boxes, wordCount } from '../lib/stores';
  import { isLight } from '../lib/colors';

  const activeBox = $derived($activeTab ? boxById($activeTab.boxId) : null);
  const noteCount = $derived($activeTab ? activeBox?.info.note_count ?? 0 : 0);
  const saveText = $derived(
    $saveState === 'saving' ? '保存中…' : $saveState === 'saved' ? '已保存' : $saveState === 'error' ? '保存失败' : '',
  );

  let ver = $state('');

  onMount(() => {
    void getVersion().then((v) => (ver = `V${v}`));
  });
</script>

<footer class="statusbar">
  <span class="left" title={activeBox?.info.path}>
    {#if activeBox}
      <span class="dot" style="background:{$activeTab ? (activeBox.notes.find(n => n.id === $activeTab?.noteId)?.color ?? '#888') : '#888'}"></span>
      {activeBox.info.name}
      <span class="dim">· {noteCount} 篇花笺</span>
    {:else}
      <span class="dim">未打开花匣</span>
    {/if}
    {#if $statusMsg}
      <span class="dim msg">{ $statusMsg }</span>
    {/if}
  </span>
  <span class="right">
    {#if $activeTab}<span class="count" title="汉字、字母、数字、标点各计 1，不含空格换行">{$wordCount} 字</span>{/if}
    {#if saveText}<span class="save-text {saveText}">{saveText}</span>{/if}
    <span class="dim">花笺infinity {ver}</span>
  </span>
</footer>

<style>
  .statusbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 26px;
    padding: 0 12px;
    font-size: 12px;
    background: var(--bg-sidebar);
    border-top: 1px solid var(--border);
    flex: none;
    user-select: none;
  }
  .left {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex: none;
  }
  .dim {
    color: var(--text-dim);
  }
  .msg {
    margin-left: 8px;
    font-style: normal;
  }
  .right {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: none;
  }
  .save-text {
    color: var(--accent);
  }
  .count {
    color: var(--text-dim);
  }
  .save-text.saved {
    color: #5b8c5a;
  }
  .save-text.error {
    color: var(--danger);
  }
</style>
