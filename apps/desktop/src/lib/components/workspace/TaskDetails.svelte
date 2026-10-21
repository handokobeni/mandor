<script lang="ts">
  import { runLabel } from '#lib/board.ts';
  import { relTime } from '#lib/board.ts';
  import { board } from '#lib/stores/board.svelte.ts';
  import { people } from '#lib/stores/people.svelte.ts';
  import RunStatusBadge from '#lib/components/mandor/RunStatusBadge.svelte';
  import type { CardView } from '#lib/protocol/CardView.ts';
  import type { RunView } from '#lib/protocol/RunView.ts';
  import type { TaskDetailsView } from '#lib/protocol/TaskDetailsView.ts';

  interface Props {
    run: RunView;
    card: CardView | undefined;
    details?: TaskDetailsView | null;
    /** Tampilkan bagian run (panel samping). */
    withRun?: boolean;
  }
  let { run, card, details = null, withRun = false }: Props = $props();
</script>

<dl class="kv">
  <dt>Lark status</dt>
  <dd>{card?.status ?? '—'}</dd>
  <dt>Readiness</dt>
  <dd>{card?.readiness ?? '—'}</dd>
  <dt>Sprint</dt>
  <dd>{card?.sprint_ids.map((id) => board.sprintName(id)).join(', ') || '—'}</dd>
  <dt>Assignees</dt>
  <dd>{details?.assignees.map((id) => people.name(id)).join(', ') || '—'}</dd>
  <dt>Reporter</dt>
  <dd>{details?.reporter ? people.name(details.reporter) : '—'}</dd>
  <dt>Claimed by</dt>
  <dd>{card?.owner ? people.name(card.owner) : '—'}</dd>
  <dt>Repo</dt>
  <dd class="mono">{run.repo_id}</dd>
</dl>
{#if withRun}
  <hr class="separator" />
  <dl class="kv">
    <dt>Run</dt>
    <dd><RunStatusBadge state={run.state} label={runLabel(run)} /></dd>
    <dt>Branch</dt>
    <dd class="mono" style="font-size:11px;word-break:break-all">{run.branch ?? '—'}</dd>
    <dt>Started</dt>
    <dd>{run.started_at_ms ? relTime(run.started_at_ms, Date.now()) : '—'}</dd>
    <dt>Last change</dt>
    <dd>{run.updated_at_ms ? relTime(run.updated_at_ms, Date.now()) : '—'}</dd>
    <dt>Started by</dt>
    <dd>{run.automatic ? 'Mandor (automatic fix)' : 'You'}</dd>
  </dl>
{/if}
