<script lang="ts">
  import { toast } from 'svelte-sonner';
  import ArrowDown from '@lucide/svelte/icons/arrow-down';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import Play from '@lucide/svelte/icons/play';
  import X from '@lucide/svelte/icons/x';
  import { Button } from '#lib/components/ui/button/index.ts';
  import { canQueue } from '#lib/overnight.ts';
  import { board } from '#lib/stores/board.svelte.ts';
  import { overnight, type QueueTarget } from '#lib/stores/overnight.svelte.ts';

  const items = $derived(overnight.queue?.items ?? []);
  const queued = $derived(new Set(items.map((i) => i.record_id)));
  const candidates = $derived(board.cards.filter((c) => canQueue(c, queued)));

  let pick = $state('');
  let target = $state('');

  function targetOf(value: string): QueueTarget | null {
    if (value.startsWith('repo:')) return { repo_id: value.slice(5) };
    if (value.startsWith('ws:')) {
      const ws = overnight.workspaces.find((w) => w.id === value.slice(3));
      return ws ? { workspace: { id: ws.id, repo_ids: ws.repo_ids } } : null;
    }
    return null;
  }

  async function act(f: () => Promise<void>, done?: string): Promise<void> {
    try {
      await f();
      if (done) toast.success(done);
    } catch (err) {
      toast.error('The VM refused', { description: err instanceof Error ? err.message : String(err) });
    }
  }

  async function add(): Promise<void> {
    const t = targetOf(target);
    if (!pick || !t) return;
    const card = candidates.find((c) => c.record_id === pick);
    await act(() => overnight.add(pick, t), `Queued ${card?.key ?? ''} for the VM`);
    pick = '';
  }

  function targetLabel(repo: string | null, ws: string | null): string {
    return ws ? `workspace ${ws}` : (repo ?? '');
  }
</script>

<section>
  <h2>Queue</h2>
  <div class="settings-card">
    <div class="content">
      {#if items.length === 0}
        <p class="field-help" style="margin:0 0 8px">The queue is empty. Add To Do cards below.</p>
      {:else}
        <ol class="queue">
          {#each items as item, i (item.record_id)}
            <li>
              <span class="mono key">{item.key ?? item.record_id}</span>
              <span class="title" title={item.title}>{item.title}</span>
              <span class="muted">{targetLabel(item.repo_id, item.workspace_id)}</span>
              {#if item.held}<span class="chip" data-tone="warning">Continues</span>{/if}
              {#if item.start_now}<span class="chip" data-tone="ok">Starting now</span>{/if}
              <span class="actions">
                <Button variant="ghost" size="icon-xs" aria-label="Move up" disabled={i === 0} onclick={() => act(() => overnight.move(item.record_id, -1))}><ArrowUp /></Button>
                <Button variant="ghost" size="icon-xs" aria-label="Move down" disabled={i === items.length - 1} onclick={() => act(() => overnight.move(item.record_id, 1))}><ArrowDown /></Button>
                <Button variant="outline" size="xs" disabled={item.start_now} onclick={() => act(() => overnight.startNow(item.record_id), 'The VM starts this card next')}><Play />Start now</Button>
                <Button variant="ghost" size="icon-xs" aria-label="Remove from queue" onclick={() => act(() => overnight.remove(item.record_id))}><X /></Button>
              </span>
            </li>
          {/each}
        </ol>
      {/if}
      <div class="add">
        <select class="native-select" aria-label="Card to queue" bind:value={pick}>
          <option value="">Choose a To Do card…</option>
          {#each candidates as c (c.record_id)}
            <option value={c.record_id}>{c.key ?? c.record_id} · {c.title}</option>
          {/each}
        </select>
        <select class="native-select" aria-label="Repo on the VM" bind:value={target}>
          <option value="">Repo on the VM…</option>
          {#each overnight.repos as r (r.id)}
            <option value={`repo:${r.id}`}>{r.id}</option>
          {/each}
          {#each overnight.workspaces as w (w.id)}
            <option value={`ws:${w.id}`}>Workspace {w.name}</option>
          {/each}
        </select>
        <Button size="sm" disabled={!pick || !target} onclick={add}>Add to queue</Button>
      </div>
      {#if overnight.queue && overnight.queue.dequeued.length > 0}
        <p class="field-help" style="margin:10px 0 4px">Removed from the queue this cycle:</p>
        <ul class="dequeued">
          {#each overnight.queue.dequeued as d (d.record_id + d.at_ms)}
            <li><span class="mono">{d.key ?? d.record_id}</span> · {d.reason}</li>
          {/each}
        </ul>
      {/if}
    </div>
  </div>
</section>

<style>
  .queue,
  .dequeued {
    margin: 0 0 8px;
    padding: 0;
    list-style: none;
  }
  .queue li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
    border-bottom: 1px solid var(--border);
  }
  .queue .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .queue .actions {
    display: flex;
    gap: 2px;
  }
  .add {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .native-select {
    height: 32px;
    border: 1px solid var(--input);
    border-radius: 8px;
    background: var(--card);
    color: var(--foreground);
    padding: 0 8px;
    max-width: 320px;
  }
  .dequeued li {
    font-size: 13px;
  }
</style>
