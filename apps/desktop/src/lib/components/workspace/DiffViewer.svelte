<script lang="ts">
  import FileCode from '@lucide/svelte/icons/file-code';
  import GitCompare from '@lucide/svelte/icons/git-compare';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import Undo2 from '@lucide/svelte/icons/undo-2';
  import * as AlertDialog from '#lib/components/ui/alert-dialog/index.ts';
  import { Button, buttonVariants } from '#lib/components/ui/button/index.ts';
  import { parsePatch } from '#lib/diff.ts';
  import EmptyState from '#lib/components/mandor/EmptyState.svelte';
  import type { FileChangeView } from '#lib/protocol/FileChangeView.ts';
  import { groupChanges } from '#lib/workspaces.ts';

  interface Props {
    files: FileChangeView[] | null;
    /** Repo run workspace (path berawalan `<repo>/`); `null` untuk run satu repo. */
    repoIds?: string[] | null;
    branch: string | null;
    patch: string | null;
    selected: string | null;
    onselect: (path: string) => void;
    /** `null` bila discard tidak tersedia (mis. agent sedang bekerja). */
    ondiscard: ((path: string) => void) | null;
  }
  let { files, repoIds = null, branch, patch, selected, onselect, ondiscard }: Props = $props();

  let confirming = $state<string | null>(null);

  const STATUS: Record<string, string> = { A: 'added', M: 'modified', D: 'deleted', R: 'renamed', U: 'untracked' };
  const rows = $derived(parsePatch(patch ?? ''));
  const adds = $derived((files ?? []).reduce((a, f) => a + (f.additions ?? 0), 0));
  const dels = $derived((files ?? []).reduce((a, f) => a + (f.deletions ?? 0), 0));
  const code = (s: string) => s.charAt(0).toUpperCase();
  const groups = $derived(groupChanges(files ?? [], repoIds));
  const shown = (path: string, repo: string | null) => (repo ? path.slice(repo.length + 1) : path);
</script>

{#if files === null}
  <div class="changes" style="display:flex">
    <EmptyState icon={LoaderCircle} title="Loading changes…" />
  </div>
{:else if files.length === 0}
  <div class="changes" style="display:flex">
    <EmptyState icon={GitCompare} title="No changes against the base branch">
      The agent has not changed any file{branch ? ` on ${branch}` : ''} yet.
    </EmptyState>
  </div>
{:else}
  <div class="changes">
    <div class="filelist scroll-area" role="listbox" aria-label="Changed files">
      <div class="caps muted" style="padding:6px 8px 8px">
        {files.length} files · <span class="add-c">+{adds}</span> <span class="del-c">−{dels}</span>
      </div>
      {#each groups as g (g.repo ?? '')}
      {#if g.repo}<div class="caps muted repo-group">{g.repo}</div>{/if}
      {#each g.files as f (f.path)}
        <button
          type="button"
          class="file-row"
          role="option"
          aria-selected={selected === f.path}
          title={f.old_path ? `${f.old_path} → ${f.path}` : f.path}
          onclick={() => onselect(f.path)}
        >
          <span class="fs" data-s={code(f.status)} aria-label={STATUS[code(f.status)] ?? f.status}>{code(f.status)}</span>
          <span class="fp">{shown(f.path, g.repo)}</span>
          <span class="fc">
            <span class="add-c">+{f.additions ?? '?'}</span> <span class="del-c">−{f.deletions ?? '?'}</span>
          </span>
        </button>
      {/each}
      {/each}
    </div>
    <div class="diff scroll-area">
      {#if selected}
        {@const path = selected}
        <div class="diff-file-head">
          <FileCode class="icon-sm" aria-hidden="true" /><span>{path}</span>
          {#if ondiscard}
            <Button variant="ghost" size="xs" class="ml-auto" onclick={() => (confirming = path)}>
              <Undo2 />Discard changes
            </Button>
          {/if}
        </div>
        {#if patch === null}
          <p class="muted" style="padding:14px">Loading the diff…</p>
        {:else if rows.length === 0}
          <p class="muted" style="padding:14px">No text changes to show (binary file or mode change).</p>
        {:else}
          <table>
            <tbody>
              {#each rows as r, i (i)}
                {#if r.kind === 'hunk'}
                  <tr class="hunk"><td class="ln"></td><td class="ln"></td><td class="code">{r.text}</td></tr>
                {:else}
                  <tr class={r.kind}>
                    <td class="ln">{r.old ?? ''}</td>
                    <td class="ln">{r.new ?? ''}</td>
                    <td class="code"><span class="sign">{r.kind === 'add' ? '+' : r.kind === 'del' ? '−' : ' '}</span>{r.text}</td>
                  </tr>
                {/if}
              {/each}
            </tbody>
          </table>
        {/if}
      {:else}
        <p class="muted" style="padding:14px">Choose a file to see its diff.</p>
      {/if}
    </div>
  </div>
{/if}

{#if confirming && ondiscard}
  {@const path = confirming}
  {@const discard = ondiscard}
  <AlertDialog.Root open onOpenChange={(o) => !o && (confirming = null)}>
    <AlertDialog.Content>
      <AlertDialog.Header>
        <AlertDialog.Title>Discard changes to {path}?</AlertDialog.Title>
        <AlertDialog.Description>
          The file goes back to its content on the base branch. This can't be undone from Mandor.
        </AlertDialog.Description>
      </AlertDialog.Header>
      <AlertDialog.Footer>
        <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
        <AlertDialog.Action
          class={buttonVariants({ variant: 'destructive' })}
          onclick={() => {
            confirming = null;
            discard(path);
          }}>Discard changes</AlertDialog.Action
        >
      </AlertDialog.Footer>
    </AlertDialog.Content>
  </AlertDialog.Root>
{/if}
