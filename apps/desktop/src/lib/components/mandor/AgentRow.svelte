<script lang="ts">
  import { runLabel } from '#lib/board.ts';
  import { flagMeta, runMeta } from '#lib/board.ts';
  import type { FlagView } from '#lib/protocol/FlagView.ts';
  import type { RunView } from '#lib/protocol/RunView.ts';
  import { flagIcon } from './icons.ts';
  import RunStatusBadge from './RunStatusBadge.svelte';
  import StatusDot from './StatusDot.svelte';

  interface Props {
    run: RunView;
    flag: FlagView | undefined;
    current: boolean;
    onselect: () => void;
  }
  let { run, flag, current, onselect }: Props = $props();

  const key = $derived(run.key ?? run.record_id);
  const meta = $derived(runMeta(run.state));
  const fmeta = $derived(flag ? flagMeta(flag.kind) : null);
  const FlagIcon = $derived(flag ? flagIcon(flag.kind) : null);
  const attention = $derived(run.state === 'awaiting_dev');
  const short = $derived(key.replace(/^[A-Z]+-/, ''));
</script>

<button
  type="button"
  class="agent-row"
  data-attention={attention ? 'true' : undefined}
  aria-current={current ? 'page' : undefined}
  title="{key} · {run.title} · {fmeta?.label ?? meta.label}"
  onclick={onselect}
>
  <StatusDot tone={fmeta?.tone ?? meta.tone} pulse={attention} />
  <span class="ar-short">{short}</span>
  <span class="ar-line"><span class="ar-key">{key}</span><span class="ar-title">{run.title}</span></span>
  {#if fmeta && FlagIcon}
    <span class="run" data-tone={fmeta.tone}><FlagIcon aria-hidden="true" /><span class="lbl">{fmeta.label}</span></span>
  {:else}
    <RunStatusBadge state={run.state} label={runLabel(run)} />
  {/if}
</button>
