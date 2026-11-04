<script lang="ts">
  import { save } from '@tauri-apps/plugin-dialog';
  import { api, errMsg } from '../lib/api';
  import { openBox, showHint } from '../lib/stores';

  interface Props {
    onClose: () => void;
  }
  let { onClose }: Props = $props();

  let name = $state('新花匣');
  let busy = $state(false);

  async function create(): Promise<void> {
    const boxName = name.trim() || '新花匣';
    busy = true;
    try {
      const path = await save({
        title: '保存花匣',
        defaultPath: `${boxName}.hxl`,
        filters: [{ name: '花匣', extensions: ['hxl'] }],
      });
      if (!path) return;
      // 花匣名只由文件名决定：用户在保存对话框里改了文件名，就以新文件名为准
      const info = await api.createBox(path);
      await openBox(info);
      showHint(`已创建花匣「${info.name}」`);
      onClose();
    } catch (e) {
      alert(errMsg(e));
    } finally {
      busy = false;
    }
  }
</script>

<div class="modal-mask" onclick={() => !busy && onClose()}>
  <div class="modal" onclick={(e) => e.stopPropagation()}>
    <h3>新建花匣</h3>
    <input
      type="text"
      placeholder="花匣名称"
      bind:value={name}
      onkeydown={(e) => {
        if (e.key === 'Enter') void create();
      }}
    />
    <p class="hint">花匣名就是文件名；若在保存对话框里改了文件名，将以文件名为准。</p>
    <div class="modal-actions">
      <button class="ghost-btn" onclick={onClose} disabled={busy}>取消</button>
      <button class="primary-btn" onclick={() => void create()} disabled={busy}>
        {busy ? '创建中…' : '创建'}
      </button>
    </div>
  </div>
</div>

<style>
  .hint {
    margin: -6px 0 14px;
    font-size: 12px;
    line-height: 1.6;
    color: var(--text-dim);
  }
</style>
