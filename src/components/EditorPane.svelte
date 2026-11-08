<script lang="ts">
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { activeTab, saveState, boxById, showHint, wordCount, editorSel } from '../lib/stores';
  import {
    initHistory,
    recordModification,
    replaceCurrent,
    historyBack,
    historyForward,
    publishHistoryState,
    noteHistoryState,
  } from '../lib/note-history';
  import { api, errMsg } from '../lib/api';
  import { createEditor, type EditorInstance } from '../lib/editor';
  import { buildPdf, bytesToBase64, strToBase64, safeFileName } from '../lib/export';
  import { registerFlush } from '../lib/session';
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
  let historyMoving = $state(false);
  /**
   * 标签切换序列号。
   *
   * `handleTabChange` 是一条有 4 个 await 点的长异步链（flush → destroy → readNote →
   * createEditor），而 activeTab 的订阅不等待它。快速连点两个标签时，两次调用会交错，
   * 出现「activeKey 指向 B、editor 却是 C 的实例」——之后任意一次 flush 都会把 C 的
   * 正文写进 B。每个 await 之后用序列号判定自己是否已过期，过期就直接退出。
   */
  let tabSeq = 0;

  /**
   * 写入序号与串行队列。
   *
   * 自动保存（1.2s 去抖）与切换标签/关窗时的 flush 会并发触发写盘；此前没有排序，
   * 两次 IPC 的完成顺序不保证，先发起的旧内容可能后落盘，把新内容覆盖掉。
   * 现在所有写入串行执行（盘上的最后一次一定是最新排队的），且只有**最新序号**
   * 才允许更新保存基线与状态栏，旧写入不会把「已保存」倒拨回去。
   */
  let writeSeq = 0;
  let writeChain: Promise<void> = Promise.resolve();

  /** 只负责写盘与基线更新，返回是否写成功 */
  async function write(key: string, md: string): Promise<boolean> {
    const [boxId, noteId] = key.split('/');
    if (!boxId || !noteId) return false;

    const seq = ++writeSeq;
    saveState.set('saving');
    let ok = true;

    writeChain = writeChain.then(async () => {
      try {
        await api.writeNote(boxId, noteId, md);
        if (seq === writeSeq) {
          if (activeKey === key) savedMdRef = md;
          saveState.set('saved');
        }
      } catch (e) {
        ok = false;
        // 只有最新一次写入失败才报错，避免旧请求的失败覆盖掉已成功的新状态
        if (seq === writeSeq) {
          saveState.set('error');
          showHint(`保存失败：${errMsg(e)}`);
        }
      }
    });

    await writeChain;
    // 只有最新一次写入才算「这次保存成功」：旧写入不该再产生修改历史记录
    return ok && seq === writeSeq;
  }

  /** 保存一次内容并计入修改历史（内容与已保存的一致时什么都不做） */
  async function saveContent(key: string, md: string): Promise<void> {
    if (md === savedMdRef) return;
    if (!(await write(key, md))) return;
    // 写盘成功后才记入历史：写失败却留下记录会造出"从没落过盘"的幽灵版本
    recordModification(key, md);
    if (activeKey === key) publishHistoryState(key);
  }

  /** 把编辑器当前内容写回后端；上下文由参数显式给出，不读全局 */
  async function flush(key: string, ed: EditorInstance | null): Promise<void> {
    if (!ed || !key) return;
    // 丢弃排队中的去抖保存：它携带的是更早的内容，若在 flush 之后入队会覆盖新内容
    ed.cancelPendingSave();
    await saveContent(key, ed.getMarkdown());
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
    if (key === activeKey) return;
    const seq = ++tabSeq;

    // 1) 先在旧上下文里落盘
    const oldEditor = editor;
    const oldKey = activeKey;
    await flush(oldKey, oldEditor);
    if (seq !== tabSeq) return;

    // 2) 让旧上下文立即失效（顺序很重要：先清 activeKey，旧编辑器的去抖回调才会放弃）
    editor = null;
    activeKey = '';
    wordCount.set(null);
    editorSel.set({ heading: 0, fontSize: null });
    publishHistoryState('');
    if (oldEditor) {
      try {
        await oldEditor.destroy();
      } catch {
        // 编辑器已销毁：忽略
      }
    }
    if (seq !== tabSeq) return;

    container?.replaceChildren();
    exitSplit();
    liveMd = '';
    if (!tab) return;

    // 3) 载入新花笺
    loading = true;
    try {
      const md = await api.readNote(tab.boxId, tab.noteId);
      if (seq !== tabSeq) return;
      if (!container) return;
      const myKey = key;
      const ed = await createEditor(
        container,
        md,
        async (newMd) => {
          // 去抖触发的自动保存：写入目标由 myKey 固定，绝不使用会变化的全局 activeKey
          if (activeKey !== myKey) return;
          await saveContent(myKey, newMd);
        },
        onLive,
      );
      // 创建期间又切走了：丢弃这个编辑器，绝不能挂到当前上下文上
      if (seq !== tabSeq) {
        await ed.destroy();
        return;
      }
      editor = ed;
      activeKey = key;
      liveMd = md;
      // 以「编辑器序列化后的结果」作为保存基线：磁盘原文与 Milkdown 的归一化输出
      // 可能不同（列表序号、标题风格等），拿原文当基线会导致"切一次标签就改一次盘"
      savedMdRef = ed.getMarkdown();
      initHistory(key, savedMdRef);
      publishHistoryState(key);
      saveState.set('saved');
    } catch (e) {
      if (seq === tabSeq) {
        publishHistoryState('');
        alert(`读取花笺失败: ${errMsg(e)}`);
      }
    } finally {
      if (seq === tabSeq) loading = false;
    }
  }

  /** 复制当前正文为快照并进入二分屏对比 */
  async function createSnapshot(): Promise<void> {
    const tab = get(activeTab);
    if (!tab || !editor) return;
    await flush(activeKey, editor);
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
    const ed = editor;
    if (!tab || !ed || historyMoving) return;
    const key = activeKey;
    try {
      // 先落盘：否则 1.2s 去抖里排队的那次自动保存会在恢复之后触发，
      // 把刚恢复的正文又覆盖回去
      await flush(key, ed);
      if (activeKey !== key || editor !== ed) return;
      const body = await api.applySnapshot(tab.boxId, tab.noteId, meta.id);
      // IPC 期间可能已切换标签，防止把旧笔记的快照内容写进新笔记
      if (activeKey !== key || editor !== ed) return;
      ed.setMarkdown(body);
      const normalized = ed.getMarkdown();
      liveMd = normalized;
      if (!(await write(key, normalized))) return;
      // 快照恢复也是一次内容修改，同样记入退回/前进历史
      recordModification(key, normalized);
      publishHistoryState(key);
      split = null;
      saveState.set('saved');
    } catch (e) {
      alert(errMsg(e));
    }
  }

  /** 合并：保留左侧修改为正文，快照继续保存在历史中 */
  async function mergeSplit(): Promise<void> {
    if (!split || !editor) return;
    split = null;
    await flush(activeKey, editor);
  }

  /** 放弃左侧修改，恢复为快照内容 */
  async function discardSplit(): Promise<void> {
    if (!split || !editor) return;
    const snap = split;
    split = null;
    editor.setMarkdown(snap.snapshotMd);
    liveMd = snap.snapshotMd;
    await flush(activeKey, editor);
  }

  /**
   * 修改历史移动：dir='back' 退回到上一次修改后的内容；dir='forward' 撤回退回。
   * 两者只是同一条历史链上的双向移动（不产生新记录）。移动前先 flush，
   * 把尚未保存的输入落定并记入历史，保证它仍可从反方向找回。
   */
  async function historyGo(dir: 'back' | 'forward'): Promise<void> {
    const key = activeKey;
    const ed = editor;
    if (!ed || !key || historyMoving) return;
    historyMoving = true;
    try {
      await flush(key, ed);
      if (activeKey !== key || editor !== ed) return;
      const target = dir === 'back' ? historyBack(key) : historyForward(key);
      if (target === null) return;
      publishHistoryState(key);
      ed.setMarkdown(target);
      const normalized = ed.getMarkdown();
      liveMd = normalized;
      // 方向移动不产生新记录：只把当前位置的记录同步成"实际落盘的文本"，
      // 否则下次重开该花笺会因归一化差异而整条历史被重建
      if (await write(key, normalized)) replaceCurrent(key, normalized);
    } finally {
      historyMoving = false;
    }
  }

  function toggleSnapshots(): void {
    showSnapshots = !showSnapshots;
  }

  /** 导出：仅正文，丢弃快照与目录 */
  async function exportNote(kind: 'md' | 'pdf'): Promise<void> {
    const tab = get(activeTab);
    if (!tab || !editor) return;
    await flush(activeKey, editor);
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

    // 供「关闭花匣」调用：先落盘再关后端花匣
    const unregister = registerFlush(() => flush(activeKey, editor));

    // 关窗握手：Rust 侧拦截 CloseRequested 并发这个事件，最多等 500ms
    let unlisten: (() => void) | undefined;
    void getCurrentWindow()
      .listen('flush-request', () => {
        void (async () => {
          try {
            await flush(activeKey, editor);
          } finally {
            // 无论如何都要回 ack，否则窗口只能等超时
            void api.flushAck();
          }
        })();
      })
      .then((un) => {
        unlisten = un;
      })
      .catch(() => {
        // 事件监听挂不上时由 Rust 侧超时兜底，不影响关闭窗口
      });

    return () => {
      unsub();
      unregister();
      unlisten?.();
      if (liveTimer) clearTimeout(liveTimer);
      void flush(activeKey, editor);
      void editor?.destroy();
    };
  });
</script>

<div class="editor-pane">
  <Toolbar
    {editor}
    canBack={$noteHistoryState.canBack}
    canForward={$noteHistoryState.canForward}
    historyBusy={historyMoving}
    snapshotsOpen={showSnapshots}
    onCreateSnapshot={() => void createSnapshot()}
    onToggleSnapshots={toggleSnapshots}
    onExport={(k) => void exportNote(k)}
    onHistoryBack={() => void historyGo('back')}
    onHistoryForward={() => void historyGo('forward')}
  />
  <div class="editor-body">
    <div class="editor-wrap">
      <div class="editor-main">
        <div bind:this={container} class="editor-container prose"></div>
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
    {#if showSnapshots && $activeTab}
      <SnapshotPanel
        boxId={$activeTab.boxId}
        noteId={$activeTab.noteId}
        onCreateSnapshot={createSnapshot}
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
    /* 正文栏：宽度与左对齐由 .editor-area 上的变量驱动；左缘贴左与工具栏/状态栏对齐，右侧留白；工具栏与状态栏固定满宽，不随模式变化 */
    width: var(--editor-col-fill, 100%);
    max-width: var(--editor-col-max, none);
    margin-left: var(--editor-col-margin-left, 0);
    margin-right: auto;
    padding: 28px var(--editor-inset, clamp(16px, 2.4vw, 44px)) 80px;
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
