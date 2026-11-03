<script lang="ts">
  // Settings › Storage: ukuran worktree run, penutupan run selesai, dan
  // setelan otomatis untuk satu daemon (plan 2026-11-03-001, R5, R7, R9).
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import { toast } from 'svelte-sonner';
  import { closeLabel, formatBytes, keyLabel, totalBytes } from '#lib/cleanup.ts';
  import CloseFinishedDialog from '#lib/components/cleanup/CloseFinishedDialog.svelte';
  import * as AlertDialog from '#lib/components/ui/alert-dialog/index.ts';
  import { Button, buttonVariants } from '#lib/components/ui/button/index.ts';
  import { Switch } from '#lib/components/ui/switch/index.ts';
  import type { RunStorageView } from '#lib/protocol/RunStorageView.ts';
  import type { Rpc } from '#lib/rpc.ts';
  import type { CleanupStore } from '#lib/stores/cleanup.svelte.ts';
  import { nav } from '#lib/stores/nav.svelte.ts';
  import { runMeta } from '#lib/board.ts';

  interface Props {
    rpc: Rpc;
    store: CleanupStore;
    /** Run VM tidak bisa dibuka di workspace laptop. */
    vm?: boolean;
  }
  let { rpc, store, vm = false }: Props = $props();

  let rows = $state<RunStorageView[] | null>(null);
  let auto = $state(false);
  let dialog = $state(false);
  let closing = $state<RunStorageView | null>(null);

  async function reload(): Promise<void> {
    try {
      const [list, on] = await Promise.all([store.storage(), store.autoClose(), store.load()]);
      rows = list;
      auto = on;
    } catch (err) {
      rows = [];
      toast.error('Could not read storage', { description: err instanceof Error ? err.message : String(err) });
    }
  }

  $effect(() => {
    void store;
    void reload();
  });

  async function setAuto(on: boolean): Promise<void> {
    auto = on;
    try {
      await store.setAutoClose(on);
    } catch (err) {
      auto = !on;
      toast.error('Could not save the setting', { description: err instanceof Error ? err.message : String(err) });
    }
  }

  async function closeOne(r: RunStorageView): Promise<void> {
    closing = null;
    try {
      await rpc.request('runs.close', { run_id: r.run_id });
      toast.success(`Closed the run for ${r.keys.join(', ')}`);
      await reload();
    } catch (err) {
      toast.error('Could not close the run', { description: err instanceof Error ? err.message : String(err) });
    }
  }

  const total = $derived(totalBytes(rows ?? []));
  const freeable = $derived(totalBytes(store.closable));
  const label = $derived(closeLabel(store.closable.length));
</script>

<section>
  <h2>Storage</h2>
  <p class="lede">
    Every run keeps its own worktree, including installed dependencies. Close runs you no longer need to free space.
  </p>
  <div class="settings-card">
    <div class="content">
      <p class="summary">
        All worktrees: <b>{rows ? formatBytes(total) : '…'}</b>
        {#if store.closable.length > 0}· {store.closable.length} finished {store.closable.length === 1 ? 'run' : 'runs'} can free <b>{formatBytes(freeable)}</b>{/if}
      </p>
      <div class="row-setting">
        <label class="txt" for="cleanup-auto">
          Close finished runs automatically
          <span>After each Lark sync, at most every 10 minutes. Runs with unpushed changes are never closed.</span>
        </label>
        <Switch id="cleanup-auto" checked={auto} onCheckedChange={(on) => void setAuto(on)} />
      </div>
      {#if label}
        <Button variant="outline" size="sm" onclick={() => (dialog = true)}><Trash2 />{label}</Button>
      {:else}
        <p class="muted" style="margin:0">No finished runs to close. Cards count as finished when they are Delivered or Waiting Deploy.</p>
      {/if}
    </div>
  </div>
</section>

<section>
  <h2>Open runs</h2>
  <p class="lede">Largest first. Closing a run removes its worktree; branches and PRs stay on GitHub.</p>
  {#if rows === null}
    <p class="muted">Loading…</p>
  {:else if rows.length === 0}
    <p class="muted">No open runs.</p>
  {:else}
    <ul class="storage" aria-label="Open runs by size">
      {#each rows as r (r.run_id)}
        <li>
          <span class="mono key" title={r.keys.join(', ')}>{keyLabel(r.keys)}</span>
          <span class="title" title={r.title}>{r.title}</span>
          <span class="muted meta">{runMeta(r.run_state).label} · {[...new Set(r.card_status)].join(', ')}</span>
          <span class="mono size">{formatBytes(r.bytes)}</span>
          {#if !vm}<Button variant="ghost" size="xs" onclick={() => nav.run(r.run_id)}>Open</Button>{/if}
          <Button variant="ghost" size="xs" onclick={() => (closing = r)}>Close run</Button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<CloseFinishedDialog {store} bind:open={dialog} where={vm ? 'on the VM' : ''} ondone={() => void reload()} />

{#if closing}
  {@const r = closing}
  <AlertDialog.Root open onOpenChange={(o) => !o && (closing = null)}>
    <AlertDialog.Content>
      <AlertDialog.Header>
        <AlertDialog.Title>Close the run for {r.keys.join(', ')}?</AlertDialog.Title>
        <AlertDialog.Description>
          Mandor stops the agent and removes the worktree ({formatBytes(r.bytes)}). A worktree with unpushed changes is
          kept. The branch stays on origin, and Lark is not changed.
        </AlertDialog.Description>
      </AlertDialog.Header>
      <AlertDialog.Footer>
        <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
        <AlertDialog.Action class={buttonVariants({ variant: 'destructive' })} onclick={() => void closeOne(r)}>Close run</AlertDialog.Action>
      </AlertDialog.Footer>
    </AlertDialog.Content>
  </AlertDialog.Root>
{/if}

<style>
  .summary {
    margin: 0 0 8px;
  }
  .storage {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .storage li {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
    font-size: 13px;
    min-width: 0;
  }
  .storage .key,
  .storage .size,
  .storage .meta {
    flex-shrink: 0;
  }
  .storage .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
