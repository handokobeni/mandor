<script lang="ts">
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { Button } from '#lib/components/ui/button/index.ts';
  import * as Dialog from '#lib/components/ui/dialog/index.ts';
  import { Input } from '#lib/components/ui/input/index.ts';
  import type { RepoView } from '#lib/protocol/RepoView.ts';
  import type { WorkspaceView } from '#lib/protocol/WorkspaceView.ts';
  import type { Rpc } from '#lib/rpc.ts';
  import { reposUnder, validateWorkspace, workspaceName, type WorkspaceErrors } from '#lib/workspaces.ts';
  import CheckList from './CheckList.svelte';
  import Field from './Field.svelte';
  import FolderBrowser from './FolderBrowser.svelte';

  interface Props {
    rpc: Rpc;
    repos: RepoView[];
    onadded: (ws: WorkspaceView) => void;
    oncancel: () => void;
  }
  let { rpc, repos, onadded, oncancel }: Props = $props();

  let name = $state('');
  let root = $state('');
  let repoIds = $state<string[]>([]);
  let mcpServers = $state<string[]>([]);
  let available = $state<string[] | null>(null);
  let errors = $state<WorkspaceErrors>({});
  let error = $state<string | null>(null);
  let busy = $state(false);
  let view = $state<'main' | 'browse'>('main');

  const repoItems = $derived(repos.map((r) => ({ id: r.id, label: r.id, hint: r.path })));

  /** Folder induk dipilih: nama, repo di dalamnya, dan MCP server dari `.mcp.json`. */
  async function useRoot(path: string): Promise<void> {
    root = path;
    view = 'main';
    if (!name.trim()) name = workspaceName(path);
    repoIds = reposUnder(path, repos);
    mcpServers = [];
    available = null;
    try {
      available = await rpc.request<string[]>('workspaces.mcp_servers', { root: path });
    } catch (err) {
      available = [];
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function add(): Promise<void> {
    errors = validateWorkspace({ name, root, repoIds });
    if (Object.keys(errors).length > 0) return;
    busy = true;
    error = null;
    try {
      onadded(
        await rpc.request<WorkspaceView>('workspaces.add', {
          name: name.trim(),
          root: root.trim(),
          repo_ids: repoIds,
          mcp_servers: mcpServers
        })
      );
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog.Root open onOpenChange={(o) => !o && !busy && oncancel()}>
  <Dialog.Content class="sm:max-w-[560px] max-h-[90vh] overflow-y-auto">
    {#if view === 'main'}
      <Dialog.Header>
        <Dialog.Title>Add a workspace</Dialog.Title>
        <Dialog.Description>
          A parent folder that holds several repos, e.g. an API, a web app, and a mobile app. The agent works across the
          repos you check when you start a task.
        </Dialog.Description>
      </Dialog.Header>
      <Field id="aw-root" label="Parent folder" error={errors.root}>
        <div class="flex gap-2">
          <Input id="aw-root" class="font-mono" placeholder="/home/you/work/product" bind:value={root} onchange={() => useRoot(root)} />
          <Button variant="outline" type="button" onclick={() => (view = 'browse')}><FolderOpen />Browse…</Button>
        </div>
        {#snippet help()}Its CLAUDE.md is copied for the agent. Its .claude/settings.local.json is not used.{/snippet}
      </Field>
      <Field id="aw-name" label="Name" error={errors.name}>
        <Input id="aw-name" placeholder="product" bind:value={name} />
      </Field>
      <div class="grid gap-1.5">
        <span class="text-sm font-medium">Repos</span>
        <CheckList items={repoItems} bind:selected={repoIds} label="Workspace repos" empty="Add the repos in Settings › Repos first." />
        {#if errors.repos}<p class="field-error">{errors.repos}</p>{/if}
      </div>
      <div class="grid gap-1.5">
        <span class="text-sm font-medium">MCP servers from .mcp.json</span>
        <p class="field-help">
          Off by default. A checked server can act outside Mandor's guardrails (e.g. write to GitHub or Lark), so only
          check the ones the agent needs.
        </p>
        {#if available === null && root.trim()}
          <p class="muted pick-note"><LoaderCircle class="spin icon-sm" aria-hidden="true" /> Reading .mcp.json…</p>
        {:else}
          <CheckList
            items={(available ?? []).map((s) => ({ id: s, label: s }))}
            bind:selected={mcpServers}
            label="MCP servers"
            empty={root.trim() ? 'No .mcp.json in this folder.' : 'Choose the parent folder first.'}
          />
        {/if}
      </div>
      {#if error}<p class="field-error">{error}</p>{/if}
      <Dialog.Footer>
        <Button variant="ghost" disabled={busy} onclick={oncancel}>Cancel</Button>
        <Button disabled={busy} onclick={add}>
          {#if busy}<LoaderCircle class="spin" />Saving{:else}<CircleCheck />Add workspace{/if}
        </Button>
      </Dialog.Footer>
    {:else}
      <FolderBrowser
        {rpc}
        start={root.trim().startsWith('/') ? root.trim() : null}
        pickRepos={false}
        description="Choose the folder that contains the repos."
        onpick={useRoot}
        onback={() => (view = 'main')}
      />
    {/if}
  </Dialog.Content>
</Dialog.Root>
