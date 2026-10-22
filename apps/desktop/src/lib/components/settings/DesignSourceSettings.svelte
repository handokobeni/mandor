<script lang="ts">
  // Settings › Design sources (R3, jenis 2.0): daftar global yang dipilih saat Start.
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import FolderCode from '@lucide/svelte/icons/folder-code';
  import Frame from '@lucide/svelte/icons/frame';
  import Plus from '@lucide/svelte/icons/plus';
  import Trash from '@lucide/svelte/icons/trash';
  import { toast } from 'svelte-sonner';
  import EmptyState from '#lib/components/mandor/EmptyState.svelte';
  import { Button } from '#lib/components/ui/button/index.ts';
  import type { DesignSourceView } from '#lib/protocol/DesignSourceView.ts';
  import type { Rpc } from '#lib/rpc.ts';
  import DesignSourceDialog from './DesignSourceDialog.svelte';

  let { rpc }: { rpc: Rpc } = $props();

  let sources = $state<DesignSourceView[] | null>(null);
  let editing = $state<DesignSourceView | 'new' | null>(null);

  async function load(): Promise<void> {
    try {
      sources = await rpc.request<DesignSourceView[]>('design_sources.list');
    } catch (err) {
      sources = [];
      toast.error('Could not load design sources', { description: err instanceof Error ? err.message : String(err) });
    }
  }
  $effect(() => {
    void load();
  });

  async function remove(s: DesignSourceView): Promise<void> {
    try {
      await rpc.request('design_sources.remove', { id: s.id });
      await load();
      toast.success('Design source removed', { description: s.name });
    } catch (err) {
      toast.error('Could not remove it', { description: err instanceof Error ? err.message : String(err) });
    }
  }
</script>

<section class="flex items-end gap-3" style="max-width:none">
  <div class="flex-1">
    <h1>Design sources</h1>
    <p class="lede">
      Global list of designs the agent checks the AC against during planning and review. Pick them per task when you
      start it. Figma files, prototype URLs, and image folders come in a later release.
    </p>
  </div>
  {#if sources && sources.length > 0}
    <Button variant="secondary" size="sm" onclick={() => (editing = 'new')}><Plus />Add source</Button>
  {/if}
</section>

<section>
  <div class="settings-card">
    {#if sources === null}
      <div class="content"><p class="muted">Loading…</p></div>
    {:else if sources.length === 0}
      <EmptyState icon={Frame} title="No design sources yet">
        Add a code repo or folder with the design, such as a prototype, so planning can mark which AC the design covers.
        {#snippet actions()}<Button onclick={() => (editing = 'new')}><Plus />Add a source</Button>{/snippet}
      </EmptyState>
    {:else}
      <div class="content grid gap-2">
        {#each sources as s (s.id)}
          <div class="source-item">
            <span class="si-ico"><FolderCode aria-hidden="true" /></span>
            <div class="si-txt">
              <div class="si-name">{s.name}</div>
              <div class="si-loc mono">{s.path}</div>
              <div class="muted si-kind">Code repo or folder</div>
            </div>
            <span class="si-status" data-tone={s.ready ? 'ok' : 'failed'}>
              {#if s.ready}<CircleCheck class="icon-sm" aria-hidden="true" />{:else}<CircleX class="icon-sm" aria-hidden="true" />{/if}{s.status}
            </span>
            <span class="si-act">
              <Button variant="ghost" size="xs" onclick={() => (editing = s)}>Edit</Button>
              <Button variant="ghost" size="icon-xs" aria-label="Remove {s.name}" onclick={() => remove(s)}><Trash /></Button>
            </span>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</section>

{#if editing}
  <DesignSourceDialog
    {rpc}
    source={editing === 'new' ? null : editing}
    oncancel={() => (editing = null)}
    onsaved={async (s) => {
      editing = null;
      await load();
      toast.success('Design source saved', { description: s.name });
    }}
  />
{/if}
