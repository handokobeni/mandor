<script lang="ts">
  import { runMeta } from '#lib/board.ts';
  import { runIcon } from './icons.ts';

  interface Props {
    state: string;
    /** Mengganti label bawaan, mis. "Quiet period: forwarding in 1:30". */
    label?: string;
  }
  let { state, label }: Props = $props();

  const meta = $derived(runMeta(state));
  const Icon = $derived(runIcon(state));
</script>

<span class="run" data-tone={meta.tone}>
  <Icon class={meta.spin ? 'spin' : ''} aria-hidden="true" />
  <span class="lbl">{label ?? meta.label}</span>
</span>
