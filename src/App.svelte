<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import Sidebar from './components/Sidebar.svelte';
  import TabBar from './components/TabBar.svelte';
  import EditorPane from './components/EditorPane.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import NewBoxModal from './components/NewBoxModal.svelte';
  import ThemePicker from './components/ThemePicker.svelte';
  import { api, errMsg } from './lib/api';
  import {
    boxes,
    openBox,
    refreshNotes,
    sameBoxPath,
    showHint,
    statusMsg,
    toggleBox,
    widthMode,
  } from './lib/stores';
  import { themeById } from './lib/themes';
  import type { WidthMode } from './lib/types';

  const win = getCurrentWindow();

  let sidebarW = $state(300);
  let showNewBox = $state(false);
  let isFullscreen = $state(false);
  let themeId = $state('warm-paper');
  let isDark = $state(false);
  let showThemes = $state(false);
  let renameTarget = $state<{ boxId: string; noteId: string } | null>(null);
  let drag = $state(false);

  /** 启动提示（数据迁移等）的自动清空计时器 */
  const STARTUP_NOTICE_MS = 8000;
  let startupNoticeTimer: ReturnType<typeof setTimeout> | null = null;

  function showStartupNotice(text: string): void {
    statusMsg.set(text);
    if (startupNoticeTimer !== null) clearTimeout(startupNoticeTimer);
    startupNoticeTimer = setTimeout(() => {
      startupNoticeTimer = null;
      // 期间若被别的提示覆盖，就不要清掉新内容
      statusMsg.update((cur) => (cur === text ? '' : cur));
    }, STARTUP_NOTICE_MS);
  }

  function applyTheme(id: string, dark: boolean): void {
    const t = themeById(id);
    const vars = dark ? t.dark : t.light;
    for (const [k, v] of Object.entries(vars)) {
      document.documentElement.style.setProperty(k, v);
    }
    document.documentElement.classList.toggle('dark', dark);
  }

  function persistAppearance(): void {
    void api
      .setAppearance(themeId, isDark, sidebarW, $widthMode)
      .catch((e: unknown) => showHint(`设置保存失败：${errMsg(e)}`));
  }

  /** 两档循环：靠左 75% → 铺满 → 靠左 75% */
  const WIDTH_CYCLE: WidthMode[] = ['left75', 'full'];

  function cycleWidth(): void {
    const i = WIDTH_CYCLE.indexOf($widthMode);
    widthMode.set(WIDTH_CYCLE[(i + 1) % WIDTH_CYCLE.length]);
    persistAppearance();
  }

  function setDark(dark: boolean): void {
    isDark = dark;
    applyTheme(themeId, dark);
    persistAppearance();
  }

  function pickTheme(id: string): void {
    themeId = id;
    applyTheme(id, isDark);
    persistAppearance();
  }

  function requestRename(boxId: string, noteId: string): void {
    renameTarget = { boxId, noteId };
  }

  function renameDone(): void {
    renameTarget = null;
  }

  /** 路径 → 展示用花匣名（去掉目录与 .hxl 后缀） */
  function pathBaseName(p: string): string {
    const name = p.split(/[\\/]/).pop() ?? p;
    return name.replace(/\.hxl$/i, '');
  }

  /**
   * 尝试打开一个花匣路径：成功返回 null，失败返回错误文案。
   * 不在这里弹窗——启动时批量打开需要用一条汇总提示。
   */
  async function tryOpenHxPath(path: string): Promise<string | null> {
    // 同一文件不重复打开：已打开则展开并刷新（覆盖打开对话框/拖拽/命令行/文件关联四个入口，
    // 也避免 settings.boxes 里出现同一路径两次）；后端 open 里还有一层同路径去重兜底
    const existing = $boxes.find((b) => sameBoxPath(b.info.path, path));
    if (existing) {
      if (!existing.expanded) toggleBox(existing.info.id);
      await refreshNotes(existing.info.id);
      return null;
    }
    try {
      const info = await api.openBox(path);
      await openBox(info);
      return null;
    } catch (e) {
      return errMsg(e);
    }
  }

  /** 用户主动打开（对话框/拖拽/命令行）：失败直接提示 */
  async function openHxPath(path: string): Promise<void> {
    const err = await tryOpenHxPath(path);
    if (err) alert(`打开失败: ${err}`);
  }

  async function pickOpenBox(): Promise<void> {
    try {
      const path = await openDialog({
        title: '打开花匣',
        multiple: false,
        filters: [{ name: '花匣 (.hxl)', extensions: ['hxl'] }],
      });
      if (typeof path === 'string') await openHxPath(path);
    } catch (e) {
      alert(`打开失败: ${errMsg(e)}`);
    }
  }

  async function toggleFullscreen(): Promise<void> {
    try {
      const fs = await win.isFullscreen();
      await win.setFullscreen(!fs);
      isFullscreen = !fs;
    } catch (e) {
      showHint(`窗口操作失败：${errMsg(e)}`);
    }
  }

  async function toggleMaximize(): Promise<void> {
    try {
      // 用 toggleMaximize 而不是 maximize/unmaximize：一条命令、一条权限
      await win.toggleMaximize();
    } catch (e) {
      showHint(`窗口操作失败：${errMsg(e)}`);
    }
  }

  function toggleTheme(): void {
    setDark(!isDark);
  }

  function startDrag(e: PointerEvent): void {
    if (e.button !== 0) return;
    // 捕获指针：拖到窗口外松手也能收到 pointerup/pointercancel，否则 drag 会一直为真
    try {
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    } catch {
      // 捕获失败时退化为窗口内拖拽
    }
    drag = true;
  }
  function onMove(e: PointerEvent): void {
    if (!drag) return;
    sidebarW = Math.min(560, Math.max(180, e.clientX));
  }
  function endDrag(): void {
    if (!drag) return;
    drag = false;
    persistAppearance();
  }

  onMount(() => {
    // 首屏先隐藏，等设置（主题/明暗）读回来再显示：
    // 否则深色主题用户每次启动都会看到一闪的浅色。1.5s 兜底，设置读不出来也必须显示。
    document.documentElement.style.visibility = 'hidden';
    const reveal = () => {
      document.documentElement.style.visibility = '';
    };
    const revealTimer = setTimeout(reveal, 1500);

    applyTheme(themeId, isDark);

    void (async () => {
      let saved: string[] = [];
      try {
        const s = await api.getSettings();
        themeId = s.theme || 'warm-paper';
        isDark = s.dark;
        sidebarW = s.sidebar_width || 300;
        // 旧配置可能残留已删除的 narrow 等值，非 full 一律按默认靠左 75% 处理
        widthMode.set(s.width_mode === 'full' ? 'full' : 'left75');
        saved = s.boxes;
        applyTheme(themeId, isDark);
      } catch (e) {
        alert(`读取设置失败: ${errMsg(e)}`);
      }
      try {
        // 启动时批量恢复花匣：失败的用一条汇总提示，并顺手从设置里移除，
        // 否则每次启动都会为同一个失效路径再弹一次
        const failed: string[] = [];
        for (const p of saved) {
          const err = await tryOpenHxPath(p);
          if (err) {
            failed.push(p);
            void api.dropBoxPath(p).catch(() => {});
          }
        }
        if (failed.length > 0) {
          showStartupNotice(
            `有 ${failed.length} 个花匣已无法打开，已从列表移除：${failed.map(pathBaseName).join('、')}`,
          );
        }
      } finally {
        clearTimeout(revealTimer);
        reveal();
      }
      void api
        .getStartupHx()
        .then((p) => {
          if (p) void openHxPath(p);
        })
        .catch(() => {});
      // 一次性启动提示（旧数据迁移成功/失败），后端读取后即清空
      void api
        .getStartupNotice()
        .then((notice) => {
          if (notice) showStartupNotice(notice);
        })
        .catch(() => {});
    })();

    const cleanup: (() => void)[] = [];

    void win
      .listen<string>('open-hx', (e) => {
        void openHxPath(e.payload);
      })
      .then((un) => cleanup.push(un))
      .catch(() => {});

    // 设置写盘失败（磁盘满/无权限）由后端上报，这里必须让用户看见
    void win
      .listen<string>('settings-error', (e) => showHint(`设置保存失败：${e.payload}`))
      .then((un) => cleanup.push(un))
      .catch(() => {});

    void win
      .onDragDropEvent((e) => {
        if (e.payload.type === 'drop') {
          for (const p of e.payload.paths) {
            if (p.toLowerCase().endsWith('.hxl')) void openHxPath(p);
          }
        }
      })
      .then((un) => cleanup.push(un))
      .catch(() => {});

    void win
      .onResized(() => {
        void win
          .isFullscreen()
          .then((v) => (isFullscreen = v))
          .catch(() => {});
      })
      .then((un) => cleanup.push(un))
      .catch(() => {});

    const keyHandler = (e: KeyboardEvent) => {
      if (e.key === 'F11') {
        e.preventDefault();
        void toggleFullscreen();
      }
    };
    window.addEventListener('keydown', keyHandler);

    return () => {
      for (const un of cleanup) un();
      window.removeEventListener('keydown', keyHandler);
      clearTimeout(revealTimer);
      reveal();
      if (startupNoticeTimer !== null) {
        clearTimeout(startupNoticeTimer);
        startupNoticeTimer = null;
      }
    };
  });
</script>

<div class="app">
  <div class="main">
    <aside class="sidebar-outer" style="width:{sidebarW}px">
      <Sidebar
        onNewBox={() => (showNewBox = true)}
        onOpenBox={() => void pickOpenBox()}
        onRenameRequest={requestRename}
      />
    </aside>
    <div
      class="resizer"
      class:dragging={drag}
      role="separator"
      aria-orientation="vertical"
      aria-label="调整侧栏宽度"
      onpointerdown={startDrag}
      onpointermove={onMove}
      onpointerup={endDrag}
      onpointercancel={endDrag}
    ></div>
    <section class="editor-area" data-width-mode={$widthMode}>
      <div class="header-row">
        <TabBar {renameTarget} onRenameDone={renameDone} />
        <div class="win-controls">
          <button
            class="icon-btn"
            title="切换正文宽度（靠左 75% / 铺满）"
            aria-label="切换正文宽度"
            onclick={cycleWidth}
          >
            <svg class="width-icon" viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
              <rect class="frame" x="3" y="6" width="18" height="12" rx="2" />
              {#if $widthMode === 'full'}
                <rect class="fill" x="5.4" y="8.4" width="13.2" height="7.2" rx="1" />
              {:else}
                <rect class="fill" x="5.4" y="8.4" width="9.9" height="7.2" rx="1" />
              {/if}
            </svg>
          </button>
          <button class="icon-btn" title="界面样式" onclick={() => (showThemes = !showThemes)}>◑</button>
          <button class="icon-btn" title="主题切换" onclick={toggleTheme}>◐</button>
          <button
            class="icon-btn"
            title="最大化/还原"
            onclick={() => void toggleMaximize()}
          >▢</button>
          <button
            class="icon-btn"
            title="全屏 (F11)"
            onclick={() => void toggleFullscreen()}
          >{isFullscreen ? '⤡' : '⛶'}</button>
        </div>
      </div>
      <EditorPane />
      <StatusBar />
    </section>
  </div>
</div>

{#if showNewBox}
  <NewBoxModal onClose={() => (showNewBox = false)} />
{/if}

{#if showThemes}
  <div class="picker-mask" role="button" tabindex="-1" onclick={() => (showThemes = false)}></div>
  <ThemePicker current={themeId} onPick={pickTheme} onClose={() => (showThemes = false)} />
{/if}

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--bg);
  }
  .main {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .sidebar-outer {
    flex: none;
    min-width: 180px;
    overflow: hidden;
  }
  .resizer {
    width: 4px;
    cursor: col-resize;
    background: transparent;
    flex: none;
    transition: background 0.1s;
    margin-left: -2px;
    z-index: 10;
    touch-action: none;
  }
  .resizer:hover,
  .resizer.dragging {
    background: var(--accent);
  }
  .editor-area {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    /* 正文栏宽与水平内缩量；内缩量同时用于工具栏/状态栏，保证左缘对齐 */
    --editor-inset: clamp(16px, 2.4vw, 44px);
    /* 靠左 75%：正文占可用宽度的 75%，右边留白，不居中；工具栏/状态栏不随此变化 */
    --editor-col-align: flex-start;
    --editor-col-fill: 75%;
    /* 正文栏左外边距；两种模式均为 0（贴左），变量为将来可能的居中窄栏模式预留 */
    --editor-col-margin-left: 0;
  }
  /* 铺满：正文用满侧栏以外的全部宽度 */
  .editor-area[data-width-mode='full'] {
    --editor-col-max: none;
    --editor-col-align: flex-start;
    --editor-col-fill: 100%;
    --editor-col-margin-left: 0;
  }
  .header-row {
    display: flex;
    flex: none;
    align-items: stretch;
  }
  .header-row :global(.tabbar) {
    flex: 1;
    min-width: 0;
  }
  .win-controls {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 8px;
    background: var(--bg-panel);
    border-bottom: 1px solid var(--border);
    border-left: 1px solid var(--border);
    flex: none;
    position: relative;
  }
  .picker-mask {
    position: fixed;
    inset: 0;
    z-index: 55;
  }
  /* ---- 正文宽度切换：外框 + 当前正文列的示意 ---- */
  .width-icon {
    display: block;
    overflow: visible;
  }
  .width-icon .frame {
    fill: none;
    stroke: currentColor;
    stroke-width: 1.6;
    opacity: 0.55;
  }
  .width-icon .fill {
    fill: currentColor;
  }
</style>
