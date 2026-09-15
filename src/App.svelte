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
  import { openBox, widthMode } from './lib/stores';
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

  function applyTheme(id: string, dark: boolean): void {
    const t = themeById(id);
    const vars = dark ? t.dark : t.light;
    for (const [k, v] of Object.entries(vars)) {
      document.documentElement.style.setProperty(k, v);
    }
    document.documentElement.classList.toggle('dark', dark);
  }

  function persistAppearance(): void {
    void api.setAppearance(themeId, isDark, sidebarW, $widthMode);
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

  async function openHxPath(path: string): Promise<void> {
    try {
      const info = await api.openBox(path);
      await openBox(info);
    } catch (e) {
      alert(`打开失败: ${errMsg(e)}`);
    }
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
    const fs = await win.isFullscreen();
    await win.setFullscreen(!fs);
    isFullscreen = !fs;
  }

  async function toggleMaximize(): Promise<void> {
    const m = await win.isMaximized();
    if (m) await win.unmaximize();
    else await win.maximize();
  }

  function toggleTheme(): void {
    setDark(!isDark);
  }

  function startDrag(): void {
    drag = true;
  }
  function onMove(e: PointerEvent): void {
    if (!drag) return;
    sidebarW = Math.min(560, Math.max(180, e.clientX));
  }
  function endDrag(): void {
    drag = false;
    persistAppearance();
  }

  onMount(() => {
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
      for (const p of saved) await openHxPath(p);
      void api.getStartupHx().then((p) => {
        if (p) void openHxPath(p);
      });
    })();

    const cleanup: (() => void)[] = [];

    void win.listen<string>('open-hx', (e) => {
      void openHxPath(e.payload);
    }).then((un) => cleanup.push(un));

    void win
      .onDragDropEvent((e) => {
        if (e.payload.type === 'drop') {
          for (const p of e.payload.paths) {
            if (p.toLowerCase().endsWith('.hxl')) void openHxPath(p);
          }
        }
      })
      .then((un) => cleanup.push(un));

    void win.onResized(async () => {
      isFullscreen = await win.isFullscreen();
    }).then((un) => cleanup.push(un));

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
    };
  });
</script>

<div class="app" onpointermove={onMove} onpointerup={endDrag}>
  <div class="main">
    <aside class="sidebar-outer" style="width:{sidebarW}px">
      <Sidebar
        onNewBox={() => (showNewBox = true)}
        onOpenBox={() => void pickOpenBox()}
        onRenameRequest={requestRename}
      />
    </aside>
    <div class="resizer" class:dragging={drag} onpointerdown={startDrag}></div>
    <section class="editor-area" data-width-mode={$widthMode}>
      <div class="header-row">
        <TabBar {renameTarget} onRenameDone={renameDone} />
        <div class="win-controls">
          <button
            class="icon-btn"
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
          <button class="icon-btn" title="最大化/还原" onclick={() => void toggleMaximize()}>▢</button>
          <button class="icon-btn" title="全屏 (F11)" onclick={() => void toggleFullscreen()}>{isFullscreen ? '⤡' : '⛶'}</button>
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
  }
  /* 铺满：正文用满侧栏以外的全部宽度 */
  .editor-area[data-width-mode='full'] {
    --editor-col-max: none;
    --editor-col-align: flex-start;
    --editor-col-fill: 100%;
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
