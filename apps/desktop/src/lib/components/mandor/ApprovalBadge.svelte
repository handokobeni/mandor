<script lang="ts">
  // Badge "Needs approval" untuk run dengan kartu persetujuan menunggu (R9).
  import ShieldQuestion from '@lucide/svelte/icons/shield-question';
  import { approvalBadge } from '#lib/board.ts';

  let { run }: { run: { pending_approvals: number } | null | undefined } = $props();
  const label = $derived(approvalBadge(run));
</script>

{#if label}
  <span class="run" data-tone="waiting"><ShieldQuestion aria-hidden="true" /><span class="lbl">{label}</span></span>
{/if}
