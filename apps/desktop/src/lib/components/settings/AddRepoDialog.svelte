<script lang="ts">
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import FolderGit2 from '@lucide/svelte/icons/folder-git-2';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { Badge } from '#lib/components/ui/badge/index.ts';
  import { Button } from '#lib/components/ui/button/index.ts';
  import * as Dialog from '#lib/components/ui/dialog/index.ts';
  import { Input } from '#lib/components/ui/input/index.ts';
  import type { RepoCandidateView } from '#lib/protocol/RepoCandidateView.ts';
  import type { RepoView } from '#lib/protocol/RepoView.ts';
  import type { Rpc } from '#lib/rpc.ts';
  import Field from './Field.svelte';
  import FolderBrowser from './FolderBrowser.svelte';

  interface Props {
    rpc: Rpc;
    onadded: (repo: RepoView) => void;
    oncancel: () => void;
  }
  let { rpc, onadded, oncancel }: Props = $props();

  let path = $state('');
  let gh = $state('');
  let error = $state<string | null>(null);
  let busy = $state(false);

  let view = $state<'main' | 'browse'>('main');
  let candidates = $state<RepoCandidateView[] | null>(null);
  let browseFrom = $state<string | null>(null);

  function message(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
  }

  // Saran dimuat sekali saat dialog dibuka.
  $effect(() => {
    rpc
      .request<RepoCandidateView[]>('repos.discover')
      .then((list) => (candidates = list))
      .catch(() => (candidates = []));
  });

  function browse(to: string | null): void {
    browseFrom = to;
    view = 'browse';
  }

  // Dibuka di folder induk path terisi, agar repo di sebelahnya ikut terlihat.
  function browseStart(p: string): string | null {
    const t = p.trim().replace(/\/+$/, '');
    if (!t.startsWith('/')) return null;
    return t.slice(0, t.lastIndexOf('/')) || '/';
  }

  function pick(p: string): void {
    path = p;
    error = null;
    view = 'main';
  }

  async function add(): Promise<void> {
    const p = path.trim();
    if (!p.startsWith('/')) {
      error = 'Use an absolute path on the daemon machine, e.g. /home/you/work/project.';
      return;
    }
    busy = true;
    error = null;
    try {
      onadded(await rpc.request<RepoView>('repos.add', { path: p, gh_profile: gh.trim() || null }));
    } catch (err) {
      error = message(err);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog.Root open onOpenChange={(o) => !o && !busy && oncancel()}>
  <Dialog.Content class="sm:max-w-[560px] max-h-[90vh] overflow-y-auto">
    {#if view === 'main'}
      <Dialog.Header>
        <Dialog.Title>Add a repo</Dialog.Title>
        <Dialog.Description>A git clone on the daemon machine with an origin remote.</Dialog.Description>
      </Dialog.Header>

      <div class="grid gap-1.5">
        <span class="text-sm font-medium">Found on this machine</span>
        <div class="repo-pick scroll-area">
          {#if candidates === null}
            <p class="muted pick-note"><LoaderCircle class="spin icon-sm" aria-hidden="true" /> Looking for git repos…</p>
          {:else if candidates.length === 0}
            <p class="muted pick-note">No git repos with an origin remote in your home folder. Use Browse or type the path.</p>
          {:else}
            {#each candidates as c (c.path)}
              <button
                type="button"
                class="pick-row"
                aria-pressed={path === c.path}
                disabled={c.added}
                title={c.origin ?? c.path}
                onclick={() => pick(c.path)}
              >
                <FolderGit2 class="icon-sm" aria-hidden="true" />
                <span class="pick-txt"><b>{c.name}</b><span class="mono">{c.path}</span></span>
                {#if c.added}<Badge variant="secondary">Added</Badge>{/if}
              </button>
            {/each}
          {/if}
        </div>
      </div>

      <Field id="ar-path" label="Path" error={error ?? undefined}>
        <div class="flex gap-2">
          <Input
            id="ar-path"
            class="font-mono"
            placeholder="/home/you/work/project"
            bind:value={path}
            aria-invalid={error ? 'true' : undefined}
          />
          <Button variant="outline" type="button" onclick={() => browse(browseStart(path))}>
            <FolderOpen />Browse…
          </Button>
        </div>
        {#snippet help()}Pick a repo above, browse, or type an absolute path.{/snippet}
      </Field>
      <Field id="ar-gh" label="gh profile (optional)">
        <Input id="ar-gh" placeholder="default" bind:value={gh} />
      </Field>
      <Dialog.Footer>
        <Button variant="ghost" disabled={busy} onclick={oncancel}>Cancel</Button>
        <Button disabled={busy || !path.trim()} onclick={add}>
          {#if busy}<LoaderCircle class="spin" />Checking{:else}<CircleCheck />Add repo{/if}
        </Button>
      </Dialog.Footer>
    {:else}
      <FolderBrowser
        {rpc}
        start={browseFrom}
        pickRepos
        description="Folders on the daemon machine. Git repos are marked."
        onpick={pick}
        onback={() => (view = 'main')}
      />
    {/if}
  </Dialog.Content>
</Dialog.Root>
