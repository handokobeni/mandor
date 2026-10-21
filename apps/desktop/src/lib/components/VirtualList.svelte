<script lang="ts" generics="T extends { id: number }">
  import type { Snippet } from 'svelte';

  interface Props {
    items: T[];
    /** Perkiraan tinggi baris sebelum diukur. */
    estimate?: number;
    class?: string;
    row: Snippet<[T]>;
  }
  let { items, estimate = 80, class: className = '', row }: Props = $props();

  // Tinggi terukur per id; `measured` dinaikkan agar posisi dihitung ulang.
  const heights = new Map<number, number>();
  let measured = $state(0);
  let containerHeight = $state(0);
  let scrollTop = $state(0);
  let container: HTMLDivElement | undefined = $state();
  let pinnedToBottom = true;

  const overscanPx = 600;

  const offsets = $derived.by(() => {
    void measured;
    const out = new Array<number>(items.length + 1);
    out[0] = 0;
    for (let i = 0; i < items.length; i++) out[i + 1] = out[i] + (heights.get(items[i].id) ?? estimate);
    return out;
  });

  function indexAt(y: number): number {
    let lo = 0;
    let hi = items.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (offsets[mid + 1] <= y) lo = mid + 1;
      else hi = mid;
    }
    return lo;
  }

  const start = $derived(indexAt(Math.max(0, scrollTop - overscanPx)));
  const end = $derived(Math.min(items.length, indexAt(scrollTop + containerHeight + overscanPx) + 1));
  const visible = $derived(items.slice(start, end));

  function measure(node: HTMLElement, id: number) {
    let current = id;
    const ro = new ResizeObserver(() => {
      const h = node.offsetHeight;
      if (heights.get(current) !== h) {
        heights.set(current, h);
        measured++;
      }
    });
    ro.observe(node);
    return {
      update(next: number) {
        current = next;
      },
      destroy() {
        ro.disconnect();
      }
    };
  }

  function onscroll(e: Event): void {
    const el = e.currentTarget as HTMLElement;
    scrollTop = el.scrollTop;
    pinnedToBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 60;
  }

  // Ikuti pesan terbaru selama pengguna tidak menggulir ke atas.
  $effect(() => {
    void items.length;
    void measured;
    if (pinnedToBottom && container) {
      const el = container;
      requestAnimationFrame(() => (el.scrollTop = el.scrollHeight));
    }
  });
</script>

<div class="scroll-area {className}" bind:this={container} bind:clientHeight={containerHeight} {onscroll}>
  <div style="padding-top: {offsets[start]}px; padding-bottom: {offsets[items.length] - offsets[end]}px">
    {#each visible as item (item.id)}
      <div class="vrow" use:measure={item.id}>
        {@render row(item)}
      </div>
    {/each}
  </div>
</div>
