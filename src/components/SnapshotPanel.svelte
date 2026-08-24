<script lang="ts">
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { activeTab } from '../lib/stores';
  import { api, errMsg } from '../lib/api';
  import type { SnapshotMeta } from '../lib/types';

  interface Props {
    onCreateSnapshot: () => void;
    onCompare: (meta: SnapshotMeta) => void;
    onRestored: (meta: SnapshotMeta) => void;
    onClose: () => void;
  }
  let { onCreateSnapshot, onCompare, onRestored, onClose }: Props = $props();

  let snapshots = $state<SnapshotMeta[]>([]);
  let loading = $state(true);
  let confirmRestore = $state<SnapshotMeta | null>(null);
  let confirmDelete = $state<SnapshotMeta | null>(null);
  let busy = $state(false);

  async function refresh(): Promise<void> {
    const tab = get(activeTab);
    if (!tab) {
      snapshots = [];
      loading = false;
      return;
    }
    loading = true;
    try {
      snapshots = (await api.listSnapshots(tab.boxId, tab.noteId)).reverse();
    } catch (e) {
      snapshots = [];
      alert(errMsg(e));
    } finally {
      loading = false;
    }
  }

  async function restore(meta: SnapshotMeta): Promise<void> {
    confirmRestore = null;
    busy = true;
    try {
      await onRestored(meta);
      await refresh();
    } finally {
      busy = false;
    }
  }

  async function remove(meta: SnapshotMeta): Promise<void> {
    confirmDelete = null;
    const tab = get(activeTab);
    if (!tab) return;
    busy = true;
    try {
      await api.deleteSnapshot(tab.boxId, tab.noteId, meta.id);
      await refresh();
    } catch (e) {
      alert(errMsg(e));
    } finally {
      busy = false;
    }
  }

  function fmtTime(iso: string): string {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return iso;
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }

  onMount(() => {
    void refresh();
  });
</script>

<div class="snap-panel">
  <header class="panel-head">
    <span class="title">快照记录</span>
    <button class="icon-btn" title="关闭" onclick={onClose}>×</button>
  </header>

  <div class="panel-actions">
    <button class="btn primary" title="把当前正文保存为一个新快照" onclick={onCreateSnapshot}>⧉ 复制当前快照</button>
  </div>

  <div class="list">
    {#if loading}
      <div class="hint">载入中…</div>
    {:else if snapshots.length === 0}
      <div class="hint">还没有快照<br />点击「复制当前快照」保存当前正文为第一个版本</div>
    {:else}
      {#each snapshots as s (s.id)}
        <div class="snap-item">
          <div class="snap-meta">
            <span class="ver">v{s.version}</span>
            <div class="texts">
              <span class="label">{s.label}</span>
              <span class="time">{fmtTime(s.created_at)}</span>
            </div>
          </div>
          <div class="snap-actions">
            <button class="icon-btn" title="与当前正文对比" onclick={() => onCompare(s)}>⇄</button>
            <button class="icon-btn" title="恢复此版本为正文" onclick={() => (confirmRestore = s)}>↩</button>
            <button class="icon-btn danger" title="删除此快照" onclick={() => (confirmDelete = s)}>🗑</button>
          </div>
        </div>
      {/each}
    {/if}
  </div>

  {#if busy}
    <div class="hint busy">处理中…</div>
  {/if}
</div>

{#if confirmRestore}
  <div class="modal-mask" onclick={() => (confirmRestore = null)}>
    <div class="modal" onclick={(e) => e.stopPropagation()}>
      <h3>恢复快照</h3>
      <p style="margin:0 0 14px">将正文恢复为「{confirmRestore.label}」（v{confirmRestore.version}）？当前正文将被覆盖，该快照本身保留。</p>
      <div class="modal-actions">
        <button class="ghost-btn" onclick={() => (confirmRestore = null)}>取消</button>
        <button class="primary-btn" onclick={() => restore(confirmRestore!)}>恢复</button>
      </div>
    </div>
  </div>
{/if}

{#if confirmDelete}
  <div class="modal-mask" onclick={() => (confirmDelete = null)}>
    <div class="modal" onclick={(e) => e.stopPropagation()}>
      <h3>删除快照</h3>
      <p style="margin:0 0 14px">确定删除「{confirmDelete.label}」（v{confirmDelete.version}）？删除后不可恢复。</p>
      <div class="modal-actions">
        <button class="ghost-btn" onclick={() => (confirmDelete = null)}>取消</button>
        <button class="primary-btn" style="background:var(--danger)" onclick={() => remove(confirmDelete!)}>删除</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .snap-panel {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: 300px;
    display: flex;
    flex-direction: column;
    background: var(--bg-panel);
    border-left: 1px solid var(--border);
    box-shadow: -8px 0 24px rgba(0, 0, 0, 0.12);
    z-index: 30;
  }
  .panel-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 10px 8px 14px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .title {
    font-size: 13px;
    font-weight: 700;
    color: var(--accent);
  }
  .panel-actions {
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .btn {
    font: inherit;
    font-size: 12.5px;
    padding: 5px 12px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--bg);
    color: var(--text);
    cursor: pointer;
    width: 100%;
  }
  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
    font-weight: 600;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 6px 8px 12px;
  }
  .snap-item {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 6px 7px 10px;
    border-radius: 6px;
    margin-bottom: 2px;
  }
  .snap-item:hover {
    background: var(--bg-hover);
  }
  .snap-meta {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .ver {
    flex: none;
    font-size: 11px;
    font-weight: 700;
    color: var(--accent);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 9px;
    padding: 1px 8px;
  }
  .texts {
    min-width: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .label {
    font-size: 13px;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .time {
    font-size: 11px;
    color: var(--text-dim);
  }
  .snap-actions {
    display: none;
    gap: 2px;
    flex: none;
  }
  .snap-item:hover .snap-actions {
    display: inline-flex;
  }
  .icon-btn.danger:hover {
    color: var(--danger);
    border-color: var(--danger);
  }
  .hint {
    padding: 26px 14px;
    text-align: center;
    color: var(--text-dim);
    font-size: 12.5px;
    line-height: 1.8;
  }
  .hint.busy {
    padding: 4px;
    border-top: 1px solid var(--border);
  }
  .modal-mask {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 80;
  }
  .modal {
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 20px;
    width: 360px;
    max-width: 90vw;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.3);
  }
  .modal h3 {
    margin: 0 0 10px;
    font-size: 16px;
  }
  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .ghost-btn,
  .primary-btn {
    font: inherit;
    font-size: 13px;
    padding: 6px 16px;
    border-radius: 5px;
    cursor: pointer;
  }
  .ghost-btn {
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
  }
  .primary-btn {
    border: 1px solid var(--accent);
    background: var(--accent);
    color: #fff;
    font-weight: 600;
  }
</style>
