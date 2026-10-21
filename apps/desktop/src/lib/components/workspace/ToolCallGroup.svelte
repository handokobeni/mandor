<script lang="ts">
  import Ban from '@lucide/svelte/icons/ban';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import FilePen from '@lucide/svelte/icons/file-pen';
  import FilePlus from '@lucide/svelte/icons/file-plus';
  import FileText from '@lucide/svelte/icons/file-text';
  import FolderSearch from '@lucide/svelte/icons/folder-search';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import SquareTerminal from '@lucide/svelte/icons/square-terminal';
  import TextSearch from '@lucide/svelte/icons/text-search';
  import Wrench from '@lucide/svelte/icons/wrench';
  import { summarizeCalls, toolLabel, type ToolCall } from '#lib/chat.ts';

  interface Props {
    calls: ToolCall[];
    /** Grup terakhir saat agent masih bekerja; call tanpa hasil sedang berjalan. */
    live: boolean;
  }
  let { calls, live }: Props = $props();

  const ICONS: Record<string, typeof Wrench> = {
    Read: FileText,
    Grep: TextSearch,
    Glob: FolderSearch,
    Edit: FilePen,
    MultiEdit: FilePen,
    Write: FilePlus,
    Bash: SquareTerminal
  };

  const denied = $derived(calls.filter((c) => c.denied).length);
  const failed = $derived(calls.filter((c) => !c.denied && c.result?.isError).length);
  const running = (c: ToolCall) => live && !c.result && !c.denied;
</script>

<details class="toolgroup" open={live || denied > 0 || failed > 0}>
  <summary>
    <ChevronRight class="chev icon-sm" aria-hidden="true" />
    {#if live}<LoaderCircle class="spin icon-sm" aria-hidden="true" />{:else}<Wrench class="icon-sm" aria-hidden="true" />{/if}
    <span>{summarizeCalls(calls)}</span>
    <span class="issues">
      {#if failed > 0}<span class="tissue failed"><CircleX class="icon-sm" aria-hidden="true" />{failed} failed</span>{/if}
      {#if denied > 0}<span class="tissue failed"><Ban class="icon-sm" aria-hidden="true" />{denied} denied</span>{/if}
    </span>
  </summary>
  {#each calls as c (c.id)}
    {@const Icon = ICONS[c.name] ?? Wrench}
    <details class="toolcall" data-s={c.denied ? 'denied' : c.result?.isError ? 'error' : 'ok'} open={!!c.denied}>
      <summary>
        {#if running(c)}<LoaderCircle class="spin icon-sm" aria-hidden="true" />{:else}<Icon class="icon-sm" aria-hidden="true" />{/if}
        <span class="tname" title={c.name}>{toolLabel(c.name)}</span>
        <span class="tprev">{c.preview}</span>
        <span class="tstate">
          {#if c.denied}<Ban class="icon-sm" aria-hidden="true" />Denied{:else if c.result?.isError}<CircleX class="icon-sm" aria-hidden="true" />Error{/if}
        </span>
      </summary>
      {#if c.denied}
        <div class="denial">{c.denied}</div>
      {:else}
        <pre>{c.result?.preview ?? (running(c) ? 'running…' : '')}</pre>
      {/if}
    </details>
  {/each}
</details>
