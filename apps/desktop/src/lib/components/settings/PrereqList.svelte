<script lang="ts">
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import { Button } from '#lib/components/ui/button/index.ts';
  import type { PrereqView } from '#lib/protocol/PrereqView.ts';
  import type { Rpc } from '#lib/rpc.ts';

  let { rpc }: { rpc: Rpc } = $props();

  let items = $state<PrereqView[] | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  async function check(): Promise<void> {
    busy = true;
    try {
      items = await rpc.request<PrereqView[]>('system.check');
      error = null;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  $effect(() => {
    void check();
  });
</script>

<div class="checklist">
  <div class="flex items-center">
    <b>Tools on the daemon machine</b>
    <Button variant="ghost" size="xs" class="ml-auto" disabled={busy} onclick={check}>
      {#if busy}<LoaderCircle class="spin" />{:else}<RefreshCw />{/if}Re-check
    </Button>
  </div>
  {#if error}
    <p class="field-error">{error}</p>
  {:else if items === null}
    <p class="muted">Checking…</p>
  {:else}
    <div class="steps">
      {#each items as p (p.name)}
        <div class="step" data-s={p.ok ? 'done' : 'error'}>
          {#if p.ok}<CircleCheck aria-hidden="true" />{:else}<CircleX aria-hidden="true" />{/if}
          <span>
            <span class="mono">{p.name}</span>
            <span class="err" style:color={p.ok ? 'var(--muted-foreground)' : undefined}>{p.detail || (p.ok ? 'OK' : 'Not found')}</span>
          </span>
        </div>
      {/each}
    </div>
  {/if}
</div>
