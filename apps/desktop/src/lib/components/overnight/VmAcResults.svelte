<script lang="ts">
  // Hasil AC kartu VM (hanya baca): verdict, hasil tampilan, dan screenshot
  // yang hanya disimpan daemon VM (plan 2026-11-03-002, R21, R26).
  import AcRow from '#lib/components/workspace/AcRow.svelte';
  import { activeRows } from '#lib/ac-review.ts';
  import type { AcRowView } from '#lib/protocol/AcRowView.ts';
  import { overnight } from '#lib/stores/overnight.svelte.ts';

  let { runId, taskKey }: { runId: string; taskKey: string } = $props();

  let rows = $state<AcRowView[] | null>(null);
  let error = $state<string | null>(null);
  let expanded = $state<Record<string, boolean>>({});
  let focusIdx = $state(0);

  $effect(() => {
    const id = runId;
    rows = null;
    error = null;
    overnight
      .ledger(id)
      .then((l) => (rows = activeRows(l.rows)))
      .catch((err: unknown) => (error = err instanceof Error ? err.message : String(err)));
  });
</script>

{#if error}
  <p class="field-error">Could not load the AC from the VM: {error}</p>
{:else if rows === null}
  <p class="muted">Loading the AC from the VM…</p>
{:else if rows.length === 0}
  <p class="muted">This card has no acceptance criteria.</p>
{:else}
  <div class="ac-ledger" role="grid" aria-label="AC results for {taskKey}" aria-readonly="true">
    <div class="ac-row ac-head" role="row">
      <span class="ac-c ac-sel" role="columnheader"><span class="sr-only">Select</span></span>
      <span class="ac-c ac-id" role="columnheader">AC</span>
      <span class="ac-c ac-txt" role="columnheader">Criterion</span>
      <span class="ac-c ac-d" role="columnheader">Design</span>
      <span class="ac-c ac-v" role="columnheader">Verdict</span>
      <span class="ac-c ac-x" role="columnheader"><span class="sr-only">Expand</span></span>
    </div>
    {#each rows as r, i (r.ac_id)}
      <AcRow
        row={r}
        expanded={!!expanded[r.ac_id]}
        selected={false}
        focused={i === Math.min(focusIdx, rows.length - 1)}
        readonly={true}
        noSource={false}
        onexpand={() => (expanded[r.ac_id] = !expanded[r.ac_id])}
        onselect={() => {}}
        onfocus={() => (focusIdx = i)}
        onkey={(e) => {
          if (e.key === 'Enter') expanded[r.ac_id] = !expanded[r.ac_id];
        }}
        onfile={() => {}}
        onscreenshot={(idx) => overnight.screenshot(runId, r.ac_id, idx)}
      />
    {/each}
  </div>
{/if}
