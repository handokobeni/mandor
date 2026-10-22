<script lang="ts">
  // Satu baris buku AC: pilih (hanya Fail), label + penanda, teks, Design, Verdict,
  // dan bukti saat dibuka. Bukti selalu teks biasa; berkas:baris membuka Changes.
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import FileCode from '@lucide/svelte/icons/file-code';
  import Wrench from '@lucide/svelte/icons/wrench';
  import { canSend, DESIGN_META, evidenceParts, label, rowMarks, shownVerdict } from '#lib/ac-review.ts';
  import { Button } from '#lib/components/ui/button/index.ts';
  import { Checkbox } from '#lib/components/ui/checkbox/index.ts';
  import type { AcRowView } from '#lib/protocol/AcRowView.ts';
  import AcChip from './AcChip.svelte';

  interface Props {
    row: AcRowView;
    expanded: boolean;
    selected: boolean;
    focused: boolean;
    readonly?: boolean;
    reviewing?: boolean;
    noSource: boolean;
    onexpand: () => void;
    onselect: () => void;
    onfocus: () => void;
    onkey: (e: KeyboardEvent) => void;
    onfile: (path: string) => void;
  }
  let {
    row,
    expanded,
    selected,
    focused,
    readonly = false,
    reviewing = false,
    noSource,
    onexpand,
    onselect,
    onfocus,
    onkey,
    onfile
  }: Props = $props();

  let el = $state<HTMLDivElement | null>(null);
  $effect(() => {
    if (focused && el && document.activeElement?.closest('.ac-ledger')) el.focus();
  });

  const verdict = $derived(shownVerdict(row));
  const selectable = $derived(!readonly && canSend(row));
  const fixing = $derived(!readonly && row.verdict === 'fix_in_progress');
  const pending = $derived(reviewing && row.in_gate && row.verdict === 'not_reviewed');
  const heading = $derived(
    {
      pass: 'Reviewer found it working',
      fail: 'Reviewer found a problem',
      cant_verify: "Reviewer couldn't verify this",
      checked_manually: 'Checked manually',
      skipped: 'Skipped with Push anyway',
      follows_design: 'Not reviewed against the AC',
      fix_in_progress: 'Sent to the agent; fix in progress',
      not_reviewed: 'Not reviewed yet'
    }[verdict] ?? ''
  );
</script>

<div
  bind:this={el}
  class="ac-row"
  role="row"
  tabindex={focused ? 0 : -1}
  aria-expanded={expanded}
  aria-selected={selectable ? selected : undefined}
  data-stale={row.stale ? 'true' : undefined}
  data-fixing={fixing ? 'true' : undefined}
  onclick={(e) => {
    onfocus();
    if ((e.target as HTMLElement).closest('[data-open-file],button[role="checkbox"]')) return;
    onexpand();
  }}
  onkeydown={(e) => {
    if (e.target === el) onkey(e);
  }}
>
  <span class="ac-c ac-sel" role="gridcell">
    {#if fixing}
      <span class="ac-fixing" title="Fix in progress"><Wrench aria-hidden="true" /><span class="sr-only">Fix in progress</span></span>
    {:else if selectable}
      <Checkbox
        tabindex={-1}
        checked={selected}
        onCheckedChange={() => onselect()}
        aria-label="Select {label(row)} for Send to agent"
      />
    {/if}
  </span>
  <span class="ac-c ac-id" role="gridcell">
    <span class="mono">{label(row)}</span>
    {#each rowMarks(row) as m (m)}
      <span class="mark" data-k={m}>{m === 'new' ? 'New' : m === 'changed' ? 'Changed' : 'Removed'}</span>
    {/each}
  </span>
  <span class="ac-c ac-txt" role="gridcell"><span class="clamp">{row.text}</span></span>
  <span class="ac-c ac-d" role="gridcell">
    {#if !noSource}<AcChip kind="design" value={row.design} />{/if}
  </span>
  <span class="ac-c ac-v" role="gridcell"><AcChip kind="verdict" value={verdict} stale={row.stale} {pending} /></span>
  <span class="ac-c ac-x" role="gridcell">
    <Button variant="ghost" size="icon-xs" tabindex={-1} aria-label="{expanded ? 'Collapse' : 'Expand'} {label(row)}" onclick={onexpand}>
      {#if expanded}<ChevronDown />{:else}<ChevronRight />{/if}
    </Button>
  </span>
</div>
{#if expanded}
  <div class="ac-detail" role="row">
    <div role="gridcell" class="ac-ev">
      <div class="ev-col">
        <div class="ev-h">Verdict{#if row.stale}<span class="muted"> (from before the code changed)</span>{/if}</div>
        <p class="ev-lead">{heading}</p>
        {#if row.decided_by && (verdict === 'checked_manually' || verdict === 'skipped')}
          <p>By {row.decided_by}{#if row.reason}: {row.reason}{/if}</p>
        {/if}
        {#if verdict === 'follows_design'}
          <p class="muted">
            You marked this AC as outdated. The agent follows the design, the row is left out of the gate, and the PM was
            asked to update the AC in Lark.
          </p>
        {/if}
        {#if row.evidence}
          <p class="ev-text">
            {#each evidenceParts(row.evidence) as part, i (i)}
              {#if part.kind === 'file'}
                <button class="ev-file" data-open-file title="Open in Changes" onclick={() => onfile(part.path)}>
                  <FileCode class="icon-sm" aria-hidden="true" /><span>{part.text}</span>
                </button>
              {:else}{part.text}{/if}
            {/each}
          </p>
        {/if}
      </div>
      <div class="ev-col">
        <div class="ev-h">Design</div>
        {#if noSource}
          <p class="muted">No design source for this run.</p>
        {:else}
          <p class="ev-lead">{DESIGN_META[row.design]?.label ?? row.design}</p>
          {#if row.design_note}<p>{row.design_note}</p>{/if}
          {#if row.design === 'not_checked'}<p class="muted">Planning has not reported on this AC yet.</p>{/if}
        {/if}
      </div>
    </div>
  </div>
{/if}
