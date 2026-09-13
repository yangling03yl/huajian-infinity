<script lang="ts">
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { activeTab, tabs, setTabDirty, saveState, boxById, showHint } from '../lib/stores';
  import { api, errMsg } from '../lib/api';
  import { createEditor, type EditorInstance } from '../lib/editor';
  import { buildPdf, bytesToBase64, strToBase64, safeFileName } from '../lib/export';
  import type { SnapshotMeta } from '../lib/types';
  import Toolbar from './Toolbar.svelte';
  import DiffPane from './DiffPane.svelte';
  import SnapshotPanel from './SnapshotPanel.svelte';

  interface SplitSession {
    meta: SnapshotMeta;
    snapshotMd: string;
    /** 复制快照会话可合并/放弃；查看历史差异时只读对比 */
    canMerge: boolean;
  }

  let container = $state<HTMLDivElement>();
  let editor = $state<EditorInstance | null>(null);
  let activeKey = '';
  /** 当前已持久化的 md（以 activeKey 为键，切换标签时仍可正确 flush） */
  let savedMdRef = '';
  let loading = $state(false);
  let split = $state<SplitSession | null>(null);
  let showSnapshots = $state(false);
  /** 编辑器当前内容（节流更新），用于实时差异 */
  let liveMd = $state('');
  let liveTimer: ReturnType<typeof setTimeout> | null = null;

  /** 把当前编辑器内容写回后端；以 activeKey 定位，不依赖 activeTab 是否已切换 */
  async function flush(): Promise<void> {
    if (!editor || !activeKey) return;
    const [boxId, noteId] = activeKey.split('/');
    const md = editor.getMarkdown();
    if (md === savedMdRef) return;
    saveState.set('saving');
    try {
      await api.writeNote(boxId, noteId, md);
      savedMdRef = md;
      const t = get(tabs).find((x) => x.boxId === boxId && x.noteId === noteId);
      if (t) setTabDirty(t, false, md);
      saveState.set('saved');
    } catch (e) {
      saveState.set('error');
    }
  }

  /** 编辑器内容即时回调（节流 200ms），驱动右侧实时差异 */
  function onLive(md: string): void {
    if (liveTimer) clearTimeout(liveTimer);
    liveTimer = setTimeout(() => {
      liveMd = md;
    }, 200);
  }

  function exitSplit(): void {
    split = null;
    showSnapshots = false;
    if (liveTimer) clearTimeout(liveTimer);
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
    exitSplit();
    liveMd = '';
    if (!tab) {
      activeKey = '';
      return;
    }
    activeKey = key;
    loading = true;
    try {
      const md = await api.readNote(tab.boxId, tab.noteId);
      if (!container) return;
      liveMd = md;
      savedMdRef = md;
      const myKey = key;
      editor = await createEditor(container, md, async (newMd) => {
        const [boxId, noteId] = myKey.split('/');
        saveState.set('saving');
        try {
          await api.writeNote(boxId, noteId, newMd);
          if (activeKey !== myKey) return;
          savedMdRef = newMd;
          const t = get(tabs).find((x) => x.boxId === boxId && x.noteId === noteId);
          if (t) setTabDirty(t, false, newMd);
          saveState.set('saved');
        } catch (e) {
          saveState.set('error');
        }
      }, onLive);
      saveState.set('saved');
    } catch (e) {
      alert(`读取花笺失败: ${errMsg(e)}`);
    } finally {
      loading = false;
    }
  }

  /** 复制当前正文为快照并进入二分屏对比 */
  async function createSnapshot(): Promise<void> {
    const tab = get(activeTab);
    if (!tab || !editor) return;
    await flush();
    const md = editor.getMarkdown();
    try {
      const meta = await api.createSnapshot(tab.boxId, tab.noteId, null);
      split = { meta, snapshotMd: md, canMerge: true };
      liveMd = md;
    } catch (e) {
      alert(`复制快照失败: ${errMsg(e)}`);
    }
  }

  /** 查看某个快照与当前正文的差异 */
  async function compareSnapshot(meta: SnapshotMeta): Promise<void> {
    const tab = get(activeTab);
    if (!tab || !editor) return;
    try {
      const md = await api.readSnapshot(tab.boxId, tab.noteId, meta.id);
      split = { meta, snapshotMd: md, canMerge: false };
      liveMd = editor.getMarkdown();
    } catch (e) {
      alert(`读取快照失败: ${errMsg(e)}`);
    }
  }

  /** 恢复快照版本为正文 */
  async function restoreSnapshot(meta: SnapshotMeta): Promise<void> {
    const tab = get(activeTab);
    if (!tab || !editor) return;
    try {
      const body = await api.applySnapshot(tab.boxId, tab.noteId, meta.id);
      editor.setMarkdown(body);
      setTabDirty(tab, false, body);
      savedMdRef = body;
      liveMd = body;
      split = null;
      saveState.set('saved');
    } catch (e) {
      alert(errMsg(e));
    }
  }

  /** 合并：保留左侧修改为正文，快照继续保存在历史中 */
  async function mergeSplit(): Promise<void> {
    if (!split || !editor) return;
    const snap = split;
    split = null;
    await flush();
  }

  /** 放弃左侧修改，恢复为快照内容 */
  async function discardSplit(): Promise<void> {
    if (!split || !editor) return;
    const snap = split;
    split = null;
    editor.setMarkdown(snap.snapshotMd);
    liveMd = snap.snapshotMd;
    await flush();
  }

  function toggleSnapshots(): void {
    showSnapshots = !showSnapshots;
  }

  /** 导出：仅正文，丢弃快照与目录 */
  async function exportNote(kind: 'md' | 'pdf'): Promise<void> {
    const tab = get(activeTab);
    if (!tab || !editor) return;
    await flush();
    const md = editor.getMarkdown();
    const box = boxById(tab.boxId);
    const title = box?.notes.find((n) => n.id === tab.noteId)?.title || '未命名花笺';
    const base = safeFileName(title);
    try {
      if (kind === 'md') {
        const path = await saveDialog({
          title: '导出 Markdown（仅正文）',
          defaultPath: `${base}.md`,
          filters: [{ name: 'Markdown', extensions: ['md'] }],
        });
        if (!path) return;
        await api.writeBytes(path, strToBase64(md));
        showHint(`已导出 Markdown：${path}`);
      } else {
        const path = await saveDialog({
          title: '导出 PDF（仅正文）',
          defaultPath: `${base}.pdf`,
          filters: [{ name: 'PDF', extensions: ['pdf'] }],
        });
        if (!path) return;
        // buildPdf 首行会 await 字体加载，提示能先绘制出来
        showHint('正在生成 PDF…');
        const bytes = await buildPdf(md, title);
        await api.writeBytes(path, bytesToBase64(bytes));
        showHint(`已导出 PDF：${path}`);
      }
    } catch (e) {
      alert(`导出失败: ${errMsg(e)}`);
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
  <Toolbar
    {editor}
    snapshotsOpen={showSnapshots}
    onCreateSnapshot={() => void createSnapshot()}
    onToggleSnapshots={toggleSnapshots}
    onExport={(k) => void exportNote(k)}
  />
  <div class="editor-body">
    <div class="editor-wrap">
      <div class="editor-main">
        <div bind:this={container} class="editor-container"></div>
        {#if loading}
          <div class="loading-hint">载入中…</div>
        {/if}
      </div>
      {#if split}
        <DiffPane
          meta={split.meta}
          snapshotMd={split.snapshotMd}
          currentMd={liveMd}
          canMerge={split.canMerge}
          onMerge={() => void mergeSplit()}
          onDiscard={() => void discardSplit()}
          onClose={() => (split = null)}
        />
      {/if}
    </div>
    {#if showSnapshots}
      <SnapshotPanel
        onCreateSnapshot={() => void createSnapshot()}
        onCompare={(m) => void compareSnapshot(m)}
        onRestored={(m) => restoreSnapshot(m)}
        onClose={() => (showSnapshots = false)}
      />
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
  .editor-body {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .editor-wrap {
    flex: 1;
    min-width: 0;
    display: flex;
    background: var(--bg);
  }
  .editor-main {
    position: relative;
    flex: 1;
    min-width: 0;
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
