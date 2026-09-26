<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    count: number;
    rowHeight?: number;
    overscan?: number;
    children?: Snippet<[number]>;
  }
  let { count, rowHeight = 32, overscan = 8, children }: Props = $props();
  let scrollTop = $state(0);
  let viewH = $state(0);
  let el: HTMLDivElement | undefined = $state();

  let ro: ResizeObserver | undefined;
  $effect(() => {
    if (!el) return;
    ro ??= new ResizeObserver(() => {
      viewH = el!.clientHeight;
    });
    ro.observe(el);
    viewH = el.clientHeight;
    return () => ro?.disconnect();
  });

  const total = $derived(count * rowHeight);
  const start = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - overscan));
  const end = $derived(Math.min(count, Math.ceil((scrollTop + viewH) / rowHeight) + overscan));
  const padTop = $derived(start * rowHeight);

  /** 拖拽悬停在上下边缘时自动滚动，便于把花笺拖到当前可视区之外的位置 */
  function onContainerDragOver(e: DragEvent): void {
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const edge = 28;
    if (e.clientY < rect.top + edge) {
      el.scrollTop -= Math.max(5, (rect.top + edge - e.clientY) / 2);
    } else if (e.clientY > rect.bottom - edge) {
      el.scrollTop += Math.max(5, (e.clientY - (rect.bottom - edge)) / 2);
    }
  }
</script>

<div
  class="vl"
  style="overflow-y:auto;overflow-x:hidden;height:100%"
  bind:this={el}
  onscroll={(e) => (scrollTop = (e.currentTarget as HTMLDivElement).scrollTop)}
  ondragover={onContainerDragOver}
>
  <div class="vl-inner" style="height:{total}px">
    <div class="vl-window" style="transform:translateY({padTop}px)">
      {#each Array.from({ length: Math.max(0, end - start) }, (_, i) => start + i) as idx}
        {@render children?.(idx)}
      {/each}    </div>
  </div>
</div>

<style>
  .vl {
    height: 100%;
  }
  .vl-inner {
    position: relative;
  }
  .vl-window {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
  }
</style>
