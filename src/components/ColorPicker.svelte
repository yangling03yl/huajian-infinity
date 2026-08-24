<script lang="ts">
  import { PALETTE } from '../lib/colors';

  interface Props {
    onPick: (color: string) => void;
    onClose: () => void;
  }
  let { onPick, onClose }: Props = $props();
</script>

<div class="picker" role="menu">
  <div class="grid">
    {#each PALETTE as c}
      <button
        class="swatch"
        style="background:{c}"
        title={c}
        onclick={() => onPick(c)}
      ></button>
    {/each}
  </div>
  <label class="custom" title="自定义颜色">
    <input
      type="color"
      value="#b4753f"
      onchange={(e) => onPick((e.currentTarget as HTMLInputElement).value)}
    />
    <span>自定义</span>
  </label>
</div>

<style>
  .picker {
    position: absolute;
    top: 40px;
    left: 12px;
    z-index: 50;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px;
    box-shadow: 0 8px 28px rgba(0, 0, 0, 0.22);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(6, 24px);
    gap: 6px;
  }
  .swatch {
    width: 24px;
    height: 24px;
    border-radius: 6px;
    border: 1px solid rgba(0, 0, 0, 0.15);
    transition: transform 0.1s;
  }
  .swatch:hover {
    transform: scale(1.15);
  }
  .custom {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--text-dim);
    cursor: pointer;
  }
  .custom input[type='color'] {
    width: 30px;
    height: 24px;
    border: none;
    padding: 0;
    background: none;
    cursor: pointer;
  }
</style>
