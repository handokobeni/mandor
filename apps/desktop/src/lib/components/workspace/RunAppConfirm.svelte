<script lang="ts">
  // Konfirmasi sebelum app dijalankan (R25a): berkas berubah per repo dengan
  // penanda manifest/skrip, kunci .env yang diteruskan (tanpa nilai), dan run
  // lain yang app-nya akan dihentikan.
  import FileCode from '@lucide/svelte/icons/file-code';
  import Play from '@lucide/svelte/icons/play';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import { confirmModel } from '#lib/app-run.ts';
  import * as Alert from '#lib/components/ui/alert/index.ts';
  import { Button } from '#lib/components/ui/button/index.ts';
  import * as Dialog from '#lib/components/ui/dialog/index.ts';
  import type { AppChangesView } from '#lib/protocol/AppChangesView.ts';

  interface Props {
    taskKey: string;
    changes: AppChangesView[];
    /** Kunci .env per repo (dari Settings). */
    envKeys: Record<string, string[]>;
    /** Run lain yang app-nya menyala (akan dihentikan). */
    otherRun: string | null;
    oncancel: () => void;
    onconfirm: () => void;
    onfile: (path: string) => void;
  }
  let { taskKey, changes, envKeys, otherRun, oncancel, onconfirm, onfile }: Props = $props();

  const repos = $derived(confirmModel(changes));
</script>

<Dialog.Root open onOpenChange={(o) => !o && oncancel()}>
  <Dialog.Content class="sm:max-w-[620px]">
    <Dialog.Header>
      <span class="mono muted">{taskKey}</span>
      <Dialog.Title class="text-[length:var(--text-md)]">Run the app with the agent's changes?</Dialog.Title>
      <Dialog.Description>
        The app runs code written by the agent on your machine. Check the changed files first, especially the flagged
        ones. Use local or dev credentials only in .env.
      </Dialog.Description>
    </Dialog.Header>
    {#if otherRun}
      <Alert.Root>
        <TriangleAlert class="text-status-warning" />
        <Alert.Title>{otherRun}'s app is running</Alert.Title>
        <Alert.Description>Only one run's app runs at a time. Starting this one stops {otherRun}'s app.</Alert.Description>
      </Alert.Root>
    {/if}
    <div class="grid gap-3" style="max-height:360px;overflow:auto">
      {#each repos as r (r.repo_id)}
        <div class="grid gap-1.5">
          <span class="text-sm font-medium">{r.repo_id}
            <span class="muted font-normal">· {r.files.length} changed file{r.files.length === 1 ? '' : 's'}</span></span>
          {#if r.files.length === 0}
            <span class="muted text-[length:var(--text-sm)]">No changes since the app last ran.</span>
          {:else}
            <ul class="grid gap-1">
              {#each r.files as f (f)}
                <li class="flex items-center gap-2">
                  <button class="ev-file" onclick={() => onfile(f)} title="Open in Changes">
                    <FileCode class="icon-sm" aria-hidden="true" /><span>{f}</span>
                  </button>
                  {#if r.flagged.has(f)}
                    <span class="chip" data-tone="warning"><TriangleAlert aria-hidden="true" />Runs code at start</span>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
          {#if (envKeys[r.repo_id] ?? []).length > 0}
            <span class="muted text-[length:var(--text-sm)]">
              .env keys passed: <span class="mono">{(envKeys[r.repo_id] ?? []).join(', ')}</span> (values are never shown)
            </span>
          {/if}
        </div>
      {/each}
    </div>
    <Dialog.Footer>
      <Button variant="ghost" onclick={oncancel}>Cancel</Button>
      <Button onclick={onconfirm}><Play />Run app</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
