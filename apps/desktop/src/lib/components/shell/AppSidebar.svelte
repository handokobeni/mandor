<script lang="ts">
  import Settings2 from '@lucide/svelte/icons/settings-2';
  import SquareKanban from '@lucide/svelte/icons/square-kanban';
  import { agentGroups } from '#lib/board.ts';
  import AgentRow from '#lib/components/mandor/AgentRow.svelte';
  import type { FlagView } from '#lib/protocol/FlagView.ts';
  import { board } from '#lib/stores/board.svelte.ts';
  import { lark } from '#lib/stores/lark.svelte.ts';
  import { nav } from '#lib/stores/nav.svelte.ts';
  import { people } from '#lib/stores/people.svelte.ts';
  import { runs } from '#lib/stores/runs.svelte.ts';
  import { workspace } from '#lib/stores/workspace.svelte.ts';
  import AppWindow from '@lucide/svelte/icons/app-window';
  import { appStarting, appStateMeta } from '#lib/app-run.ts';
  import { appRun } from '#lib/stores/app-run.svelte.ts';

  // Penanda run pemilik app yang menyala (hanya satu app dalam satu waktu).
  const appOwner = $derived(
    appRun.owner && (appRun.owner.state === 'running' || appStarting(appRun.owner.state)) ? appRun.owner : null
  );
  const appOwnerRun = $derived(appOwner ? runs.byId(appOwner.run_id) : undefined);

  const flags = $derived(
    new Map<string, FlagView>(board.cards.flatMap((c) => (c.flag ? [[c.record_id, c.flag] as const] : [])))
  );
  const groups = $derived(agentGroups(runs.sorted, flags));
  const onBoard = $derived(nav.page === 'board' && !workspace.selected);
  const me = $derived(lark.status?.open_id ?? board.me);

  // Inisial nama ("Beni Handoko" → "BH"); open_id tanpa nama memakai dua huruf setelah `ou_`.
  function initials(name: string): string {
    if (name.startsWith('ou_')) return name.slice(3, 5).toUpperCase();
    return name
      .split(/\s+/)
      .filter(Boolean)
      .slice(0, 2)
      .map((w) => w[0].toUpperCase())
      .join('');
  }
</script>

<aside class="sidebar" aria-label="Active agents">
  <nav class="sidebar-nav" aria-label="Main">
    <button type="button" class="nav-item" aria-current={onBoard ? 'page' : undefined} title="Board" onclick={() => nav.board()}>
      <SquareKanban class="icon" aria-hidden="true" /><span class="label-text">Board</span>
    </button>
    <button
      type="button"
      class="nav-item"
      aria-current={nav.page === 'settings' ? 'page' : undefined}
      title="Settings"
      onclick={() => nav.settings()}
    >
      <Settings2 class="icon" aria-hidden="true" /><span class="label-text">Settings</span>
    </button>
  </nav>
  <hr class="separator" />
  {#if appOwner}
    <button
      type="button"
      class="nav-item app-owner"
      title="This run's app is running. Only one run's app runs at a time."
      onclick={() => appOwnerRun && nav.run(appOwnerRun.id)}
    >
      <AppWindow class="icon" aria-hidden="true" />
      <span class="label-text">
        App: <span class="mono">{appOwnerRun?.key ?? 'a run'}</span>
        <span class="muted">· {appStateMeta(appOwner.state).label}</span>
      </span>
    </button>
  {/if}
  <div class="sidebar-group-label caps" style="padding-top:12px">
    <span class="label-text">Active agents</span><span class="count mono">{runs.runs.length}</span>
  </div>
  <div class="sidebar-list scroll-area">
    {#each groups as g (g.id)}
      <div class="sidebar-group-label caps">
        <span class="label-text">{g.label}</span><span class="count mono">{g.runs.length}</span>
      </div>
      <div class="group">
        {#each g.runs as r (r.id)}
          <AgentRow
            run={r}
            flag={flags.get(r.record_id)}
            current={nav.page === 'board' && workspace.selected === r.id}
            onselect={() => nav.run(r.id)}
          />
        {/each}
      </div>
    {:else}
      <p class="muted label-text" style="padding:8px 16px;font-size:12px">
        No agents running. Start a task from the board.
      </p>
    {/each}
  </div>
  {#if me}
    {@const name = people.name(me)}
    <div class="sidebar-footer">
      <span class="avatar avatar-md" aria-hidden="true">{initials(name)}</span>
      <div class="who">{name}<span>Lark · Sprint Tasks</span></div>
    </div>
  {/if}
</aside>
