<script lang="ts">
  // Selisih desain saat menyetujui plan (R8a, brief §4.2): baris Missing, Partly,
  // dan Conflicts bisa ditandai Follow design; daftar dikirim saat approve.
  import Frame from '@lucide/svelte/icons/frame';
  import Info from '@lucide/svelte/icons/info';
  import SignpostBig from '@lucide/svelte/icons/signpost-big';
  import Undo2 from '@lucide/svelte/icons/undo-2';
  import * as Alert from '#lib/components/ui/alert/index.ts';
  import { Button } from '#lib/components/ui/button/index.ts';
  import { activeRows } from '#lib/ac-review.ts';
  import { followCandidates, toggleFollow } from '#lib/plan-approval.ts';
  import type { AcLedgerView } from '#lib/protocol/AcLedgerView.ts';
  import AcChip from './AcChip.svelte';

  let { ledger, follow = $bindable() }: { ledger: AcLedgerView; follow: string[] } = $props();
  const gaps = $derived(followCandidates(ledger.rows));
  const total = $derived(activeRows(ledger.rows).length);
</script>

<div class="plan-doc plan-gaps-wrap">
  {#if ledger.design_sources.length === 0}
    <Alert.Root>
      <Frame />
      <Alert.Title>No design source for this run</Alert.Title>
      <Alert.Description>Planning did not compare the AC with a design.</Alert.Description>
    </Alert.Root>
  {:else}
    <section class="plan-gaps" aria-label="Design gaps">
      <div class="pg-head">
        <h2><Frame class="icon-sm" aria-hidden="true" />Design gaps <span class="muted">({gaps.length} of {total} AC)</span></h2>
        <span class="muted">
          Compared with <b>{ledger.design_sources.join(', ')}</b>. By default the agent builds every AC; the design only sets how
          it looks.
        </span>
      </div>
      {#if gaps.length > 0}
        <div class="pg-list">
          {#each gaps as r (r.ac_id)}
            {@const id = r.label ?? r.ac_id}
            {@const on = follow.includes(r.ac_id)}
            <div class="pg-row" data-follow={on ? 'true' : undefined}>
              <span class="mono pg-id">{id}</span>
              <div class="pg-txt">
                <div class="ac-text">{r.text}</div>
                <div class="pg-meta">
                  <AcChip kind="design" value={r.design} />
                  {#if r.design_note}<span class="muted">{r.design_note}</span>{/if}
                </div>
                {#if on}
                  <div class="pg-follow">
                    <SignpostBig class="icon-sm" aria-hidden="true" />
                    <span>The agent follows the design for {id}. This AC leaves the gate, and the PM is asked to update it in Lark.</span>
                  </div>
                {/if}
              </div>
              <div class="pg-act">
                {#if on}
                  <Button variant="ghost" size="sm" onclick={() => (follow = toggleFollow(follow, r.ac_id))}><Undo2 />Undo</Button>
                {:else}
                  <Button
                    variant="outline"
                    size="sm"
                    title="Use when the design is newer and this AC is outdated"
                    onclick={() => (follow = toggleFollow(follow, r.ac_id))}
                  >
                    <SignpostBig />Follow design
                  </Button>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {:else}
        <p class="muted" style="padding:10px 12px">Every AC is shown in the design.</p>
      {/if}
      <div class="pg-foot">
        <Info class="icon-sm" aria-hidden="true" />Missing and Partly rows are informational: the agent still builds them. Follow
        design changes that, until you approve the plan.
      </div>
    </section>
  {/if}
</div>
