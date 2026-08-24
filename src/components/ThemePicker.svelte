<script lang="ts">
  import { THEMES } from '../lib/themes';

  interface Props {
    current: string;
    onPick: (id: string) => void;
    onClose: () => void;
  }
  let { current, onPick, onClose }: Props = $props();
</script>

<div class="picker" role="menu" onkeydown={(e) => e.key === 'Escape' && onClose()}>
  <div class="title">界面样式</div>
  {#each THEMES as t}
    {@const light = t.light}
    {@const dark = t.dark}
    <button
      class="theme-row {current === t.id ? 'active' : ''}"
      onclick={() => {
        onPick(t.id);
        onClose();
      }}
    >
      <span class="swatches">
        <span class="swatch light" style="background:{light['--bg']};color:{light['--accent']}">
          <i class="bar" style="background:{light['--accent']}"></i>
        </span>
        <span class="swatch dark" style="background:{dark['--bg']};color:{dark['--accent']}">
          <i class="bar" style="background:{dark['--accent']}"></i>
        </span>
      </span>
      <span class="name">{t.name}</span>
      <span class="mark">{current === t.id ? '✓' : ''}</span>
    </button>
  {/each}
</div>

<style>
  .picker {
    position: absolute;
    top: 40px;
    right: 8px;
    z-index: 60;
    min-width: 200px;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 8px;
    box-shadow: 0 8px 28px rgba(0, 0, 0, 0.22);
  }
  .title {
    font-size: 12px;
    color: var(--text-dim);
    padding: 2px 6px 6px;
  }
  .theme-row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 8px;
    border-radius: 8px;
    cursor: pointer;
    text-align: left;
  }
  .theme-row:hover {
    background: var(--bg-hover);
  }
  .theme-row.active {
    background: var(--bg-active);
  }
  .swatches {
    display: flex;
    gap: 4px;
    flex: none;
  }
  .swatch {
    width: 26px;
    height: 20px;
    border-radius: 5px;
    border: 1px solid rgba(0, 0, 0, 0.15);
    display: flex;
    align-items: flex-end;
    padding: 3px;
  }
  .bar {
    width: 100%;
    height: 3px;
    border-radius: 2px;
  }
  .name {
    flex: 1;
    font-size: 13px;
    color: var(--text);
  }
  .mark {
    color: var(--accent);
    font-size: 13px;
  }
</style>
