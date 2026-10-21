<script lang="ts">
  import { flagMeta } from '#lib/board.ts';
  import type { FlagView } from '#lib/protocol/FlagView.ts';
  import { flagIcon } from './icons.ts';

  let { flag }: { flag: FlagView } = $props();

  const meta = $derived(flagMeta(flag.kind));
  const Icon = $derived(flagIcon(flag.kind));
</script>

<div class="flag" data-tone={meta.tone}>
  <Icon aria-hidden="true" />
  <div>
    <b>{meta.label}</b>
    {#if flag.detail}<div class="reason">{flag.detail}</div>{/if}
  </div>
</div>
