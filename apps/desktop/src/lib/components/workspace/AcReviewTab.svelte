<script lang="ts">
  // Tab AC review (brief §4.1): strip ringkasan dengan state gerbang, baris buku
  // AC dengan navigasi keyboard, pilihan baris Fail untuk Send to agent, dan
  // riwayat review.
  import CircleMinus from '@lucide/svelte/icons/circle-minus';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import ClockArrowLeft from '@lucide/svelte/icons/clock-arrow-left';
  import Frame from '@lucide/svelte/icons/frame';
  import ListX from '@lucide/svelte/icons/list-x';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import ScanSearch from '@lucide/svelte/icons/scan-search';
  import SendHorizontal from '@lucide/svelte/icons/send-horizontal';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import {
    activeRows,
    counts,
    gate,
    label,
    moveFocus,
    noDesignSource,
    removedRows,
    toggleSelection,
    VERDICT_META
  } from '#lib/ac-review.ts';
  import { relTime } from '#lib/board.ts';
  import * as Alert from '#lib/components/ui/alert/index.ts';
  import { Badge } from '#lib/components/ui/badge/index.ts';
  import { Button } from '#lib/components/ui/button/index.ts';
  import type { AcRowView } from '#lib/protocol/AcRowView.ts';
  import type { LedgerStore } from '#lib/stores/ledger.svelte.ts';
  import AcRow from './AcRow.svelte';
  import ReviewHistory from './ReviewHistory.svelte';

  interface Props {
    taskKey: string;
    store: LedgerStore;
    reviewing: boolean;
    /** Agent pelaksana sedang bekerja: review belum bisa dimulai. */
    agentBusy: boolean;
    now: number;
    onreview: () => void;
    onsend: (acIds: string[]) => void;
    onfile: (path: string) => void;
  }
  let { taskKey, store, reviewing, agentBusy, now, onreview, onsend, onfile }: Props = $props();

  let focusIdx = $state(0);
  let expanded = $state<Record<string, boolean>>({});
  let selected = $state(new Set<string>());
  let viewing = $state<string | null>(null);

  const ledger = $derived(store.ledger);
  const rows = $derived(ledger ? activeRows(ledger.rows) : []);
  const removed = $derived(ledger ? removedRows(ledger.rows) : []);
  const g = $derived(gate(rows, ledger?.gate_mode ?? 'block', reviewing));
  const c = $derived(counts(rows));
  const noSource = $derived(ledger ? noDesignSource(ledger) : true);
  const last = $derived(store.history.find((r) => r.status !== 'running') ?? null);
  const failedReview = $derived(last && (last.status === 'failed' || last.status === 'cancelled') ? last : null);
  const view = $derived(viewing ? (store.history.find((r) => r.id === viewing) ?? null) : null);

  // Verdict review lama (hanya baca) ditampilkan di atas baris saat ini.
  const shown = $derived<AcRowView[]>(
    view
      ? rows.map((r) => {
          const v = view.verdicts.find((x) => x.ac_id === r.ac_id);
          return { ...r, verdict: v?.verdict ?? 'not_reviewed', evidence: v?.evidence ?? null, stale: false };
        })
      : rows
  );
  // Pilihan yang tidak lagi Fail dibuang (mis. setelah review baru).
  $effect(() => {
    const ids = new Set(rows.filter((r) => r.verdict === 'fail').map((r) => r.ac_id));
    if ([...selected].some((id) => !ids.has(id))) selected = new Set([...selected].filter((id) => ids.has(id)));
  });

  const TRIGGER: Record<string, string> = {
    manual: 'Manual',
    push_and_pr: 'Push & create PR',
    automatic_fix: 'Automatic fix'
  };

  function onkey(e: KeyboardEvent, r: AcRowView): void {
    if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) {
      e.preventDefault();
      focusIdx = moveFocus(focusIdx, e.key, shown.length);
    } else if (e.key === ' ') {
      e.preventDefault();
      if (!view) selected = toggleSelection(selected, r);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      expanded[r.ac_id] = !expanded[r.ac_id];
    }
  }

  function send(): void {
    const ids = [...selected];
    selected = new Set();
    onsend(ids);
  }

  const COUNT_ORDER = [
    ['pass', 'pass'],
    ['fail', 'fail'],
    ['cant', 'cant_verify'],
    ['manual', 'checked_manually'],
    ['skipped', 'skipped'],
    ['follows', 'follows_design'],
    ['fixing', 'fix_in_progress'],
    ['notReviewed', 'not_reviewed']
  ] as const;
</script>

{#if !ledger}
  <div class="ac-wrap">
    {#if store.error}
      <Alert.Root variant="destructive">
        <CircleX />
        <Alert.Title>Couldn't load the AC ledger</Alert.Title>
        <Alert.Description>{store.error}</Alert.Description>
      </Alert.Root>
    {:else}
      <p class="muted"><LoaderCircle class="spin icon-sm" aria-hidden="true" /> Loading the AC ledger…</p>
    {/if}
  </div>
{:else if rows.length === 0 && removed.length === 0}
  <div class="empty">
    <span class="ico"><ListX aria-hidden="true" /></span>
    <h3>No acceptance criteria for this task</h3>
    <p>
      Nothing to review against. Link AC to the task in Lark and Mandor adds them here on the next sync. Push &amp;
      create PR is not gated.
    </p>
  </div>
{:else}
  <div class="ac-wrap">
    {#if view}
      <Alert.Root>
        <ClockArrowLeft />
        <Alert.Title>Viewing the review from {relTime(view.started_at_ms, now)} (read-only)</Alert.Title>
        <Alert.Description>
          {TRIGGER[view.trigger] ?? view.trigger}{view.browser_used ? ' · browser used' : ' · no browser'}
          {#if view.status !== 'finished'} · stopped before finishing{/if}
        </Alert.Description>
        <Alert.Action><Button variant="outline" size="sm" onclick={() => (viewing = null)}>Back to current</Button></Alert.Action>
      </Alert.Root>
    {:else}
      <div class="ac-summary">
        <div class="ac-sum-top">
          <span class="gate-state" data-tone={g.tone}>
            {#if g.state === 'reviewing'}<LoaderCircle class="spin" aria-hidden="true" />{/if}<span>{g.text}</span>
          </span>
          {#if ledger.gate_mode === 'warn'}
            <Badge variant="outline" title="Set per workspace or repo in Settings"><TriangleAlert />Warn mode</Badge>
          {/if}
          <span class="muted ac-meta">
            {#if reviewing}
              Review running
            {:else if last}
              Last review {relTime(last.started_at_ms, now)} · {TRIGGER[last.trigger] ?? last.trigger}{last.browser_used
                ? ' · browser used'
                : ' · no browser'}
            {:else}
              No review yet
            {/if}
          </span>
          <span class="right">
            {#if agentBusy && !reviewing}<span class="muted" id="ac-run-why">Review when the agent is waiting for you after coding.</span>{/if}
            {#if reviewing}
              <Button variant="outline" size="sm" disabled><LoaderCircle class="spin" />Reviewing…</Button>
            {:else}
              <Button
                variant={g.state === 'review_needed' ? 'default' : 'outline'}
                size="sm"
                disabled={agentBusy}
                aria-describedby={agentBusy ? 'ac-run-why' : undefined}
                onclick={onreview}
              >
                {#if failedReview}<RotateCcw />Retry review{:else}<ScanSearch />Run review{/if}
              </Button>
            {/if}
          </span>
        </div>
        <div class="ac-sum-bottom">
          {#each COUNT_ORDER as [k, v] (k)}
            {#if c[k] > 0}
              <span class="count-chip" data-tone={VERDICT_META[v].tone}><b>{c[k]}</b> {VERDICT_META[v].label}</span>
            {/if}
          {/each}
          {#if noSource}
            <span class="src" data-none="true"><Frame class="icon-sm" aria-hidden="true" />No design source</span>
          {:else}
            <span class="src"><Frame class="icon-sm" aria-hidden="true" />Design source <b>{ledger.design_sources.join(', ')}</b></span>
          {/if}
        </div>
      </div>
      {#if failedReview && !reviewing}
        <Alert.Root variant="destructive">
          <CircleX />
          <Alert.Title>Review {failedReview.status === 'cancelled' ? 'cancelled' : 'failed'}</Alert.Title>
          <Alert.Description>
            {failedReview.error ?? 'The review stopped before finishing.'} Unreported AC stay Not reviewed.
          </Alert.Description>
        </Alert.Root>
      {/if}
      {#if c.stale > 0 && !reviewing}
        <Alert.Root>
          <ClockArrowLeft class="text-status-warning" />
          <Alert.Title>Results are stale</Alert.Title>
          <Alert.Description>
            The code changed after the review. Previous verdicts are kept for reference. Run a review before pushing.
          </Alert.Description>
        </Alert.Root>
      {/if}
      {#if ledger.sync_error}
        <Alert.Root>
          <TriangleAlert class="text-status-warning" />
          <Alert.Title>Couldn't refresh the AC from Lark</Alert.Title>
          <Alert.Description>Showing the copy Mandor has. {ledger.sync_error}</Alert.Description>
        </Alert.Root>
      {/if}
      {#if selected.size > 0}
        <div class="ac-selbar">
          <span><b>{selected.size}</b> failed AC selected</span>
          <Button variant="ghost" size="sm" onclick={() => (selected = new Set())}>Clear</Button>
          <Button size="sm" disabled={agentBusy || reviewing} onclick={send}><SendHorizontal />Send to agent</Button>
        </div>
      {/if}
    {/if}
    <div class="ac-ledger" role="grid" aria-label="AC ledger for {taskKey}" aria-multiselectable="true" aria-readonly={!!view}>
      <div class="ac-row ac-head" role="row">
        <span class="ac-c ac-sel" role="columnheader"><span class="sr-only">Select</span></span>
        <span class="ac-c ac-id" role="columnheader">AC</span>
        <span class="ac-c ac-txt" role="columnheader">Criterion</span>
        <span class="ac-c ac-d" role="columnheader">
          Design{#if noSource}<span class="chip" data-tone="muted"><Frame aria-hidden="true" />No design source</span>{/if}
        </span>
        <span class="ac-c ac-v" role="columnheader">Verdict</span>
        <span class="ac-c ac-x" role="columnheader"><span class="sr-only">Expand</span></span>
      </div>
      {#each shown as r, i (r.ac_id)}
        <AcRow
          row={r}
          expanded={!!expanded[r.ac_id]}
          selected={selected.has(r.ac_id)}
          focused={i === Math.min(focusIdx, shown.length - 1)}
          readonly={!!view}
          reviewing={reviewing && !view}
          {noSource}
          onexpand={() => (expanded[r.ac_id] = !expanded[r.ac_id])}
          onselect={() => (selected = toggleSelection(selected, r))}
          onfocus={() => (focusIdx = i)}
          onkey={(e) => onkey(e, r)}
          {onfile}
        />
      {/each}
      {#each removed as r (r.ac_id)}
        <div class="ac-row" role="row" data-removed="true">
          <span class="ac-c ac-sel" role="gridcell"></span>
          <span class="ac-c ac-id" role="gridcell"><span class="mono">{label(r)}</span></span>
          <span class="ac-c ac-txt" role="gridcell"><span class="clamp">{r.text}</span></span>
          <span class="ac-c ac-rm" role="gridcell">
            <span class="chip" data-tone="muted"><CircleMinus aria-hidden="true" />Removed in Lark</span>
            <span class="muted">Not counted, not in the gate</span>
          </span>
        </div>
      {/each}
    </div>
    <ReviewHistory history={store.history} {viewing} {now} onview={(id) => (viewing = id)} />
  </div>
{/if}
