<script lang="ts">
  // "Start together" (plan run grup U9): kartu satu epic milik dev yang bisa
  // dikerjakan sebagai satu run, satu branch, dan satu PR. Urutan centang dan
  // panah menentukan urutan pengerjaan.
  import ArrowDown from '@lucide/svelte/icons/arrow-down';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import { Button } from '#lib/components/ui/button/index.ts';
  import { Checkbox } from '#lib/components/ui/checkbox/index.ts';
  import { cardKey, initialOrder, moveCard, toggleCard } from '#lib/group.ts';
  import type { GroupCandidateView } from '#lib/protocol/GroupCandidateView.ts';
  import type { Rpc } from '#lib/rpc.ts';

  interface Props {
    rpc: Rpc;
    recordId: string;
    /** Kartu terpilih dalam urutan pengerjaan; satu kartu = Start biasa. */
    order: string[];
  }
  let { rpc, recordId, order = $bindable() }: Props = $props();

  let candidates = $state<GroupCandidateView[] | null>(null);
  $effect(() => {
    rpc
      .request<GroupCandidateView[]>('groups.candidates', { record_id: recordId })
      .then((list) => {
        candidates = list;
        // Awalnya hanya kartu ini; dev mencentang kartu lain untuk Start together.
        order = initialOrder(list).filter((id) => id === recordId);
      })
      .catch(() => {
        candidates = [];
      });
  });

  const selectable = $derived((candidates ?? []).filter((c) => !c.blocked));
  const ordered = $derived(
    order
      .map((id) => candidates?.find((c) => c.record_id === id))
      .filter((c): c is GroupCandidateView => !!c)
  );
  const others = $derived((candidates ?? []).filter((c) => !order.includes(c.record_id)));
</script>

{#if selectable.length > 1}
  <div class="grid gap-1.5">
    <span class="text-sm font-medium">Start together <span class="muted font-normal">(optional)</span></span>
    <p class="muted text-[length:var(--text-sm)]">
      Other cards in this epic assigned to you. Checked cards are built one after another in one branch and one PR, in
      this order.
    </p>
    <div class="repo-pick scroll-area" role="group" aria-label="Cards to start together">
      {#each ordered as c, i (c.record_id)}
        <div class="pick-row">
          <Checkbox checked onCheckedChange={() => (order = toggleCard(order, c.record_id))} aria-label="Include {cardKey(c)}" />
          <span class="pick-txt"><b><span class="mono">{i + 1}. {cardKey(c)}</span> {c.title}</b></span>
          <span class="ml-auto flex gap-0.5">
            <Button
              variant="ghost"
              size="icon-xs"
              aria-label="Move {cardKey(c)} up"
              disabled={i === 0}
              onclick={() => (order = moveCard(order, c.record_id, -1))}><ArrowUp /></Button
            >
            <Button
              variant="ghost"
              size="icon-xs"
              aria-label="Move {cardKey(c)} down"
              disabled={i === ordered.length - 1}
              onclick={() => (order = moveCard(order, c.record_id, 1))}><ArrowDown /></Button
            >
          </span>
        </div>
      {/each}
      {#each others as c (c.record_id)}
        <label class="pick-row" title={c.blocked ?? undefined}>
          <Checkbox
            checked={false}
            disabled={!!c.blocked}
            onCheckedChange={() => (order = toggleCard(order, c.record_id))}
          />
          <span class="pick-txt">
            <b><span class="mono">{cardKey(c)}</span> {c.title}</b>
            {#if c.blocked}<span class="muted">{c.blocked}</span>{/if}
          </span>
        </label>
      {/each}
    </div>
    {#if order.length > 1}
      <p class="muted text-[length:var(--text-sm)]">
        {order.length} cards: Mandor claims all of them and moves them to In Progress. After each card, press Next card
        to commit it and start the next one.
      </p>
    {/if}
  </div>
{/if}
