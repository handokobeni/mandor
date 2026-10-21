<script lang="ts">
  import FolderGit2 from '@lucide/svelte/icons/folder-git-2';
  import Plus from '@lucide/svelte/icons/plus';
  import Trash from '@lucide/svelte/icons/trash';
  import { toast } from 'svelte-sonner';
  import EmptyState from '#lib/components/mandor/EmptyState.svelte';
  import * as AlertDialog from '#lib/components/ui/alert-dialog/index.ts';
  import { Button, buttonVariants } from '#lib/components/ui/button/index.ts';
  import { Input } from '#lib/components/ui/input/index.ts';
  import type { RepoView } from '#lib/protocol/RepoView.ts';
  import type { Rpc } from '#lib/rpc.ts';
  import { lark } from '#lib/stores/lark.svelte.ts';
  import AddRepoDialog from './AddRepoDialog.svelte';
  import Field from './Field.svelte';
  import ListEditor from './ListEditor.svelte';

  let { rpc }: { rpc: Rpc } = $props();

  let selectedId = $state<string | null>(null);
  const repo = $derived(lark.repos.find((r) => r.id === selectedId) ?? lark.repos[0]);

  // Salinan yang bisa diedit; diganti setiap repo terpilih berubah.
  let edit = $state<{ gh: string; companions: string[]; reviewers: string[]; allowed: string[] } | null>(null);
  let editFor = $state<string | null>(null);
  $effect(() => {
    if (repo && editFor !== repo.id) {
      editFor = repo.id;
      edit = {
        gh: repo.gh_profile ?? '',
        companions: [...repo.companions],
        reviewers: [...repo.trusted_reviewers],
        allowed: [...repo.extra_allowed]
      };
    }
  });

  let adding = $state(false);
  let removing = $state<RepoView | null>(null);

  function message(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
  }

  async function save(r: RepoView): Promise<void> {
    if (!edit) return;
    try {
      await rpc.request('repos.update', {
        id: r.id,
        gh_profile: edit.gh.trim() || null,
        companions: edit.companions,
        trusted_reviewers: edit.reviewers,
        extra_allowed: edit.allowed
      });
      await lark.refresh();
      toast.success('Repo saved', { description: `${r.id} · used on the next agent start` });
    } catch (err) {
      toast.error('Could not save the repo', { description: message(err) });
    }
  }

  async function remove(r: RepoView): Promise<void> {
    removing = null;
    try {
      await rpc.request('repos.remove', { id: r.id });
      await lark.refresh();
      selectedId = null;
      toast.success('Repo removed', { description: r.id });
    } catch (err) {
      toast.error('Could not remove the repo', { description: message(err) });
    }
  }

  function openAdd(): void {
    adding = true;
  }
</script>

<section class="flex items-end gap-3" style="max-width:none">
  <div class="flex-1">
    <h1>Repos</h1>
    <p class="lede">Repos on the daemon machine that agents can work in.</p>
  </div>
  {#if lark.repos.length > 0}
    <Button variant="secondary" size="sm" onclick={openAdd}><Plus />Add repo</Button>
  {/if}
</section>

{#if lark.repos.length === 0}
  <section>
    <div class="settings-card">
      <EmptyState icon={FolderGit2} title="No repos yet">
        Add a git repo on the daemon machine so agents have somewhere to work.
        {#snippet actions()}<Button onclick={openAdd}><Plus />Add a repo</Button>{/snippet}
      </EmptyState>
    </div>
  </section>
{:else}
  <section>
    <div class="settings-card" style="overflow:hidden">
      <table class="table">
        <thead><tr><th>Repo</th><th>Path</th><th>Default branch</th><th>gh profile</th></tr></thead>
        <tbody>
          {#each lark.repos as r (r.id)}
            <tr
              aria-selected={repo?.id === r.id}
              tabindex="0"
              onclick={() => (selectedId = r.id)}
              onkeydown={(e) => e.key === 'Enter' && (selectedId = r.id)}
            >
              <td><b>{r.id}</b></td>
              <td class="mono">{r.path}</td>
              <td class="mono">{r.default_branch ?? '—'}</td>
              <td>{r.gh_profile ?? 'default'}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </section>

  {#if repo && edit}
    <section>
      <div class="settings-card">
        <header>
          <h2>{repo.id}</h2>
          <p class="mono">{repo.path} · origin/{repo.default_branch ?? '?'}</p>
        </header>
        <div class="content">
          <div style="max-width:280px">
            <Field id="r-gh" label="gh profile">
              <Input id="r-gh" placeholder="default" bind:value={edit.gh} />
              {#snippet help()}The GitHub CLI account that pushes and opens PRs for this repo.{/snippet}
            </Field>
          </div>
          <div class="field">
            <span class="text-sm font-medium">Read-only companion paths</span>
            <p class="field-help">Extra folders the agent may read but never write, e.g. a shared design-token repo.</p>
            <ListEditor list="companions" label="companion paths" placeholder="/home/you/work/shared-config" bind:items={edit.companions} />
          </div>
          <div class="field">
            <span class="text-sm font-medium">Trusted reviewers and QA</span>
            <p class="field-help">
              Lark open_ids whose record comments are forwarded to the agent automatically. Assignees and the reporter are
              always trusted.
            </p>
            <ListEditor list="reviewers" label="trusted reviewers" placeholder="ou_51aa6c" bind:items={edit.reviewers} />
          </div>
          <div class="field">
            <span class="text-sm font-medium">Extra agent allowlist</span>
            <p class="field-help">
              Tool patterns the agent can run without asking. Anything not listed is denied, and
              <span class="mono">git push</span> is always run by the daemon.
            </p>
            <ListEditor list="allowed" label="agent allowlist" placeholder="Bash(just *)" bind:items={edit.allowed} />
          </div>
        </div>
        <footer style="justify-content:space-between">
          <Button variant="ghost" class="text-status-failed" onclick={() => (removing = repo)}><Trash />Remove repo</Button>
          <Button onclick={() => save(repo)}>Save changes</Button>
        </footer>
      </div>
    </section>
  {/if}
{/if}

{#if adding}
  <AddRepoDialog
    {rpc}
    oncancel={() => (adding = false)}
    onadded={async (r) => {
      adding = false;
      await lark.refresh();
      selectedId = r.id;
      toast.success('Repo added', { description: `${r.id} · default branch ${r.default_branch ?? 'unknown'}` });
    }}
  />
{/if}

{#if removing}
  {@const r = removing}
  <AlertDialog.Root open onOpenChange={(o) => !o && (removing = null)}>
    <AlertDialog.Content>
      <AlertDialog.Header>
        <AlertDialog.Title>Remove {r.id}?</AlertDialog.Title>
        <AlertDialog.Description>
          Mandor forgets this repo and its settings. The folder at {r.path} is not deleted. Runs already working in it keep
          running.
        </AlertDialog.Description>
      </AlertDialog.Header>
      <AlertDialog.Footer>
        <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
        <AlertDialog.Action class={buttonVariants({ variant: 'destructive' })} onclick={() => remove(r)}>Remove repo</AlertDialog.Action>
      </AlertDialog.Footer>
    </AlertDialog.Content>
  </AlertDialog.Root>
{/if}
