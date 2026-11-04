<script lang="ts">
  // Konfirmasi revisi prototype tanpa Run app; dipakai untuk prototype di VM
  // yang dijalankan sendiri saat lembur (plan 2026-11-03-002, R15, R28).
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { onMount } from 'svelte';
  import { toast } from 'svelte-sonner';
  import { Button } from '#lib/components/ui/button/index.ts';
  import * as Dialog from '#lib/components/ui/dialog/index.ts';
  import type { DesignSourceView } from '#lib/protocol/DesignSourceView.ts';
  import type { PrototypeRevisionView } from '#lib/protocol/PrototypeRevisionView.ts';
  import type { Rpc } from '#lib/rpc.ts';

  interface Props {
    rpc: Rpc;
    source: DesignSourceView;
    onclose: () => void;
  }
  let { rpc, source, onclose }: Props = $props();

  let rev = $state<PrototypeRevisionView | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);
  const upToDate = $derived(rev !== null && rev.confirmed === rev.current);

  onMount(async () => {
    try {
      rev = await rpc.request<PrototypeRevisionView>('design_sources.revision', { id: source.id });
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  });

  async function confirm(): Promise<void> {
    if (!rev) return;
    busy = true;
    try {
      rev = await rpc.request<PrototypeRevisionView>('design_sources.confirm', { id: source.id, revision: rev.current });
      toast.success('Prototype revision confirmed', { description: source.name });
      onclose();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog.Root open onOpenChange={(o) => !o && onclose()}>
  <Dialog.Content class="sm:max-w-[520px] max-h-[90vh] overflow-y-auto">
    <Dialog.Header>
      <Dialog.Title class="text-[length:var(--text-md)]">Prototype code: {source.name}</Dialog.Title>
      <Dialog.Description>
        The prototype only runs at a revision you confirmed. Overnight reviews on the VM skip it until you confirm the
        current code.
      </Dialog.Description>
    </Dialog.Header>
    {#if error}
      <p class="field-error">{error}</p>
    {:else if !rev}
      <p class="text-sm text-muted-foreground"><LoaderCircle class="icon-sm animate-spin" /> Reading the folder…</p>
    {:else if upToDate}
      <p class="text-sm">The current code is confirmed.</p>
    {:else if rev.confirmed === null}
      <p class="text-sm">This prototype has never been confirmed. Check the folder before it runs on this machine.</p>
    {:else if rev.files.length === 0}
      <p class="text-sm">The folder changed since the last confirmation, but the changed files could not be listed.</p>
    {:else}
      <p class="text-sm">{rev.files.length} file{rev.files.length === 1 ? '' : 's'} changed since the last confirmation:</p>
      <ul class="files mono">
        {#each rev.files as f (f)}<li>{f}</li>{/each}
      </ul>
    {/if}
    <p class="muted mono rev">{rev?.current ?? ''}</p>
    <Dialog.Footer>
      <Button variant="ghost" onclick={onclose}>Close</Button>
      {#if rev && !upToDate}
        <Button disabled={busy} onclick={confirm}>Confirm this revision</Button>
      {/if}
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<style>
  .files {
    max-height: 40vh;
    overflow-y: auto;
    font-size: var(--text-xs);
    overflow-wrap: anywhere;
  }
  .rev {
    font-size: var(--text-xs);
    overflow-wrap: anywhere;
  }
</style>
