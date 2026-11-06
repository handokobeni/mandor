<script lang="ts">
  // Kartu persetujuan izin agent (plan 2026-11-06-001, R5–R6, R8, R10, R17, R20).
  // Tidak ada tombol yang fokus otomatis; sisa waktu tidak diumumkan per detik.
  import ShieldQuestion from '@lucide/svelte/icons/shield-question';
  import { answerLabel, canAnswer, inputText, remaining, visible } from '#lib/approvals.ts';
  import { Button } from '#lib/components/ui/button/index.ts';
  import type { ApprovalView } from '#lib/protocol/ApprovalView.ts';
  import type { Answer } from '#lib/stores/approvals.svelte.ts';

  interface Props {
    card: ApprovalView;
    submitting: boolean;
    note?: string;
    now: number;
    onanswer: (answer: Answer) => void;
  }
  let { card, submitting, note, now, onanswer }: Props = $props();

  const pending = $derived(card.status === 'pending');
  const enabled = $derived(canAnswer(card, submitting));
</script>

<article class="approval" data-status={card.status} id="approval-{card.id}" aria-label="Approval request for {card.tool}">
  <header class="approval-head">
    <ShieldQuestion class="icon-sm" aria-hidden="true" />
    <span class="tool">{card.tool}</span>
    <span class="state" data-pending={pending ? 'true' : undefined}>
      {#if submitting}Sending…{:else}{answerLabel(card)}{/if}
    </span>
    {#if pending}<span class="left mono" aria-hidden="true">{remaining(card, now)} left</span>{/if}
  </header>
  <!-- Blok bergulir harus bisa dicapai keyboard agar input panjang terbaca utuh. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <pre class="approval-input mono" tabindex="0" aria-label="Requested input">{inputText(card)}</pre>
  <div class="meta muted">Folder: <span class="mono">{visible(card.cwd)}</span></div>
  {#if card.description}
    <div class="claim">
      <span class="caps muted">Agent's description (not verified by Mandor)</span>
      <span>{visible(card.description)}</span>
    </div>
  {/if}
  {#if note}<div class="note" role="status">{note}</div>{/if}
  {#if pending}
    <div class="actions">
      <Button variant="outline" size="sm" disabled={!enabled} onclick={() => onanswer('deny')}>Deny</Button>
      <Button variant="secondary" size="sm" disabled={!enabled} onclick={() => onanswer('allow_once')}>Allow once</Button>
      <Button size="sm" disabled={!enabled} onclick={() => onanswer('allow_run')}>Allow this exact request for this run</Button>
    </div>
  {/if}
</article>

<style>
  .approval { margin: 8px 16px; padding: 10px 12px; border: 1px solid var(--border); border-left: 3px solid var(--status-waiting); border-radius: var(--radius); background: var(--card); display: grid; gap: 6px; }
  .approval[data-status='answered'], .approval[data-status='expired'], .approval[data-status='cancelled'] { border-left-color: var(--border); opacity: 0.85; }
  .approval-head { display: flex; align-items: center; gap: 6px; font-weight: 500; }
  .state { color: var(--muted-foreground); font-size: var(--text-sm); }
  .state[data-pending] { color: var(--status-waiting); }
  .left { margin-left: auto; font-size: var(--text-xs); color: var(--muted-foreground); }
  .approval-input { margin: 0; max-height: 220px; overflow: auto; padding: 8px; background: var(--muted); border-radius: var(--radius-sm); white-space: pre-wrap; word-break: break-all; font-size: var(--text-sm); }
  .meta { font-size: var(--text-xs); }
  .claim { display: grid; gap: 2px; font-size: var(--text-sm); }
  .note { font-size: var(--text-sm); color: var(--status-failed); }
  .actions { display: flex; flex-wrap: wrap; gap: 6px; position: sticky; bottom: 0; }
</style>
