<script lang="ts">
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import Folder from '@lucide/svelte/icons/folder';
  import FolderGit2 from '@lucide/svelte/icons/folder-git-2';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { Badge } from '#lib/components/ui/badge/index.ts';
  import { Button } from '#lib/components/ui/button/index.ts';
  import * as Dialog from '#lib/components/ui/dialog/index.ts';
  import type { DirListingView } from '#lib/protocol/DirListingView.ts';
  import type { Rpc } from '#lib/rpc.ts';
  import { pathCrumbs } from '#lib/settings.ts';

  interface Props {
    rpc: Rpc;
    /** Folder awal; `null` = home pengguna di mesin daemon. */
    start: string | null;
    /** Klik git repo langsung memilihnya (Add repo); selain itu membuka foldernya. */
    pickRepos: boolean;
    description: string;
    onpick: (path: string) => void;
    onback: () => void;
  }
  let { rpc, start, pickRepos, description, onpick, onback }: Props = $props();

  let listing = $state<DirListingView | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(false);

  async function browse(to: string | null): Promise<void> {
    loading = true;
    error = null;
    try {
      listing = await rpc.request<DirListingView>('fs.list_dirs', { path: to });
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void browse(start);
  });
</script>

<Dialog.Header>
  <Dialog.Title>Choose a folder</Dialog.Title>
  <Dialog.Description>{description}</Dialog.Description>
</Dialog.Header>
{#if listing}
  {@const current = listing}
  <nav class="crumbs" aria-label="Folder path">
    {#each pathCrumbs(current.path) as c, i (c.path)}
      {#if i > 1}<span class="muted">/</span>{/if}
      <button type="button" class="crumb" onclick={() => browse(c.path)}>{c.name}</button>
    {/each}
  </nav>
  <div class="repo-pick scroll-area">
    {#if current.parent}
      {@const parent = current.parent}
      <button type="button" class="pick-row" onclick={() => browse(parent)}>
        <ArrowUp class="icon-sm" aria-hidden="true" /><span class="pick-txt"><b>..</b></span>
      </button>
    {/if}
    {#each current.entries as e (e.path)}
      <button
        type="button"
        class="pick-row"
        onclick={() => (e.is_repo && pickRepos ? onpick(e.path) : browse(e.path))}
        title={e.is_repo && pickRepos ? 'Use this repo' : 'Open folder'}
      >
        {#if e.is_repo}<FolderGit2 class="icon-sm" aria-hidden="true" />{:else}<Folder class="icon-sm" aria-hidden="true" />{/if}
        <span class="pick-txt"><b>{e.name}</b></span>
        {#if e.is_repo}<Badge variant="outline">git repo</Badge>{/if}
      </button>
    {:else}
      <p class="muted pick-note">No subfolders here.</p>
    {/each}
  </div>
{:else if loading}
  <p class="muted pick-note"><LoaderCircle class="spin icon-sm" aria-hidden="true" /> Reading folders…</p>
{/if}
{#if error}<p class="field-error">{error}</p>{/if}
<Dialog.Footer>
  <Button variant="ghost" onclick={onback}><ArrowLeft />Back</Button>
  {#if listing}
    {@const here = listing.path}
    <Button onclick={() => onpick(here)}>Use this folder</Button>
  {/if}
</Dialog.Footer>
