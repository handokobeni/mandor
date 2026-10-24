<script lang="ts">
  import { toast } from 'svelte-sonner';
  import ArrowLeftRight from '@lucide/svelte/icons/arrow-left-right';
  import GitPullRequest from '@lucide/svelte/icons/git-pull-request';
  import Send from '@lucide/svelte/icons/send';
  import { Button } from '#lib/components/ui/button/index.ts';
  import { Textarea } from '#lib/components/ui/textarea/index.ts';
  import { holdLabel, reportGroups, usageText } from '#lib/overnight.ts';
  import type { AcRowView } from '#lib/protocol/AcRowView.ts';
  import type { ReportCardView } from '#lib/protocol/ReportCardView.ts';
  import type { Rpc } from '#lib/rpc.ts';
  import { nav } from '#lib/stores/nav.svelte.ts';
  import { overnight } from '#lib/stores/overnight.svelte.ts';
  import { runs } from '#lib/stores/runs.svelte.ts';

  let { laptop, laptopDaemonId }: { laptop: Rpc; laptopDaemonId: string | null } = $props();

  const report = $derived(overnight.report);
  const groups = $derived(report ? reportGroups(report) : []);

  let busy = $state<string | null>(null);
  let answers = $state<Record<string, string>>({});
  /** Baris AC yang memblokir per run, setelah dibuka. */
  let gate = $state<Record<string, AcRowView[]>>({});
  let confirmed = $state<Record<string, string[]>>({});
  let reasons = $state<Record<string, string>>({});

  async function act(card: ReportCardView, f: () => Promise<void>, done: string): Promise<void> {
    busy = card.run_id;
    try {
      await f();
      toast.success(done);
    } catch (err) {
      toast.error('The VM refused', { description: err instanceof Error ? err.message : String(err) });
    } finally {
      busy = null;
    }
  }

  async function openGate(card: ReportCardView): Promise<void> {
    try {
      const ledger = await overnight.ledger(card.run_id);
      gate[card.run_id] = ledger.rows.filter((r) => r.verdict === 'cant_verify' || r.verdict === 'fail');
      confirmed[card.run_id] = [];
    } catch (err) {
      toast.error('Could not load the AC', { description: err instanceof Error ? err.message : String(err) });
    }
  }

  function toggle(runId: string, acId: string): void {
    const cur = confirmed[runId] ?? [];
    confirmed[runId] = cur.includes(acId) ? cur.filter((a) => a !== acId) : [...cur, acId];
  }

  function key(c: ReportCardView): string {
    return c.key ?? c.record_id;
  }

  async function takeover(card: ReportCardView): Promise<void> {
    if (!laptopDaemonId) {
      toast.error('Update Mandor on this laptop to take cards over from the VM');
      return;
    }
    busy = card.run_id;
    try {
      const runId = await overnight.takeover(card, laptop, laptopDaemonId);
      await runs.load();
      nav.run(runId);
      toast.success(`${key(card)} is on this laptop now`);
    } catch (err) {
      toast.error('Take over failed', { description: err instanceof Error ? err.message : String(err) });
    } finally {
      busy = null;
    }
  }
</script>

<section>
  <h2>This cycle</h2>
  {#if !report}
    <p class="field-help">Loading…</p>
  {:else}
    <div class="settings-card">
      <div class="content summary">
        <span>Usage: <b>{usageText(report)}</b></span>
        <span>Reached review without Waiting Fix: <b>{report.reviewed_without_fix}</b></span>
        {#if report.dequeued.length > 0}<span>Removed from queue: <b>{report.dequeued.length}</b></span>{/if}
      </div>
    </div>
    {#if groups.length === 0}
      <p class="field-help">No VM cards yet in this cycle.</p>
    {/if}
    {#each groups as g (g.id)}
      <h3>{g.label} <span class="muted mono">{g.cards.length}</span></h3>
      {#each g.cards as c (c.run_id)}
        <div class="settings-card report-card">
          <div class="content">
            <div class="head">
              <span class="mono key">{key(c)}</span>
              <span class="title">{c.title}</span>
              {#if c.pr_url}
                <a class="pr-link" href={c.pr_url} target="_blank" rel="noopener noreferrer"><GitPullRequest class="icon-sm" aria-hidden="true" />PR</a>
              {/if}
              <span class="muted">AC {c.ac_pass} pass · {c.ac_fail} fail · {c.ac_cant_verify} can't verify</span>
            </div>
            {#if c.outcome === 'held'}
              <p class="hold"><span class="chip" data-tone="warning">{holdLabel(c.hold_reason)}</span> {c.hold_note ?? ''}</p>
            {/if}

            <div class="actions">
              {#if c.outcome === 'draft'}
                <Button size="sm" disabled={busy === c.run_id} onclick={() => act(c, () => overnight.sendToReview(c.run_id), `${key(c)} sent to review`)}>
                  <Send />Send to review
                </Button>
              {/if}
              {#if c.outcome === 'held' && c.hold_reason === 'ac_gate' && !gate[c.run_id]}
                <Button size="sm" variant="outline" onclick={() => openGate(c)}>Decide AC…</Button>
              {/if}
              {#if c.outcome === 'held' && c.hold_reason === 'needs_confirmation'}
                <Button size="sm" disabled={busy === c.run_id} onclick={() => act(c, () => overnight.decide(c.run_id, [], null), `${key(c)} pushed as a draft PR`)}>
                  Confirm and push
                </Button>
              {/if}
              {#if c.outcome === 'held' || c.outcome === 'failed' || c.entered_waiting_fix}
                <Button size="sm" variant="outline" disabled={busy === c.run_id} onclick={() => takeover(c)}>
                  <ArrowLeftRight />Take over to laptop
                </Button>
              {/if}
            </div>

            {#if c.outcome === 'held' && (c.hold_reason === 'stuck' || c.hold_reason === 'usage_limit')}
              <div class="answer">
                <Textarea rows={2} placeholder="Answer the agent, then the card goes back to the queue" bind:value={answers[c.run_id]} />
                <Button size="sm" disabled={busy === c.run_id} onclick={() => act(c, () => overnight.answer(c, answers[c.run_id] ?? ''), `${key(c)} is back in the queue`)}>
                  Answer and queue
                </Button>
              </div>
            {/if}

            {#if gate[c.run_id]}
              <div class="gate">
                {#each gate[c.run_id] as row (row.ac_id)}
                  <label class="gate-row">
                    {#if row.verdict === 'cant_verify'}
                      <input type="checkbox" checked={(confirmed[c.run_id] ?? []).includes(row.ac_id)} onchange={() => toggle(c.run_id, row.ac_id)} />
                      <span><b>{row.label ?? 'AC'}</b> I checked this myself · {row.text}</span>
                    {:else}
                      <span class="chip" data-tone="failed">Fail</span>
                      <span><b>{row.label ?? 'AC'}</b> {row.text}</span>
                    {/if}
                  </label>
                {/each}
                {#if gate[c.run_id].some((r) => r.verdict === 'fail')}
                  <Textarea rows={2} placeholder="Why push even though an AC fails" bind:value={reasons[c.run_id]} />
                {/if}
                <Button size="sm" disabled={busy === c.run_id} onclick={() => act(c, () => overnight.decide(c.run_id, confirmed[c.run_id] ?? [], reasons[c.run_id] ?? null), `${key(c)} pushed as a draft PR`)}>
                  Push and create draft PR
                </Button>
              </div>
            {/if}
          </div>
        </div>
      {/each}
    {/each}
  {/if}
</section>

<style>
  h2 {
    font-size: 14px;
    font-weight: 600;
    margin: 0 0 8px;
  }
  h3 {
    font-size: 13px;
    font-weight: 600;
    margin: 14px 0 6px;
  }
  .summary {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
  }
  .report-card {
    margin-bottom: 8px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .head .title {
    flex: 1;
    min-width: 0;
  }
  .hold {
    margin: 6px 0 0;
  }
  .actions,
  .answer,
  .gate {
    display: flex;
    gap: 8px;
    margin-top: 8px;
    flex-wrap: wrap;
    align-items: center;
  }
  .gate {
    flex-direction: column;
    align-items: stretch;
  }
  .gate-row {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    font-size: 13px;
  }
  .pr-link {
    display: inline-flex;
    gap: 4px;
    align-items: center;
  }
</style>
