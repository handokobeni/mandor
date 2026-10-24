<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { toast } from 'svelte-sonner';
  import { appRun } from '#lib/stores/app-run.svelte.ts';
  import { background } from '#lib/stores/background.svelte.ts';
  import { TauriTransport } from '#lib/api/tauri-transport.ts';
  import { flagMeta } from '#lib/board.ts';
  import Board from '#lib/components/kanban/Board.svelte';
  import CloseRunDialog from '#lib/components/kanban/CloseRunDialog.svelte';
  import StartTaskDialog from '#lib/components/kanban/StartTaskDialog.svelte';
  import Settings from '#lib/components/settings/Settings.svelte';
  import CommandPalette from '#lib/components/shell/CommandPalette.svelte';
  import Onboarding from '#lib/components/shell/Onboarding.svelte';
  import { type OnboardingStep, onboardingStart, shouldNotify } from '#lib/settings.ts';
  import { notifyPrefs } from '#lib/stores/notify-prefs.svelte.ts';
  import AppSidebar from '#lib/components/shell/AppSidebar.svelte';
  import ConnectionScreen from '#lib/components/shell/ConnectionScreen.svelte';
  import TitleBar from '#lib/components/shell/TitleBar.svelte';
  import { Toaster } from '#lib/components/ui/sonner/index.ts';
  import Workspace from '#lib/components/workspace/Workspace.svelte';
  import { notifyDesktop, onTrayOpenRun, updateTray } from '#lib/native.ts';
  import type { CardView } from '#lib/protocol/CardView.ts';
  import type { Rpc } from '#lib/rpc.ts';
  import { board } from '#lib/stores/board.svelte.ts';
  import { connection } from '#lib/stores/connection.svelte.ts';
  import { vm } from '#lib/stores/vm.svelte.ts';
  import { vmConnection } from '#lib/stores/connection.svelte.ts';
  import { overnight } from '#lib/stores/overnight.svelte.ts';
  import OvernightPage from '#lib/components/overnight/OvernightPage.svelte';
  import { supportsVmLink } from '#lib/vm.ts';
  import { lark } from '#lib/stores/lark.svelte.ts';
  import { nav } from '#lib/stores/nav.svelte.ts';
  import { notifications } from '#lib/stores/notifications.svelte.ts';
  import { people } from '#lib/stores/people.svelte.ts';
  import { runs } from '#lib/stores/runs.svelte.ts';
  import { workspace } from '#lib/stores/workspace.svelte.ts';

  // Onboarding dilewati atau selesai di perangkat ini.
  const ONBOARDED_KEY = 'mandor:onboarded';
  function readOnboarded(): boolean {
    try {
      return localStorage.getItem(ONBOARDED_KEY) === '1';
    } catch {
      return false;
    }
  }
  let onboarded = $state(readOnboarded());
  function finishOnboarding(): void {
    onboarded = true;
    try {
      localStorage.setItem(ONBOARDED_KEY, '1');
    } catch {
      // Tanpa penyimpanan, onboarding muncul lagi saat app dibuka ulang selama setup belum lengkap.
    }
  }
  // Langkah awal dihitung sekali per sesi agar tidak melompat saat status berubah di tengah setup.
  let onboardingAt = $state<OnboardingStep | null | undefined>(undefined);
  $effect(() => {
    if (onboardingAt !== undefined || !lark.loaded) return;
    onboardingAt = onboardingStart(lark.status, lark.repos.length, onboarded);
  });

  let starting = $state<{ card: CardView; action: 'start' | 'takeover' } | null>(null);
  let closing = $state<CardView | null>(null);

  const rpc = $derived<Rpc | null>(connection.state.kind === 'ready' ? connection.client : null);
  const selectedRun = $derived(workspace.selected ? runs.byId(workspace.selected) : undefined);

  onMount(() => {
    let stop = () => {};
    void (async () => {
      // `/?mock` di mode dev: data contoh tanpa daemon, untuk meninjau tampilan.
      const mock = import.meta.env.DEV && location.search.includes('mock');
      const transport = mock
        ? new (await import('#lib/dev/mock-transport.ts')).MockTransport()
        : new TauriTransport();
      if (mock) vm.useMock('beni@overnight-vm', new (await import('#lib/dev/mock-vm.ts')).MockVmTransport());
      const client = connection.start(transport);
      stop = () => client.stop();
    })();
    return () => stop();
  });

  // Link VM dibuka setelah daemon laptop siap dan mendukung klaim per daemon
  // (KTD-55 plan VM); tidak di mode contoh.
  $effect(() => {
    const s = connection.state;
    if (s.kind === 'ready' && supportsVmLink(s.protocol) && !location.search.includes('mock')) {
      untrack(() => void vm.load());
    }
  });

  // Store Overnight mengikuti link VM: lepas saat terputus (badge antrean hilang).
  $effect(() => {
    const client = vmConnection.state.kind === 'ready' ? vmConnection.client : null;
    if (!client) {
      untrack(() => overnight.detach());
      return;
    }
    return untrack(() => overnight.attach(client));
  });

  // Store disambungkan ulang setiap kali koneksi siap (termasuk setelah reconnect).
  $effect(() => {
    const client = rpc;
    if (!client) return;
    // Hanya `rpc` yang dilacak; state yang dibaca store saat memuat tidak boleh memicu attach ulang.
    const offs = untrack(() => [
      lark.attach(client, (n) => {
        if (shouldNotify(n.kind, notifyPrefs.prefs)) void notifyDesktop(n);
      }),
      board.attach(client),
      runs.attach(client),
      workspace.attach(client),
      notifications.attach(client),
      people.attach(client),
      appRun.attach(client),
      background.attach(client)
    ]);
    return () => offs.forEach((off) => off());
  });

  // Menu tray mengikuti run yang menunggu dev.
  $effect(() => {
    const waiting = runs.sorted
      .filter((r) => r.state === 'awaiting_dev')
      .map((r) => ({ run_id: r.id, key: r.key ?? r.record_id, title: r.title }));
    void updateTray(waiting);
  });
  onMount(() => onTrayOpenRun((runId) => nav.run(runId)));

  /** Menjalankan aksi; gagal ditampilkan sebagai toast error, sukses opsional. */
  async function act(fn: () => Promise<unknown>, ok?: string, okDescription?: string): Promise<void> {
    try {
      await fn();
      if (ok) toast.success(ok, { description: okDescription });
    } catch (err) {
      toast.error('Something went wrong', { description: err instanceof Error ? err.message : String(err) });
    }
  }

  function refresh(): void {
    const client = rpc;
    if (!client) return;
    board.beginSync();
    void act(async () => {
      try {
        await client.request('board.refresh');
      } catch (err) {
        // Permintaan tidak sampai ke daemon: tidak akan ada hasil poll yang ditunggu.
        board.syncing = false;
        throw err;
      }
    });
  }
</script>

<Toaster position="bottom-right" />

{#if connection.state.kind !== 'ready'}
  <ConnectionScreen state={connection.state} />
{:else if rpc}
  {@const client = rpc}
  <div class="app">
    <TitleBar onrefresh={refresh} />
    <div class="body">
      <AppSidebar />
      <main class="main">
        {#if onboardingAt && !onboarded}
          <Onboarding rpc={client} start={onboardingAt} onfinish={finishOnboarding} />
        {:else if nav.page === 'settings'}
          <Settings rpc={client} />
        {:else if nav.page === 'overnight'}
          <OvernightPage laptop={client} laptopDaemonId={background.status?.daemon_id ?? null} />
        {:else if selectedRun}
          {#key selectedRun.id}
            <Workspace rpc={client} run={selectedRun} />
          {/key}
        {:else}
          <Board
            onstart={(card) => (starting = { card, action: 'start' })}
            ontakeover={(card) => (starting = { card, action: 'takeover' })}
            onopen={(card) => card.run && nav.run(card.run.id)}
            onretry={(card) =>
              act(() => client.request('card.retry', { record_id: card.record_id }), `Retried ${card.key ?? ''}`)}
            onclose={(card) => (closing = card)}
            onrefresh={refresh}
          />
        {/if}
      </main>
    </div>
  </div>

  <CommandPalette onstart={(card) => (starting = { card, action: 'start' })} onrefresh={refresh} />

  {#if starting}
    <StartTaskDialog
      rpc={client}
      card={starting.card}
      action={starting.action}
      repos={lark.repos}
      workspaces={lark.workspaces}
      onclose={() => (starting = null)}
      ondone={(runId, open) => {
        starting = null;
        void runs.load();
        if (open) nav.run(runId);
      }}
      onaddrepo={() => {
        starting = null;
        nav.settings('repos');
      }}
      onrefresh={refresh}
    />
  {/if}

  {#if closing?.run}
    {@const card = closing}
    {@const runId = closing.run.id}
    <CloseRunDialog
      taskKey={card.key ?? card.record_id}
      branch={closing.run.branch}
      reason={card.flag ? flagMeta(card.flag.kind).label : undefined}
      oncancel={() => (closing = null)}
      onconfirm={() => {
        closing = null;
        void act(() => client.request('runs.close', { run_id: runId }), 'Run closed', `${card.key ?? ''} · worktree removed`);
      }}
    />
  {/if}
{/if}
