<script lang="ts">
  // Log app per repo saat menyala atau setelah gagal. Teks biasa, sudah diredaksi daemon.
  import { Button } from '#lib/components/ui/button/index.ts';
  import * as Dialog from '#lib/components/ui/dialog/index.ts';
  import type { AppLogView } from '#lib/protocol/AppLogView.ts';

  interface Props {
    logs: AppLogView[];
    error: string | null;
    onclose: () => void;
  }
  let { logs, error, onclose }: Props = $props();
</script>

<Dialog.Root open onOpenChange={(o) => !o && onclose()}>
  <Dialog.Content class="sm:max-w-[760px]">
    <Dialog.Header>
      <Dialog.Title class="text-[length:var(--text-md)]">App logs</Dialog.Title>
      {#if error}<Dialog.Description>{error}</Dialog.Description>{/if}
    </Dialog.Header>
    <div class="grid gap-3" style="max-height:60vh;overflow:auto">
      {#each logs as l (l.repo_id)}
        <div class="grid gap-1">
          <span class="text-sm font-medium">{l.repo_id}</span>
          <pre class="ev-pre">{l.text || 'No output yet.'}</pre>
        </div>
      {:else}
        <p class="muted">No output yet.</p>
      {/each}
    </div>
    <Dialog.Footer><Button variant="ghost" onclick={onclose}>Close</Button></Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
