<script lang="ts">
  // Pick from .env: nama kunci dari .env folder asli; nilai tidak pernah tampil (R1–R5).
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { onMount } from 'svelte';
  import { Button } from '#lib/components/ui/button/index.ts';
  import { Checkbox } from '#lib/components/ui/checkbox/index.ts';
  import * as Dialog from '#lib/components/ui/dialog/index.ts';
  import { applyPicks, type EnvKeyRow, pickerRows } from '#lib/envkeys.ts';
  import type { EnvKeysView } from '#lib/protocol/EnvKeysView.ts';
  import type { Rpc } from '#lib/rpc.ts';

  interface Props {
    rpc: Rpc;
    /** Tepat satu dari `repo_id` atau `design_source_id`. */
    target: { repo_id: string } | { design_source_id: string };
    saved: string[];
    oncancel: () => void;
    onpick: (keys: string[]) => void;
  }
  let { rpc, target, saved, oncancel, onpick }: Props = $props();

  let rows = $state<EnvKeyRow[]>([]);
  let found = $state(true);
  let loading = $state(true);
  let error = $state<string | null>(null);

  onMount(async () => {
    try {
      const view = await rpc.request<EnvKeysView>('app.env_keys', target);
      found = view.found;
      rows = pickerRows(view.keys, saved);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      rows = pickerRows([], saved);
    } finally {
      loading = false;
    }
  });

  const checkedCount = $derived(rows.filter((r) => r.checked).length);
</script>

<Dialog.Root open onOpenChange={(o) => !o && oncancel()}>
  <Dialog.Content class="sm:max-w-[480px] max-h-[90vh] overflow-y-auto">
    <Dialog.Header>
      <Dialog.Title class="text-[length:var(--text-md)]">Pick from .env</Dialog.Title>
      <Dialog.Description>
        Key names from the .env file in the original folder. Values are never shown, stored, or copied.
      </Dialog.Description>
    </Dialog.Header>
    {#if loading}
      <p class="text-sm text-muted-foreground"><LoaderCircle class="icon-sm animate-spin" /> Reading .env…</p>
    {:else}
      {#if error}
        <p class="field-error">{error}. You can still type key names by hand.</p>
      {:else if !found}
        <p class="field-help">No .env file in this folder. You can still type key names by hand.</p>
      {/if}
      {#if rows.length > 0}
        <div class="env-rows" role="group" aria-label=".env keys">
          {#each rows as row (row.name)}
            <label class="env-row">
              <Checkbox bind:checked={row.checked} aria-label={row.name} />
              <span class="font-mono text-xs">{row.name}</span>
              {#if row.missing}<span class="missing">not in .env</span>{/if}
            </label>
          {/each}
        </div>
      {/if}
    {/if}
    <Dialog.Footer>
      <Button variant="ghost" onclick={oncancel}>Cancel</Button>
      <Button disabled={loading} onclick={() => onpick(applyPicks(rows))}>
        Use {checkedCount} {checkedCount === 1 ? 'key' : 'keys'}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<style>
  .env-rows {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 50vh;
    overflow-y: auto;
  }
  .env-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 2px;
    min-width: 0;
  }
  .env-row span:not(.missing) {
    overflow-wrap: anywhere;
  }
  .missing {
    margin-left: auto;
    flex-shrink: 0;
    font-size: var(--text-xs);
    color: var(--warning, var(--muted-foreground));
  }
</style>
