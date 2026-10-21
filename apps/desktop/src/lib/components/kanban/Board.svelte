<script lang="ts">
  import CalendarX2 from '@lucide/svelte/icons/calendar-x-2';
  import Check from '@lucide/svelte/icons/check';
  import ChevronsLeftRight from '@lucide/svelte/icons/chevrons-left-right';
  import ChevronsRightLeft from '@lucide/svelte/icons/chevrons-right-left';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import LogIn from '@lucide/svelte/icons/log-in';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import RefreshCwOff from '@lucide/svelte/icons/refresh-cw-off';
  import Search from '@lucide/svelte/icons/search';
  import SearchX from '@lucide/svelte/icons/search-x';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import { toast } from 'svelte-sonner';
  import { relTime } from '#lib/board.ts';
  import EmptyState from '#lib/components/mandor/EmptyState.svelte';
  import TaskCard from '#lib/components/mandor/TaskCard.svelte';
  import * as Alert from '#lib/components/ui/alert/index.ts';
  import { Button } from '#lib/components/ui/button/index.ts';
  import { Input } from '#lib/components/ui/input/index.ts';
  import * as Select from '#lib/components/ui/select/index.ts';
  import * as ToggleGroup from '#lib/components/ui/toggle-group/index.ts';
  import type { CardView } from '#lib/protocol/CardView.ts';
  import { board } from '#lib/stores/board.svelte.ts';
  import { lark } from '#lib/stores/lark.svelte.ts';
  import { nav } from '#lib/stores/nav.svelte.ts';

  interface Props {
    onstart: (card: CardView) => void;
    onopen: (card: CardView) => void;
    ontakeover: (card: CardView) => void;
    onretry: (card: CardView) => void;
    onclose: (card: CardView) => void;
    onrefresh: () => void;
  }
  let { onstart, onopen, ontakeover, onretry, onclose, onrefresh }: Props = $props();

  let doneCollapsed = $state(true);
  let now = $state(Date.now());

  $effect(() => {
    const id = setInterval(() => (now = Date.now()), 5000);
    return () => clearInterval(id);
  });

  const sprintValue = {
    get current() {
      return board.sprint ?? 'all';
    },
    set current(v: string) {
      board.sprint = v === 'all' ? null : v;
    }
  };
  // Pindah Mine/Team: indikator tampil dulu, papan baru dirender pada frame berikutnya.
  let switching = $state(false);
  const scope = {
    get current() {
      return board.mineOnly ? 'mine' : 'all';
    },
    set current(v: string) {
      // ToggleGroup mengirim string kosong saat item aktif diklik lagi; abaikan.
      if (!v || (v === 'mine') === board.mineOnly) return;
      switching = true;
      requestAnimationFrame(() =>
        setTimeout(() => {
          board.mineOnly = v === 'mine';
          requestAnimationFrame(() => (switching = false));
        }, 0)
      );
    }
  };

  // Kolom besar (papan Team bisa ratusan kartu) dirender bertahap.
  const PAGE = 40;
  let shown = $state<Record<string, number>>({});
  $effect(() => {
    void board.mineOnly;
    void board.sprint;
    void board.query;
    shown = {};
  });
  const limitOf = (id: string) => shown[id] ?? PAGE;
  // Kolom Unknown (status tanpa kolom) hanya relevan untuk kartu sendiri.
  const columns = $derived(board.columns.filter((c) => board.mineOnly || c.id !== 'unknown'));

  const total = $derived(board.visible.length);
  const sprintEmpty = $derived(!board.query && board.sprint !== null);

  // Kartu tidak bisa diseret; seret memberi petunjuk sekali per enam detik.
  let drag: { x: number; y: number } | null = null;
  let hinted = false;
  function onpointerdown(e: PointerEvent): void {
    const target = e.target as HTMLElement;
    if (!target.closest('.task-card') || target.closest('button, a')) return;
    drag = { x: e.clientX, y: e.clientY };
  }
  function onpointermove(e: PointerEvent): void {
    if (!drag || hinted) return;
    if (Math.abs(e.clientX - drag.x) + Math.abs(e.clientY - drag.y) > 14) {
      hinted = true;
      drag = null;
      toast.info("Cards can't be dragged", {
        description: 'They move when you Start, create a PR, or when the status changes in Lark.'
      });
      setTimeout(() => (hinted = false), 6000);
    }
  }
</script>

<svelte:window {onpointermove} onpointerup={() => (drag = null)} />

<div class="toolbar">
  <h1>Board</h1>
  <Select.Root type="single" bind:value={sprintValue.current}>
    <Select.Trigger size="sm" aria-label="Sprint">
      {board.sprint ? board.sprintName(board.sprint) : 'All sprints'}
    </Select.Trigger>
    <Select.Content>
      <Select.Item value="all" label="All sprints" />
      {#each board.sprintOpts as s (s.id)}<Select.Item value={s.id} label={s.label} />{/each}
    </Select.Content>
  </Select.Root>
  <div class="search">
    <Search class="icon-sm" aria-hidden="true" />
    <Input class="h-7 pl-7" placeholder="Search key or title" aria-label="Search by key or title" bind:value={board.query} />
  </div>
  <ToggleGroup.Root type="single" size="sm" variant="outline" bind:value={scope.current} aria-label="Whose tasks">
    <ToggleGroup.Item value="mine" title="Cards assigned only to you, plus the ones you are working on">Mine</ToggleGroup.Item>
    <ToggleGroup.Item value="all" title="All team cards: active ones, plus done in the last 14 days">Team</ToggleGroup.Item>
  </ToggleGroup.Root>
  <span style="flex:1"></span>
  <span class="sync" data-state={board.error || board.syncError ? 'error' : undefined} aria-live="polite">
    {#if switching}
      <LoaderCircle class="spin icon-sm" aria-hidden="true" /><span class="sync-text">
        {board.mineOnly ? 'Loading team cards…' : 'Loading your cards…'}
      </span>
    {:else if board.syncing}
      <LoaderCircle class="spin icon-sm" aria-hidden="true" /><span class="sync-text">Syncing with Lark…</span>
    {:else if board.loading}
      <LoaderCircle class="spin icon-sm" aria-hidden="true" /><span class="sync-text">Loading the board…</span>
    {:else if board.error}
      <RefreshCwOff class="icon-sm" aria-hidden="true" /><span class="sync-text">Can't load the board: {board.error}</span>
    {:else if board.syncError}
      <RefreshCwOff class="icon-sm" aria-hidden="true" /><span class="sync-text">
        Last sync failed{board.syncedAt ? ` (last good ${relTime(board.syncedAt, now)})` : ''}: {board.syncError}
      </span>
    {:else if board.syncedAt}
      <Check class="icon-sm" aria-hidden="true" /><span class="sync-text">Synced {relTime(board.syncedAt, now)}</span>
    {:else if board.loaded}
      <LoaderCircle class="spin icon-sm" aria-hidden="true" /><span class="sync-text">Syncing with Lark…</span>
    {/if}
  </span>
  <Button variant="secondary" size="sm" disabled={board.loading || board.syncing} onclick={onrefresh}>
    <RefreshCw class={board.syncing ? 'spin' : ''} />{board.syncing ? 'Syncing' : 'Refresh'}
  </Button>
</div>

{#if lark.status?.reauth_required}
  <div class="banners">
    <Alert.Root>
      <TriangleAlert class="text-status-warning" />
      <Alert.Title>Your Lark session expired</Alert.Title>
      <Alert.Description>
        The board is not syncing. Agents keep running, but status changes wait until you log in.
      </Alert.Description>
      <Alert.Action>
        <Button size="sm" onclick={() => nav.settings('lark')}><LogIn />Log in again</Button>
      </Alert.Action>
    </Alert.Root>
  </div>
{/if}

<div class="board scroll-area" data-switching={switching ? 'true' : undefined} role="presentation" aria-label="Kanban board. Cards move through actions or in Lark, not by dragging." {onpointerdown}>
  {#if lark.status && !lark.status.logged_in && !lark.status.reauth_required}
    <EmptyState icon={LogIn} title="Log in to Lark to load your tasks">
      Mandor reads your Sprint Tasks from Lark Base. Your login opens in the browser and comes back here.
      {#snippet actions()}
        <Button onclick={() => nav.settings('lark')}><LogIn />Log in with Lark</Button>
      {/snippet}
    </EmptyState>
  {:else if !board.loaded}
    {#each ['Todo', 'In Progress', 'Review', 'Done'] as name (name)}
      <section class="col" aria-hidden="true">
        <div class="col-head"><h2>{name}</h2></div>
        <div class="col-body">
          {#each [1, 2, 3] as i (i)}
            <div class="task-card">
              <div class="skeleton" style="height:10px;width:30%"></div>
              <div class="skeleton" style="height:12px;width:90%"></div>
              <div class="skeleton" style="height:12px;width:60%"></div>
            </div>
          {/each}
        </div>
      </section>
    {/each}
  {:else if total === 0}
    <EmptyState
      icon={sprintEmpty ? CalendarX2 : SearchX}
      title={sprintEmpty
        ? `No tasks assigned to you in ${board.sprintName(board.sprint ?? '')}`
        : board.query
          ? `No cards match "${board.query}"`
          : 'No tasks assigned to you'}
    >
      {sprintEmpty || !board.query
        ? 'Tasks appear here when you are an assignee in Lark Sprint Tasks.'
        : 'The search and filters are hiding every card on the board.'}
      {#snippet actions()}
        {#if board.query || board.sprint || board.mineOnly}
          <Button variant="outline" onclick={() => board.clearFilters()}>Clear filters</Button>
        {/if}
      {/snippet}
    </EmptyState>
  {:else}
    {#each columns as col (col.id)}
      {@const collapsed = col.id === 'done' && doneCollapsed}
      <section class="col" data-collapsed={collapsed ? 'true' : undefined} aria-label="{col.label}, {col.cards.length} cards">
        <div class="col-head">
          <h2>{col.label}</h2>
          <span class="count">{col.cards.length}</span>
          {#if col.id === 'done'}
            <Button
              variant="ghost"
              size="icon-xs"
              aria-expanded={!collapsed}
              aria-label="{collapsed ? 'Expand' : 'Collapse'} Done column"
              onclick={() => (doneCollapsed = !doneCollapsed)}
            >
              {#if collapsed}<ChevronsLeftRight />{:else}<ChevronsRightLeft />{/if}
            </Button>
          {/if}
        </div>
        <div class="col-body scroll-area">
          {#each col.cards.slice(0, limitOf(col.id)) as card (card.record_id)}
            <TaskCard {card} {onstart} {onopen} {ontakeover} {onretry} {onclose} />
          {:else}
            <div class="col-empty">{col.id === 'review' ? 'No PRs waiting for review' : 'Nothing here'}</div>
          {/each}
          {#if col.cards.length > limitOf(col.id)}
            {@const id = col.id}
            <Button variant="ghost" size="sm" class="w-full" onclick={() => (shown = { ...shown, [id]: limitOf(id) + PAGE })}>
              Show {Math.min(PAGE, col.cards.length - limitOf(id))} more · {col.cards.length - limitOf(id)} left
            </Button>
          {/if}
        </div>
      </section>
    {/each}
  {/if}
</div>
