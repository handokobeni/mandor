<script lang="ts">
  // Banner lab data (plan 2026-11-10-001, U15): sesi yang memegang data lokal
  // repo run ini. Restore gagal menahan review, start app, dan giliran agent
  // repo itu sampai developer mencoba ulang atau menyatakan data pulih.
  import CircleX from '@lucide/svelte/icons/circle-x';
  import Database from '@lucide/svelte/icons/database';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { toast } from 'svelte-sonner';
  import * as AlertDialog from '#lib/components/ui/alert-dialog/index.ts';
  import * as Alert from '#lib/components/ui/alert/index.ts';
  import { Button, buttonVariants } from '#lib/components/ui/button/index.ts';
  import { dataBanner } from '#lib/lab.ts';
  import type { LabView } from '#lib/protocol/LabView.ts';
  import type { Rpc } from '#lib/rpc.ts';

  let { rpc, repoIds }: { rpc: Rpc; repoIds: string[] } = $props();

  let views = $state<LabView[]>([]);
  let busy = $state(false);
  let confirming = $state<string | null>(null);
  /** Restore bisa lama: snapshot dipulihkan per penyimpanan. */
  const RESTORE_TIMEOUT_MS = 30 * 60_000;

  function message(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
  }

  async function load(): Promise<void> {
    const out: LabView[] = [];
    for (const repo_id of repoIds) {
      try {
        out.push(await rpc.request<LabView>('lab.get', { repo_id }));
      } catch {
        // Daemon lama tanpa lab: tidak ada banner.
      }
    }
    views = out;
  }

  $effect(() => {
    void repoIds;
    void load();
    const off = rpc.on('review.progress', () => void load());
    return off;
  });

  const active = $derived(views.filter((v) => v.data_session !== null));

  // Selama sesi berjalan, status dimuat ulang berkala.
  $effect(() => {
    if (active.length === 0) return;
    const timer = setInterval(() => void load(), 10_000);
    return () => clearInterval(timer);
  });

  async function retry(repo_id: string): Promise<void> {
    busy = true;
    try {
      await rpc.request('lab.retry_restore', { repo_id }, { timeoutMs: RESTORE_TIMEOUT_MS });
      toast.success('Lab data restored', { description: repo_id });
    } catch (err) {
      toast.error('The restore still failed', { description: message(err) });
    } finally {
      busy = false;
      await load();
    }
  }

  async function restored(repo_id: string): Promise<void> {
    confirming = null;
    busy = true;
    try {
      await rpc.request('lab.data_restored', { repo_id, discard_snapshot: true });
      toast.success('Lab data marked as restored', { description: repo_id });
    } catch (err) {
      toast.error('Could not update the lab data', { description: message(err) });
    } finally {
      busy = false;
      await load();
    }
  }
</script>

{#each active as v (v.repo_id)}
  {@const b = dataBanner(v.data_session)}
  {#if b}
    <Alert.Root variant={b.tone === 'failed' ? 'destructive' : 'default'}>
      {#if b.tone === 'failed'}<CircleX />{:else if b.tone === 'working'}<LoaderCircle class="spin" />{:else}<Database class="text-status-warning" />{/if}
      <Alert.Title>{b.title} ({v.repo_id})</Alert.Title>
      <Alert.Description>{b.body}</Alert.Description>
      {#if b.retry || b.restored}
        <Alert.Action>
          {#if b.retry}
            <Button variant="outline" size="sm" disabled={busy} onclick={() => void retry(v.repo_id)}>Retry restore</Button>
          {/if}
          {#if b.restored}
            <Button variant="ghost" size="sm" disabled={busy} onclick={() => (confirming = v.repo_id)}>Data restored…</Button>
          {/if}
        </Alert.Action>
      {/if}
    </Alert.Root>
  {/if}
{/each}

{#if confirming}
  {@const repo = confirming}
  <AlertDialog.Root open onOpenChange={(o) => !o && (confirming = null)}>
    <AlertDialog.Content>
      <AlertDialog.Header>
        <AlertDialog.Title>Is the local data of {repo} restored?</AlertDialog.Title>
        <AlertDialog.Description>
          Confirm only after you checked the database and folders yourself. Mandor discards the snapshot it kept and
          lets reviews, app starts, and agent turns use the data again.
        </AlertDialog.Description>
      </AlertDialog.Header>
      <AlertDialog.Footer>
        <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
        <AlertDialog.Action class={buttonVariants({ variant: 'destructive' })} onclick={() => void restored(repo)}
          >Data restored, discard snapshot</AlertDialog.Action
        >
      </AlertDialog.Footer>
    </AlertDialog.Content>
  </AlertDialog.Root>
{/if}
