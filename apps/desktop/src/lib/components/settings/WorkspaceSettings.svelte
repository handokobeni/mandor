<script lang="ts">
  import Layers from '@lucide/svelte/icons/layers';
  import Plus from '@lucide/svelte/icons/plus';
  import Trash from '@lucide/svelte/icons/trash';
  import { toast } from 'svelte-sonner';
  import EmptyState from '#lib/components/mandor/EmptyState.svelte';
  import * as AlertDialog from '#lib/components/ui/alert-dialog/index.ts';
  import { Button, buttonVariants } from '#lib/components/ui/button/index.ts';
  import type { WorkspaceView } from '#lib/protocol/WorkspaceView.ts';
  import type { Rpc } from '#lib/rpc.ts';
  import { lark } from '#lib/stores/lark.svelte.ts';
  import AddWorkspaceDialog from './AddWorkspaceDialog.svelte';
  import CheckList from './CheckList.svelte';

  let { rpc }: { rpc: Rpc } = $props();

  let selectedId = $state<string | null>(null);
  const ws = $derived(lark.workspaces.find((w) => w.id === selectedId) ?? lark.workspaces[0]);

  // Salinan yang bisa diedit; diganti setiap workspace terpilih berubah.
  let edit = $state<{ repos: string[]; mcp: string[] } | null>(null);
  let editFor = $state<string | null>(null);
  let available = $state<string[] | null>(null);
  $effect(() => {
    if (ws && editFor !== ws.id) {
      const current = ws;
      editFor = current.id;
      edit = { repos: [...current.repo_ids], mcp: [...current.mcp_servers] };
      available = null;
      rpc
        .request<string[]>('workspaces.mcp_servers', { root: current.root })
        .then((names) => (available = names))
        .catch(() => (available = []));
    }
  });

  const dirty = $derived(
    !!ws &&
      !!edit &&
      (edit.repos.join() !== ws.repo_ids.join() || [...edit.mcp].sort().join() !== [...ws.mcp_servers].sort().join())
  );

  let adding = $state(false);
  let removing = $state<WorkspaceView | null>(null);

  function message(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
  }

  async function save(w: WorkspaceView): Promise<void> {
    if (!edit) return;
    // Urutan repo mengikuti daftar repo, agar repo utama run stabil.
    const order = lark.repos.map((r) => r.id);
    const repoIds = order.filter((id) => edit?.repos.includes(id));
    try {
      await rpc.request('workspaces.update', { id: w.id, repo_ids: repoIds, mcp_servers: edit.mcp });
      await lark.refresh();
      editFor = null;
      toast.success('Workspace saved', { description: `${w.name} · used on the next start` });
    } catch (err) {
      toast.error('Could not save the workspace', { description: message(err) });
    }
  }

  async function remove(w: WorkspaceView): Promise<void> {
    removing = null;
    try {
      await rpc.request('workspaces.remove', { id: w.id });
      await lark.refresh();
      selectedId = null;
      toast.success('Workspace removed', { description: w.name });
    } catch (err) {
      toast.error('Could not remove the workspace', { description: message(err) });
    }
  }
</script>

<section class="flex items-end gap-3" style="max-width:none">
  <div class="flex-1">
    <h1>Workspaces</h1>
    <p class="lede">Groups of repos under one parent folder. Start a task in several repos at once, one PR per repo.</p>
  </div>
  {#if lark.workspaces.length > 0}
    <Button variant="secondary" size="sm" onclick={() => (adding = true)}><Plus />Add workspace</Button>
  {/if}
</section>

{#if lark.workspaces.length === 0}
  <section>
    <div class="settings-card">
      <EmptyState icon={Layers} title="No workspaces yet">
        Optional. Use one when a task spans repos that live side by side, e.g. an API and a web app.
        {#snippet actions()}<Button onclick={() => (adding = true)}><Plus />Add a workspace</Button>{/snippet}
      </EmptyState>
    </div>
  </section>
{:else}
  <section>
    <div class="settings-card" style="overflow:hidden">
      <table class="table">
        <thead><tr><th>Workspace</th><th>Parent folder</th><th>Repos</th><th>MCP servers</th></tr></thead>
        <tbody>
          {#each lark.workspaces as w (w.id)}
            <tr
              aria-selected={ws?.id === w.id}
              tabindex="0"
              onclick={() => (selectedId = w.id)}
              onkeydown={(e) => e.key === 'Enter' && (selectedId = w.id)}
            >
              <td><b>{w.name}</b></td>
              <td class="mono">{w.root}</td>
              <td>{w.repo_ids.join(', ')}</td>
              <td>{w.mcp_servers.length > 0 ? w.mcp_servers.join(', ') : 'none'}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </section>

  {#if ws && edit}
    <section>
      <div class="settings-card">
        <header>
          <h2>{ws.name}</h2>
          <p class="mono">{ws.root}</p>
        </header>
        <div class="content">
          <div class="field">
            <span class="text-sm font-medium">Repos</span>
            <p class="field-help">Repos you can check when starting a task in this workspace.</p>
            <CheckList
              items={lark.repos.map((r) => ({ id: r.id, label: r.id, hint: r.path }))}
              bind:selected={edit.repos}
              label="Workspace repos"
              empty="Add the repos in Settings › Repos first."
            />
          </div>
          <div class="field">
            <span class="text-sm font-medium">MCP servers from .mcp.json</span>
            <p class="field-help">
              Only checked servers are loaded for the agent, and the hook denies tools from any other server. A server can
              act outside Mandor's guardrails, so check only what the agent needs.
            </p>
            {#if available === null}
              <p class="muted pick-note">Reading .mcp.json…</p>
            {:else}
              <CheckList
                items={available.map((s) => ({ id: s, label: s }))}
                bind:selected={edit.mcp}
                label="MCP servers"
                empty="No .mcp.json in the parent folder."
              />
            {/if}
          </div>
        </div>
        <footer style="justify-content:space-between">
          <Button variant="ghost" class="text-status-failed" onclick={() => (removing = ws)}><Trash />Remove workspace</Button>
          <Button disabled={!dirty || edit.repos.length === 0} onclick={() => save(ws)}>Save changes</Button>
        </footer>
      </div>
    </section>
  {/if}
{/if}

{#if adding}
  <AddWorkspaceDialog
    {rpc}
    repos={lark.repos}
    oncancel={() => (adding = false)}
    onadded={async (w) => {
      adding = false;
      await lark.refresh();
      selectedId = w.id;
      toast.success('Workspace added', { description: `${w.name} · ${w.repo_ids.length} repos` });
    }}
  />
{/if}

{#if removing}
  {@const w = removing}
  <AlertDialog.Root open onOpenChange={(o) => !o && (removing = null)}>
    <AlertDialog.Content>
      <AlertDialog.Header>
        <AlertDialog.Title>Remove {w.name}?</AlertDialog.Title>
        <AlertDialog.Description>
          Mandor forgets this workspace. The repos and the folder at {w.root} are kept. Runs already working in it keep
          running.
        </AlertDialog.Description>
      </AlertDialog.Header>
      <AlertDialog.Footer>
        <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
        <AlertDialog.Action class={buttonVariants({ variant: 'destructive' })} onclick={() => remove(w)}>Remove workspace</AlertDialog.Action>
      </AlertDialog.Footer>
    </AlertDialog.Content>
  </AlertDialog.Root>
{/if}
