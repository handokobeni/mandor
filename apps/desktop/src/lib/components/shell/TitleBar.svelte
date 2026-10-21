<script lang="ts">
  import Hand from '@lucide/svelte/icons/hand';
  import LogIn from '@lucide/svelte/icons/log-in';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Search from '@lucide/svelte/icons/search';
  import Settings from '@lucide/svelte/icons/settings';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import UserRoundCheck from '@lucide/svelte/icons/user-round-check';
  import NotificationCenter from './NotificationCenter.svelte';
  import BrandMark from '#lib/components/mandor/BrandMark.svelte';
  import { Button } from '#lib/components/ui/button/index.ts';
  import { board } from '#lib/stores/board.svelte.ts';
  import { connection } from '#lib/stores/connection.svelte.ts';
  import { lark } from '#lib/stores/lark.svelte.ts';
  import { nav } from '#lib/stores/nav.svelte.ts';
  import { people } from '#lib/stores/people.svelte.ts';
  import { runs } from '#lib/stores/runs.svelte.ts';

  let { onrefresh }: { onrefresh?: () => void } = $props();

  const isMac = typeof navigator !== 'undefined' && navigator.userAgent.includes('Mac');

  const waiting = $derived(runs.sorted.filter((r) => r.state === 'awaiting_dev'));
  const ready = $derived(connection.state.kind === 'ready');
  const version = $derived(connection.state.kind === 'ready' ? connection.state.daemonVersion : null);
</script>

<header class="titlebar">
  <button type="button" class="brand" onclick={() => nav.board()}>
    <BrandMark />Mandor
  </button>
  {#if onrefresh}
    <span class="separator-v" style="height:16px;align-self:center"></span>
    <span
      class="conn"
      data-state={ready ? 'connected' : 'reconnecting'}
      title={version ? `mandord ${version}` : 'Lost the daemon, reconnecting'}
    >
      <span class="dot"></span><span class="conn-text">{ready ? 'Daemon connected' : 'Reconnecting…'}</span>
    </span>
    {#if lark.status?.reauth_required}
      <button type="button" class="conn" data-state="expired" onclick={() => nav.settings('lark')}>
        <TriangleAlert class="icon-sm" aria-hidden="true" /><span class="conn-text">Lark session expired</span>
      </button>
    {:else if lark.status && !lark.status.logged_in}
      <button type="button" class="conn" data-state="expired" onclick={() => nav.settings('lark')}>
        <LogIn class="icon-sm" aria-hidden="true" /><span class="conn-text">Not logged in to Lark</span>
      </button>
    {:else if lark.status?.logged_in}
      <span class="conn" title="Lark session valid">
        <UserRoundCheck class="icon-sm" aria-hidden="true" /><span class="conn-text">Lark · {lark.status.open_id ? people.name(lark.status.open_id) : ''}</span>
      </span>
    {/if}
  {/if}
  <span class="spacer"></span>
  {#if onrefresh}
    {#if waiting.length > 0}
      <button
        type="button"
        class="tally"
        title="Jump to the agent waiting for you"
        onclick={() => nav.run(waiting[0].id)}
      >
        <Hand class="icon-sm" aria-hidden="true" />{waiting.length} waiting for you
      </button>
    {/if}
    <button type="button" class="cmd-trigger" onclick={() => (nav.paletteOpen = true)}>
      <Search class="icon-sm" aria-hidden="true" /><span class="cmd-text">Open task, start, settings…</span>
      <span class="kbd">{isMac ? '⌘K' : 'Ctrl K'}</span>
    </button>
    <Button variant="ghost" size="icon-sm" aria-label="Refresh board" title="Refresh board" onclick={onrefresh}>
      <RefreshCw class={board.syncing ? 'spin' : ''} />
    </Button>
    <NotificationCenter />
    <Button
      variant="ghost"
      size="icon-sm"
      aria-label="Settings"
      title="Settings"
      onclick={() => nav.settings()}
    >
      <Settings />
    </Button>
  {/if}
</header>
