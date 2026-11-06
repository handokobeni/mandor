<script lang="ts">
  // Penampil screenshot perbandingan tampilan: Fit, ukuran asli (100%, piksel
  // tajam), zoom bertingkat, dan pindah App ⇄ Prototype.
  import ChevronLeft from '@lucide/svelte/icons/chevron-left';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import ZoomIn from '@lucide/svelte/icons/zoom-in';
  import ZoomOut from '@lucide/svelte/icons/zoom-out';
  import { nextZoom, shotLabels } from '#lib/ac-review.ts';
  import { Button } from '#lib/components/ui/button/index.ts';
  import * as Dialog from '#lib/components/ui/dialog/index.ts';

  interface Props {
    shots: string[];
    title: string;
    index: number;
    onclose: () => void;
  }
  let { shots, title, index = $bindable(), onclose }: Props = $props();

  let zoom = $state<'fit' | number>('fit');
  let natural = $state<{ w: number; h: number } | null>(null);
  const labels = $derived(shotLabels(shots.length));
  const pct = $derived(zoom === 'fit' ? 'Fit' : `${Math.round(zoom * 100)}%`);

  function go(dir: 1 | -1): void {
    index = (index + dir + shots.length) % shots.length;
  }

  function onkey(e: KeyboardEvent): void {
    if (shots.length > 1 && e.key === 'ArrowRight') go(1);
    else if (shots.length > 1 && e.key === 'ArrowLeft') go(-1);
    else if (e.key === '+' || e.key === '=') zoom = nextZoom(zoom, 1);
    else if (e.key === '-') zoom = nextZoom(zoom, -1);
    else if (e.key === '0') zoom = 'fit';
    else if (e.key === '1') zoom = 1;
    else return;
    e.preventDefault();
  }
</script>

<Dialog.Root open onOpenChange={(o) => !o && onclose()}>
  <Dialog.Content class="shot-viewer" onkeydown={onkey}>
    <Dialog.Header>
      <Dialog.Title class="text-[length:var(--text-md)]">{title} · {labels[index]}</Dialog.Title>
    </Dialog.Header>
    <div class="shot-bar">
      {#if shots.length > 1}
        <Button size="sm" variant="ghost" onclick={() => go(-1)} aria-label="Previous screenshot"><ChevronLeft /></Button>
        {#each labels as l, i (i)}
          <Button size="sm" variant={i === index ? 'secondary' : 'ghost'} aria-pressed={i === index} onclick={() => (index = i)}>{l}</Button>
        {/each}
        <Button size="sm" variant="ghost" onclick={() => go(1)} aria-label="Next screenshot"><ChevronRight /></Button>
      {/if}
      <span class="shot-spacer"></span>
      <Button size="sm" variant={zoom === 'fit' ? 'secondary' : 'ghost'} aria-pressed={zoom === 'fit'} onclick={() => (zoom = 'fit')}>Fit</Button>
      <Button size="sm" variant={zoom === 1 ? 'secondary' : 'ghost'} aria-pressed={zoom === 1} onclick={() => (zoom = 1)}>100%</Button>
      <Button size="sm" variant="ghost" onclick={() => (zoom = nextZoom(zoom, -1))} aria-label="Zoom out"><ZoomOut /></Button>
      <span class="mono muted shot-pct" aria-live="polite">{pct}</span>
      <Button size="sm" variant="ghost" onclick={() => (zoom = nextZoom(zoom, 1))} aria-label="Zoom in"><ZoomIn /></Button>
    </div>
    <div class="shot-stage" data-fit={zoom === 'fit' ? 'true' : undefined}>
      <img
        src={shots[index]}
        alt="{labels[index]} of {title}"
        style:width={zoom !== 'fit' && natural ? `${natural.w * zoom}px` : undefined}
        onload={(e) => {
          const img = e.currentTarget as HTMLImageElement;
          natural = { w: img.naturalWidth, h: img.naturalHeight };
        }}
      />
    </div>
    <p class="muted shot-hint">
      {#if natural}{natural.w}×{natural.h} px · {/if}100% shows every pixel. Keys: ← → switch, + − zoom, 0 fit, 1 actual size.
    </p>
  </Dialog.Content>
</Dialog.Root>
