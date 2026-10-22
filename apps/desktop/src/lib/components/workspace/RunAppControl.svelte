<script lang="ts">
  // Kontrol Run app di action bar (brief §4.4): state, URL app, dan tombol.
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import CirclePause from '@lucide/svelte/icons/circle-pause';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import Play from '@lucide/svelte/icons/play';
  import ScrollText from '@lucide/svelte/icons/scroll-text';
  import Square from '@lucide/svelte/icons/square';
  import { appControlAction, appControlLabel, appStateMeta } from '#lib/app-run.ts';
  import { Button } from '#lib/components/ui/button/index.ts';
  import type { AppRunView } from '#lib/protocol/AppRunView.ts';

  interface Props {
    app: AppRunView | null;
    disabled: boolean;
    onrun: () => void;
    onstop: () => void;
    onlogs: () => void;
  }
  let { app, disabled, onrun, onstop, onlogs }: Props = $props();

  const meta = $derived(appStateMeta(app?.state ?? 'off'));
  const action = $derived(appControlAction(app));
  const url = $derived(app?.state === 'running' ? (app.members.find((m) => m.url)?.url ?? null) : null);
</script>

<span class="run-app">
  <span class="run" data-tone={meta.tone}>
    {#if meta.spin}<LoaderCircle class="spin" aria-hidden="true" />{:else if app?.state === 'running'}<CircleCheck
        aria-hidden="true"
      />{:else if app?.state === 'failed'}<CircleX aria-hidden="true" />{:else}<CirclePause aria-hidden="true" />{/if}
    <span class="lbl">{appControlLabel(app)}</span>
  </span>
  {#if url}
    <a class="link mono" href={url} target="_blank" rel="noopener noreferrer" title="Open in your browser">{url}</a>
  {/if}
  {#if action === 'run'}
    <Button variant="outline" size="sm" {disabled} onclick={onrun}><Play />Run app</Button>
  {:else if action === 'stop'}
    <Button variant="ghost" size="sm" onclick={onstop}><Square />Stop app</Button>
  {:else}
    <Button variant="outline" size="sm" onclick={onlogs}><ScrollText />View logs</Button>
    <Button variant="ghost" size="sm" {disabled} onclick={onrun}><Play />Retry</Button>
  {/if}
</span>
