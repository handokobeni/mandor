<script lang="ts">
  // Riwayat review terbaru dulu; memilih satu menampilkan verdict-nya (hanya baca).
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import ClockArrowLeft from '@lucide/svelte/icons/clock-arrow-left';
  import { relTime } from '#lib/board.ts';
  import type { ReviewView } from '#lib/protocol/ReviewView.ts';

  interface Props {
    history: ReviewView[];
    viewing: string | null;
    now: number;
    onview: (id: string | null) => void;
  }
  let { history, viewing, now, onview }: Props = $props();

  const TRIGGER: Record<string, string> = {
    manual: 'Manual',
    push_and_pr: 'Push & create PR',
    automatic_fix: 'Automatic fix'
  };

  function tally(r: ReviewView): string {
    const n = (v: string) => r.verdicts.filter((x) => x.verdict === v).length;
    const parts = [
      [n('pass') + n('checked_manually'), 'passed'],
      [n('fail'), 'failed'],
      [n('cant_verify'), "can't verify"],
      [n('not_reviewed'), 'not reviewed']
    ] as const;
    return parts
      .filter(([k]) => k > 0)
      .map(([k, l]) => `${k} ${l}`)
      .join(', ');
  }

  function status(r: ReviewView): string | null {
    if (r.status === 'failed') return r.error ?? 'Stopped before finishing';
    if (r.status === 'cancelled') return 'Cancelled';
    if (r.status === 'running') return 'Running';
    return null;
  }
</script>

{#if history.length === 0}
  <div class="ac-history">
    <div class="ac-hist-empty muted">
      <ClockArrowLeft class="icon-sm" aria-hidden="true" />No reviews yet. Each review is kept here with its counts.
    </div>
  </div>
{:else}
  <details class="ac-history" open={viewing !== null}>
    <summary><ChevronRight class="icon-sm" aria-hidden="true" />Review history <span class="muted">({history.length})</span></summary>
    <ol>
      {#each history as r, i (r.id)}
        <li>
          <button class="ac-hist-item" aria-pressed={viewing === r.id} onclick={() => onview(viewing === r.id ? null : r.id)}>
            <span class="when">{relTime(r.started_at_ms, now)}</span>
            <span class="muted">{TRIGGER[r.trigger] ?? r.trigger}{r.browser_used ? ' · browser' : ' · no browser'}</span>
            {#if status(r)}
              <span class="chip" data-tone={r.status === 'failed' ? 'failed' : 'muted'}>
                {#if r.status === 'failed'}<CircleX aria-hidden="true" />{/if}{status(r)}
              </span>
            {/if}
            <span class="cc muted">{tally(r)}</span>
            {#if r.cost_usd !== null}<span class="muted mono">${r.cost_usd.toFixed(2)}</span>{/if}
            {#if i === 0 && r.status === 'finished'}<span class="muted latest">Latest</span>{/if}
          </button>
        </li>
      {/each}
    </ol>
  </details>
{/if}
