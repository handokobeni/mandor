<script lang="ts" generics="T extends { id: number }">
  import type { Snippet } from 'svelte';
  import { offsetsOf, visibleRange } from '#lib/virtual.ts';

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
    return offsetsOf(
      items.map((i) => i.id),
      heights,
      estimate
    );
  });

  const range = $derived(visibleRange(offsets, scrollTop, containerHeight, overscanPx));
  const start = $derived(range.start);
  const end = $derived(range.end);
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
