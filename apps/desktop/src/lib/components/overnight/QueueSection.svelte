<script lang="ts">
  import { toast } from 'svelte-sonner';
  import ArrowDown from '@lucide/svelte/icons/arrow-down';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import Play from '@lucide/svelte/icons/play';
  import X from '@lucide/svelte/icons/x';
  import Check from '@lucide/svelte/icons/check';
  import ChevronsUpDown from '@lucide/svelte/icons/chevrons-up-down';
  import { Button } from '#lib/components/ui/button/index.ts';
  import * as Command from '#lib/components/ui/command/index.ts';
  import * as Popover from '#lib/components/ui/popover/index.ts';
  import * as Select from '#lib/components/ui/select/index.ts';
  import { canQueue } from '#lib/overnight.ts';
  import { board } from '#lib/stores/board.svelte.ts';
  import { overnight, type QueueTarget } from '#lib/stores/overnight.svelte.ts';

  const items = $derived(overnight.queue?.items ?? []);
  const queued = $derived(new Set(items.map((i) => i.record_id)));
  const candidates = $derived(board.cards.filter((c) => canQueue(c, queued)));

  let pick = $state('');
  let picking = $state(false);
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

  const pickLabel = $derived.by(() => {
    const c = candidates.find((x) => x.record_id === pick);
    return c ? `${c.key ?? c.record_id} · ${c.title}` : 'Choose a To Do card…';
  });
  const targetText = $derived.by(() => {
    if (target.startsWith('repo:')) return target.slice(5);
    const ws = overnight.workspaces.find((w) => `ws:${w.id}` === target);
    return ws ? `Workspace ${ws.name}` : 'Repo on the VM…';
  });

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
        <Popover.Root bind:open={picking}>
          <Popover.Trigger
            class="border-input dark:bg-input/30 dark:hover:bg-input/50 flex h-8 w-80 max-w-full items-center justify-between gap-1.5 rounded-lg border bg-transparent py-2 pr-2 pl-2.5 text-sm"
            aria-label="Card to queue"
            role="combobox"
            aria-expanded={picking}
          >
            <span class="min-w-0 truncate" class:placeholder={!pick}>{pickLabel}</span>
            <ChevronsUpDown class="text-muted-foreground size-4 shrink-0" />
          </Popover.Trigger>
          <Popover.Content class="w-(--bits-popover-anchor-width) p-0" align="start">
            <Command.Root>
              <Command.Input placeholder="Search key or title" />
              <Command.List>
                <Command.Empty>No To Do card matches.</Command.Empty>
                {#each candidates as c (c.record_id)}
                  {@const text = `${c.key ?? c.record_id} · ${c.title}`}
                  <Command.Item
                    value={`${c.key ?? ''} ${c.title} ${c.record_id}`}
                    onSelect={() => {
                      pick = c.record_id;
                      picking = false;
                    }}
                  >
                    <span class="min-w-0 flex-1 truncate" title={text}>{text}</span>
                    {#if pick === c.record_id}<Check class="size-4 shrink-0" />{/if}
                  </Command.Item>
                {/each}
              </Command.List>
            </Command.Root>
          </Popover.Content>
        </Popover.Root>
        <Select.Root type="single" bind:value={target}>
          <Select.Trigger class="w-44" aria-label="Repo on the VM">
            <span class="min-w-0 truncate" class:placeholder={!target}>{targetText}</span>
          </Select.Trigger>
          <Select.Content class="w-(--bits-select-anchor-width)">
            {#each overnight.repos as r (r.id)}
              <Select.Item value={`repo:${r.id}`} label={r.id} />
            {/each}
            {#each overnight.workspaces as w (w.id)}
              <Select.Item value={`ws:${w.id}`} label={`Workspace ${w.name}`} />
            {/each}
          </Select.Content>
        </Select.Root>
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
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }
  .placeholder {
    color: var(--muted-foreground);
  }
  .dequeued li {
    font-size: 13px;
  }
</style>
