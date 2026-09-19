<script lang="ts">
  import { onMount } from 'svelte';
  import { getVersion } from '@tauri-apps/api/app';
  import { activeTab, boxById, saveState, statusMsg, statusHint, boxes, wordCount } from '../lib/stores';
  import { isLight } from '../lib/colors';
  import PomodoroStatus from './PomodoroStatus.svelte';

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
  <div class="statusbar-inner">
    <span class="left" title={activeBox?.info.path}>
    {#if activeBox}
      <span class="dot" style="background:{$activeTab ? (activeBox.notes.find(n => n.id === $activeTab?.noteId)?.color ?? '#888') : '#888'}"></span>
      {activeBox.info.name}
      <span class="dim">· {noteCount} 篇花笺</span>
    {:else}
      <span class="dim">未打开花匣</span>
    {/if}
  </span>
  <span class="mid">
    {#if $statusHint}
      <span class="hint" title={$statusHint}>{$statusHint}</span>
    {/if}
  </span>
  <span class="right">
    {#if $statusMsg}
      <span class="msg" title={$statusMsg}>{$statusMsg}</span>
    {/if}
    <PomodoroStatus />
    {#if $activeTab}
      {#if $wordCount !== null}
        <span class="count" title="汉字、字母、数字、标点各计 1，不含空格换行">{$wordCount} 字</span>
      {:else}
        <span class="count" title="正在载入花笺…">-字</span>
      {/if}
    {:else}
      <span class="count" title="未打开花笺">-字</span>
    {/if}
    {#if saveText}<span class="save-text {saveText}">{saveText}</span>{/if}
    <span class="dim">花笺infinity {ver}</span>
    </span>
  </div>
</footer>

<style>
  .statusbar {
    height: 26px;
    font-size: 12px;
    background: var(--bg-sidebar);
    border-top: 1px solid var(--border);
    flex: none;
    user-select: none;
  }
  /* 状态栏固定占满编辑区宽度，不随正文宽度模式变化；左侧内缩仍与正文对齐 */
  .statusbar-inner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 100%;
    padding: 0 var(--editor-inset, 12px);
    box-sizing: border-box;
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
  /* 中段：通用状态提示（进度类），无内容时不显示任何东西 */
  .mid {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }
  .hint {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-dim);
  }
  .msg {
    max-width: 300px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--accent);
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
