<script lang="ts">
  // Konfirmasi menutup run kartu selesai (plan 2026-11-03-001, R4).
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import { toast } from 'svelte-sonner';
  import { confirmLabel, formatBytes, keyLabel, reasonLabel, totalBytes } from '#lib/cleanup.ts';
  import { Button } from '#lib/components/ui/button/index.ts';
  import * as Dialog from '#lib/components/ui/dialog/index.ts';
  import type { CleanupStore } from '#lib/stores/cleanup.svelte.ts';

  interface Props {
    store: CleanupStore;
    open: boolean;
    /** Nama daemon di judul, mis. "on the VM". */
    where?: string;
    ondone?: () => void;
  }
  let { store, open = $bindable(), where = '', ondone }: Props = $props();

  let busy = $state(false);
  const freed = $derived(totalBytes(store.closable));

  async function confirm(): Promise<void> {
    busy = true;
    try {
      const res = await store.closeAll();
      toast.success(res.summary);
      open = false;
      ondone?.();
    } catch (err) {
      toast.error('Could not close the runs', { description: err instanceof Error ? err.message : String(err) });
    } finally {
      busy = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-[560px] max-h-[90vh] overflow-y-auto overflow-x-hidden">
    <Dialog.Header>
      <Dialog.Title>Close finished runs{where ? ` ${where}` : ''}</Dialog.Title>
      <Dialog.Description>
        These cards are Delivered or Waiting Deploy. Their worktrees are deleted; branches and PRs stay on GitHub, and
        Lark is not changed.
      </Dialog.Description>
    </Dialog.Header>
    {#if store.closable.length === 0}
      <p class="muted">No finished runs to close.</p>
    {:else}
      <ul class="rows" aria-label="Runs to close">
        {#each store.closable as r (r.run_id)}
          <li><span class="mono key" title={r.keys.join(', ')}>{keyLabel(r.keys)}</span><span class="title" title={r.title}>{r.title}</span><span class="mono muted">{formatBytes(r.bytes)}</span></li>
        {/each}
      </ul>
    {/if}
    {#if store.skipped.length > 0}
      <p class="muted label">Skipped</p>
      <ul class="rows" aria-label="Skipped runs">
        {#each store.skipped as s (s.run.run_id)}
          <li>
            <span class="mono key" title={s.run.keys.join(', ')}>{keyLabel(s.run.keys)}</span><span class="title" title={s.run.title}>{s.run.title}</span>
            <span class="chip" data-tone="warning">{reasonLabel(s.reason)}</span><span class="mono muted">{formatBytes(s.run.bytes)}</span>
          </li>
        {/each}
      </ul>
    {/if}
    <Dialog.Footer>
      <Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
      <Button variant="destructive" disabled={busy || store.closable.length === 0} onclick={confirm}>
        <Trash2 />{confirmLabel(store.closable.length, freed)}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<style>
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    min-width: 0;
  }
  .rows li {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
    font-size: 13px;
    min-width: 0;
  }
  .rows .key,
  .rows .chip,
  .rows .muted {
    flex-shrink: 0;
  }
  .rows .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .label {
    margin: 12px 0 4px;
  }
</style>
