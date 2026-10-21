<script lang="ts">
  import type { Tone } from '#lib/board.ts';

  interface Props {
    tone: Tone;
    pulse?: boolean;
  }
  let { tone, pulse = false }: Props = $props();

  const color = $derived(tone === 'muted' ? 'var(--muted-foreground)' : `var(--status-${tone})`);
</script>

<span class="dot" class:pulse style:--run-c={color} aria-hidden="true"></span>
