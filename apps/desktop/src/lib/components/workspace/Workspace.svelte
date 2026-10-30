<script lang="ts">
  import { runLabel } from '#lib/board.ts';
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import ClipboardList from '@lucide/svelte/icons/clipboard-list';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import Copy from '@lucide/svelte/icons/copy';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import FileDiff from '@lucide/svelte/icons/file-diff';
  import GitBranch from '@lucide/svelte/icons/git-branch';
  import GitPullRequest from '@lucide/svelte/icons/git-pull-request';
  import GitPullRequestArrow from '@lucide/svelte/icons/git-pull-request-arrow';
  import Info from '@lucide/svelte/icons/info';
  import ListChecks from '@lucide/svelte/icons/list-checks';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import MessageSquareText from '@lucide/svelte/icons/message-square-text';
  import MessagesSquare from '@lucide/svelte/icons/messages-square';
  import PanelRight from '@lucide/svelte/icons/panel-right';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import Square from '@lucide/svelte/icons/square';
  import SquareKanban from '@lucide/svelte/icons/square-kanban';
  import Wrench from '@lucide/svelte/icons/wrench';
  import X from '@lucide/svelte/icons/x';
  import ClipboardCheck from '@lucide/svelte/icons/clipboard-check';
  import CircleQuestionMark from '@lucide/svelte/icons/circle-question-mark';
  import Lock from '@lucide/svelte/icons/lock';
  import ScanSearch from '@lucide/svelte/icons/scan-search';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import { untrack } from 'svelte';
  import { toast } from 'svelte-sonner';
  import { flagMeta, isWorking, quietLeft } from '#lib/board.ts';
  import CloseRunDialog from '#lib/components/kanban/CloseRunDialog.svelte';
  import MarkdownDoc from '#lib/components/mandor/MarkdownDoc.svelte';
  import RunStatusBadge from '#lib/components/mandor/RunStatusBadge.svelte';
  import { copyText } from '#lib/components/mandor/copy.ts';
  import { flagIcon, runIcon } from '#lib/components/mandor/icons.ts';
  import * as Alert from '#lib/components/ui/alert/index.ts';
  import { Badge } from '#lib/components/ui/badge/index.ts';
  import { Button } from '#lib/components/ui/button/index.ts';
  import * as DropdownMenu from '#lib/components/ui/dropdown-menu/index.ts';
  import * as Tabs from '#lib/components/ui/tabs/index.ts';
  import type { CommentView } from '#lib/protocol/CommentView.ts';
  import type { FileChangeView } from '#lib/protocol/FileChangeView.ts';
  import type { RunView } from '#lib/protocol/RunView.ts';
  import type { TaskDetailsView } from '#lib/protocol/TaskDetailsView.ts';
  import type { Rpc } from '#lib/rpc.ts';
  import { board } from '#lib/stores/board.svelte.ts';
  import { nav } from '#lib/stores/nav.svelte.ts';
  import { skills } from '#lib/stores/skills.svelte.ts';
  import { prLinks } from '#lib/workspaces.ts';
  import ChatStream from './ChatStream.svelte';
  import CommentsTab from './CommentsTab.svelte';
  import Composer from './Composer.svelte';
  import DiffViewer from './DiffViewer.svelte';
  import StartFixDialog from './StartFixDialog.svelte';
  import AcReviewTab from './AcReviewTab.svelte';
  import PlanApproval from './PlanApproval.svelte';
  import GateDialogs from './GateDialogs.svelte';
  import { activeRows, gate as acGate, gateButtonLabel, label as acLabel, tabCount } from '#lib/ac-review.ts';
  import * as Popover from '#lib/components/ui/popover/index.ts';
  import type { PublishParams } from '#lib/protocol/PublishParams.ts';
  import type { PublishResult } from '#lib/protocol/PublishResult.ts';
  import { LedgerStore } from '#lib/stores/ledger.svelte.ts';
  import { showQueueForVm } from '#lib/overnight.ts';
  import { overnight } from '#lib/stores/overnight.svelte.ts';
  import QueueForVm from './QueueForVm.svelte';
  import { appRun } from '#lib/stores/app-run.svelte.ts';
  import { runs } from '#lib/stores/runs.svelte.ts';
  import { snapshots } from '#lib/app-run.ts';
  import type { AppChangesView } from '#lib/protocol/AppChangesView.ts';
  import type { AppLogView } from '#lib/protocol/AppLogView.ts';
  import type { RepoView } from '#lib/protocol/RepoView.ts';
  import type { ReviewReadinessView } from '#lib/protocol/ReviewReadinessView.ts';
  import AppLogsDrawer from './AppLogsDrawer.svelte';
  import ReadinessDialog from './ReadinessDialog.svelte';
  import RunAppConfirm from './RunAppConfirm.svelte';
  import RunAppControl from './RunAppControl.svelte';
  import TaskDetails from './TaskDetails.svelte';
  import TaskTab from './TaskTab.svelte';
  import GroupProgress from './GroupProgress.svelte';
  import ChangedCardsDialog from './ChangedCardsDialog.svelte';
  import SkipForward from '@lucide/svelte/icons/skip-forward';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import Play from '@lucide/svelte/icons/play';
  import OctagonX from '@lucide/svelte/icons/octagon-x';
  import { cardKey, changedCards, groupAction, headerCard } from '#lib/group.ts';

  let { rpc, run }: { rpc: Rpc; run: RunView } = $props();

  let tab = $state('chat');
  let busy = $state(false);
  let panelOpen = $state(false);
  let files = $state<FileChangeView[] | null>(null);
  let selectedFile = $state<string | null>(null);
  let patch = $state<string | null>(null);
  let comments = $state<CommentView[] | null>(null);
  let commentsError = $state<string | null>(null);
  let details = $state<TaskDetailsView | null>(null);
  let detailsError = $state<string | null>(null);
  let fixOpen = $state(false);
  let closeOpen = $state(false);
  let now = $state(Date.now());
  let gateDialog = $state<'manual' | 'anyway' | 'warn' | null>(null);
  let blockedOpen = $state(false);
  let syncWarning = $state<string | null>(null);

  // Run grup (plan run grup U9).
  const grouped = $derived(run.cards.length > 0);
  const nextAction = $derived(groupAction(run));
  // Kunci kartu dari hasil Push yang tertahan; run.cards bisa belum diperbarui.
  let changedKeys = $state<string[]>([]);
  const changedOpen = $derived(changedKeys.length > 0);
  const changedList = $derived.by(() => {
    const fromRun = changedCards(run.cards);
    return fromRun.length > 0 ? fromRun : run.cards.filter((c) => changedKeys.includes(cardKey(c)));
  });
  const nextKey = $derived.by(() => {
    const next = run.cards.find((c) => c.state === 'pending');
    return next ? cardKey(next) : null;
  });

  async function groupStep(): Promise<void> {
    if (nextAction === 'continue') {
      await act(() => rpc.request('runs.continue_group', { run_id: run.id }));
    } else {
      await act(
        () => rpc.request('runs.next_card', { run_id: run.id }),
        nextAction === 'finish' ? 'Card committed. Review the AC, then Push & PR.' : undefined
      );
    }
  }

  async function decideCard(recordId: string, decision: 'revert' | 'keep', reason: string | null): Promise<void> {
    const c = run.cards.find((x) => x.record_id === recordId);
    const k = c ? cardKey(c) : recordId;
    busy = true;
    try {
      const res = await rpc.request<{ outcome: string }>('runs.gate_card_decision', {
        run_id: run.id,
        record_id: recordId,
        decision,
        reason
      });
      if (res.outcome === 'revert_conflict') {
        toast.error(`Revert conflict on ${k}`, { description: 'Nothing changed in the branch. Resolve it there, or keep the card.' });
        return;
      }
      toast.success(res.outcome === 'reverted' ? `Reverted ${k}. Review the AC again.` : `Kept ${k} in the PR`);
      changedKeys = changedKeys.filter((x) => x !== k);
      if (changedList.length <= 1) changedKeys = [];
      await ledger.load();
    } catch (err) {
      toast.error('Something went wrong', { description: message(err) });
    } finally {
      busy = false;
    }
  }

  // Buku AC run (Rilis 2): dimuat ulang saat run berganti.
  const ledger = new LedgerStore();
  $effect(() => {
    const id = run.id;
    return untrack(() => ledger.attach(rpc, id));
  });
  const reviewing = $derived(run.state === 'reviewing');
  const acRows = $derived(ledger.ledger ? activeRows(ledger.ledger.rows) : []);
  const gateNow = $derived(acGate(acRows, ledger.ledger?.gate_mode ?? 'block', reviewing));
  const acCount = $derived(tabCount(acRows));
  const failing = $derived(acRows.filter((r) => r.in_gate && !r.stale && r.verdict === 'fail'));

  // Run app (Rilis 2 U13).
  let repos = $state<RepoView[]>([]);
  let appChanges = $state<AppChangesView[] | null>(null);
  let appLogs = $state<AppLogView[] | null>(null);
  let readiness = $state<{ view: ReviewReadinessView; trigger: 'manual' | 'push' } | null>(null);
  let checking = $state(false);
  $effect(() => {
    const id = run.id;
    untrack(() => appRun.watch(id));
  });
  $effect(() => {
    untrack(() => {
      void rpc
        .request<RepoView[]>('repos.list')
        .then((r) => (repos = r))
        .catch(() => (repos = []));
    });
  });
  const memberIds = $derived(run.repos.map((r) => r.repo_id));
  const runnable = $derived(
    repos.some((r) => memberIds.includes(r.id) && !!r.app.start_command && r.app.port !== null)
  );
  const envKeys = $derived(Object.fromEntries(repos.filter((r) => memberIds.includes(r.id)).map((r) => [r.id, r.app.env_keys])));
  const ownerRun = $derived(appRun.otherOwner(run.id));
  const ownerKey = $derived(ownerRun ? (runs.byId(ownerRun.run_id)?.key ?? 'another run') : null);

  // Detak per detik hanya selama jeda tenang (hitung mundur).
  $effect(() => {
    if (run.state !== 'wf_quiet' || !run.quiet_until_ms) return;
    const id = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(id);
  });

  const key = $derived(run.key ?? run.record_id);
  // Run grup: header menampilkan kartu yang sedang dikerjakan.
  const header = $derived(headerCard(run));
  const card = $derived(board.card(run.record_id));
  const working = $derived(isWorking(run.state));
  const flag = $derived(card?.flag ?? null);
  const prs = $derived(prLinks(run.pr_url));
  const planning = $derived(run.phase === 'plan');
  // Saran perintah `/skill` di chat.
  $effect(() => skills.load(rpc));
  const brainstorming = $derived(run.phase === 'brainstorm');

  // Plan baru (atau revisi) langsung ditampilkan.
  $effect(() => {
    if (planning && run.state === 'awaiting_dev' && run.plan) untrack(() => (tab = 'plan'));
  });

  function continueToPlan(): void {
    void act(() => rpc.request('runs.continue_to_plan', { run_id: run.id }), 'The agent is writing the plan');
  }

  // VM terhubung, berperan VM, dan sudah login Lark: kartu bisa diantre.
  const vmQueue = $derived(showQueueForVm(overnight, grouped));

  // AC yang ditandai Follow design di tab Plan; dikirim saat approve.
  let follow = $state<string[]>([]);

  function approvePlan(): void {
    void act(async () => {
      await rpc.request('runs.approve_plan', { run_id: run.id, follow_design: follow });
      follow = [];
      tab = 'chat';
    }, 'Plan approved; the agent is coding');
  }

  function message(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
  }

  async function act(fn: () => Promise<unknown>, ok?: string): Promise<void> {
    busy = true;
    try {
      await fn();
      if (ok) toast.success(ok);
    } catch (err) {
      toast.error('Something went wrong', { description: message(err) });
    } finally {
      busy = false;
    }
  }

  async function loadChanges(): Promise<void> {
    try {
      files = await rpc.request<FileChangeView[]>('runs.changes', { run_id: run.id });
      if (selectedFile && !files.some((f) => f.path === selectedFile)) selectedFile = null;
      if (!selectedFile && files.length > 0) await selectFile(files[0].path);
      else if (selectedFile) await selectFile(selectedFile);
    } catch (err) {
      files = [];
      toast.error('Could not load changes', { description: message(err) });
    }
  }

  async function selectFile(path: string): Promise<void> {
    selectedFile = path;
    patch = null;
    try {
      patch = await rpc.request<string>('runs.file_diff', { run_id: run.id, path });
    } catch (err) {
      patch = '';
      toast.error('Could not load the diff', { description: message(err) });
    }
  }

  async function loadComments(): Promise<void> {
    try {
      comments = await rpc.request<CommentView[]>('card.comments', { record_id: run.record_id });
      commentsError = null;
    } catch (err) {
      commentsError = message(err);
    }
  }

  async function loadDetails(): Promise<void> {
    try {
      details = await rpc.request<TaskDetailsView>('card.details', { record_id: run.record_id });
      detailsError = null;
    } catch (err) {
      detailsError = message(err);
    }
  }

  // Jumlah berkas berubah dan komentar dibutuhkan header tab; muat ulang setiap state run berubah.
  $effect(() => {
    void run.state;
    untrack(() => {
      void loadChanges();
      void loadComments();
      void loadDetails();
    });
  });

  // Agent bisa sedang mengubah berkas; discard hanya saat worktree diam.
  const canDiscard = $derived(!working && !['preparing', 'publishing'].includes(run.state));

  function discardFile(path: string): void {
    void act(async () => {
      await rpc.request('runs.discard_file', { run_id: run.id, path });
      if (selectedFile === path) selectedFile = null;
      await loadChanges();
    }, 'Changes discarded');
  }

  /** Push & PR mengikuti state gerbang AC (brief §4.3). */
  function publish(): void {
    switch (gateNow.state) {
      case 'blocked':
        blockedOpen = true;
        return;
      case 'needs_confirmation':
        gateDialog = 'manual';
        return;
      case 'warn':
        gateDialog = 'warn';
        return;
      case 'review_needed':
        void requestReview('push');
        return;
      default:
        void push({ run_id: run.id });
    }
  }

  async function push(params: PublishParams): Promise<void> {
    gateDialog = null;
    await act(async () => {
      const res = await rpc.request<PublishResult>('runs.publish', params);
      syncWarning = res.sync_warning;
      switch (res.outcome) {
        case 'published':
          toast.success('Pushed and opened the PR', res.sync_warning ? { description: res.sync_warning } : undefined);
          break;
        case 'review_started':
          toast.info('Review started', { description: 'Nothing was pushed yet. Press Push & create PR again when it finishes.' });
          break;
        case 'blocked':
          toast.error(`Push blocked by ${res.failed.join(', ')}`);
          await ledger.load();
          break;
        case 'cards_changed':
          toast.warning(`${res.changed_cards.join(', ')} changed in Lark`, {
            description: 'Decide whether to revert or keep each card before the AC review and the PR.'
          });
          changedKeys = res.changed_cards;
          break;
        default:
          toast.warning(`Needs your check: ${res.unverified.join(', ')}`);
          await ledger.load();
      }
    });
  }

  /** Review dimulai lewat checklist kesiapan bila run punya repo Web (KTD-39). */
  async function requestReview(trigger: 'manual' | 'push'): Promise<void> {
    try {
      const view = await rpc.request<ReviewReadinessView>('review.readiness', { run_id: run.id });
      if (view.needs_dialog) readiness = { view, trigger };
      else await beginReview(trigger);
    } catch (err) {
      toast.error('Something went wrong', { description: message(err) });
    }
  }

  async function beginReview(trigger: 'manual' | 'push'): Promise<void> {
    readiness = null;
    if (trigger === 'push') await push({ run_id: run.id });
    else await act(() => rpc.request('review.start', { run_id: run.id }), 'Review started');
  }

  async function startFromReadiness(confirmed: { database: boolean; login: boolean }, trigger: 'manual' | 'push'): Promise<void> {
    try {
      await rpc.request('app.confirm', { run_id: run.id, database: confirmed.database, login: confirmed.login });
    } catch (err) {
      toast.error('Could not save your confirmation', { description: message(err) });
      return;
    }
    await beginReview(trigger);
  }

  async function recheckChrome(): Promise<void> {
    if (!readiness) return;
    checking = true;
    try {
      await rpc.request('system.chrome_check', { force: true });
      const view = await rpc.request<ReviewReadinessView>('review.readiness', { run_id: run.id });
      readiness = { ...readiness, view };
    } catch (err) {
      toast.error('Could not check the extension', { description: message(err) });
    } finally {
      checking = false;
    }
  }

  async function openRunApp(): Promise<void> {
    readiness = null;
    try {
      appChanges = await rpc.request<AppChangesView[]>('app.changes', { run_id: run.id });
    } catch (err) {
      toast.error('Could not prepare the app', { description: message(err) });
    }
  }

  function confirmRunApp(): void {
    const changes = appChanges ?? [];
    appChanges = null;
    void act(() => rpc.request('app.run', { run_id: run.id, snapshots: snapshots(changes) }), 'Starting the app');
  }

  async function openLogs(): Promise<void> {
    try {
      appLogs = await rpc.request<AppLogView[]>('app.logs', { run_id: run.id });
    } catch (err) {
      toast.error('Could not load the logs', { description: message(err) });
    }
  }

  function sendFindings(acIds: string[]): void {
    void act(async () => {
      await rpc.request('review.send_findings', { run_id: run.id, ac_ids: acIds });
      tab = 'chat';
    }, `Sent ${acIds.length} finding${acIds.length === 1 ? '' : 's'} to the agent`);
  }

  async function openFile(path: string): Promise<void> {
    tab = 'changes';
    await loadChanges();
    const hit = files?.find((f) => f.path === path || path.endsWith(`/${f.path}`) || f.path.endsWith(`/${path}`));
    if (hit) await selectFile(hit.path);
    else toast.info(`${path} has no changes in this run`);
  }

  function closeRun(): void {
    closeOpen = false;
    void act(async () => {
      await rpc.request('runs.close', { run_id: run.id });
      nav.board();
    }, 'Run closed');
  }

  async function openFix(): Promise<void> {
    await loadComments();
    fixOpen = true;
  }

  const FlagIcon = $derived(flag ? flagIcon(flag.kind) : null);
  const StateIcon = $derived(runIcon(run.state));
</script>

<div class="ws-header">
  <Button variant="ghost" size="icon-sm" aria-label="Back to board" title="Back to board" onclick={() => nav.board()}>
    <ArrowLeft />
  </Button>
  <div class="ws-title"><span class="key">{header.key}</span><h1 title={header.title}>{header.title}</h1></div>
  <div class="meta">
    {#if card}
      <Badge variant="outline" title="Task Status in Lark"><SquareKanban />Lark: {card.status}</Badge>
    {/if}
    <RunStatusBadge state={run.state} label={runLabel(run)} />
    {#if run.branch}
      {@const branch = run.branch}
      <span class="branch-chip">
        <GitBranch class="icon-sm" aria-hidden="true" /><span>{branch}</span>
        <Button variant="ghost" size="icon-xs" aria-label="Copy branch name" onclick={() => copyText(branch, 'Copied branch name')}>
          <Copy />
        </Button>
      </span>
    {/if}
    {#each prs as pr (pr.url)}
      <a class="link mono" href={pr.url} target="_blank" rel="noopener noreferrer" title="Open the PR on GitHub">
        <GitPullRequest class="icon-sm" aria-hidden="true" />{pr.label}
      </a>
    {/each}
    <Button
      variant="ghost"
      size="icon-sm"
      aria-pressed={panelOpen}
      aria-label="Task details"
      title="Task details"
      onclick={() => (panelOpen = !panelOpen)}
    >
      <PanelRight />
    </Button>
  </div>
</div>

<div class="actionbar">
  {#if brainstorming && working}
    <span class="muted">Agent is reading the code to work out what to ask you. It does not change any file.</span>
  {:else if brainstorming && run.state === 'awaiting_dev'}
    <span class="muted">Answer the agent in chat. Continue to plan when the requirements are clear.</span>
  {:else if planning && working}
    <span class="muted">Agent is reading the code and writing a plan. It does not change any file.</span>
  {:else if planning && run.state === 'awaiting_dev'}
    <span class="muted">Plan ready. Read it in the Plan tab, reply in chat to revise it, or approve to start coding.</span>
  {:else if run.state === 'wf_running'}
    <span class="muted">Applying review feedback. Mandor pushes and moves Lark back to Waiting Review when done.</span>
  {:else if reviewing}
    <span class="run" data-tone="working"><LoaderCircle class="spin" aria-hidden="true" /><span class="lbl">Reviewing the AC. Messages you send now are queued.</span></span>
  {:else if working}
    <span class="muted">Agent is working. Messages you send now are queued.</span>
  {:else if run.state === 'awaiting_dev' && nextAction === 'next'}
    <span class="muted">Card turn finished. Reply, or press Next card to commit it and start {nextKey}.</span>
  {:else if run.state === 'awaiting_dev' && nextAction === 'finish'}
    <span class="muted">Last card turn finished. Reply, or finish the card to commit it before Push &amp; PR.</span>
  {:else if run.state === 'awaiting_dev' && nextAction === 'continue'}
    <span class="muted">A card left the group. Continue with {nextKey}, or stop the group.</span>
  {:else if run.state === 'awaiting_dev'}
    <span class="muted">Turn finished. Reply, or push when the changes look right.</span>
  {:else if run.state === 'publishing'}
    <span class="run" data-tone="working"><LoaderCircle class="spin" aria-hidden="true" /><span class="lbl">Committing, pushing, and opening the PR…</span></span>
  {:else if run.state === 'in_review_loop'}
    <span class="muted">The PR is open. Nothing to do until a reviewer moves the task in Lark.</span>
  {:else if run.state === 'wf_awaiting_comment'}
    <span class="muted">Task is in Waiting Fix. No trusted reviewer comment yet.</span>
  {:else if run.state === 'wf_quiet'}
    <span class="run" data-tone="waiting"><StateIcon aria-hidden="true" /><span class="lbl">
        {#if run.quiet_until_ms}
          Quiet period: forwarding in <span class="mono">{quietLeft(run.quiet_until_ms, now)}</span>
        {:else}
          Quiet period: trusted comments are forwarded when it ends.
        {/if}
      </span></span>
  {/if}
  <span class="right">
    {#if runnable && run.phase === 'implement' && !['orphaned', 'stopped', 'failed'].includes(run.state)}
      <RunAppControl
        app={appRun.current}
        disabled={busy || working}
        onrun={() => void openRunApp()}
        onstop={() => act(() => rpc.request('app.stop', { run_id: run.id }), 'App stopped')}
        onlogs={() => void openLogs()}
      />
    {/if}
    {#if reviewing}
      <Button variant="outline" size="sm" disabled={busy} onclick={() => act(() => rpc.request('review.cancel', { run_id: run.id }), 'Review cancelled')}>
        <Square />Stop review
      </Button>
      <Button size="sm" disabled><LoaderCircle class="spin" />Reviewing AC…</Button>
    {:else if working}
      <Button variant="outline" size="sm" disabled={busy} onclick={() => act(() => rpc.request('runs.stop', { run_id: run.id }))}>
        <Square />Stop
      </Button>
    {:else if brainstorming && run.state === 'awaiting_dev'}
      <Button size="sm" disabled={busy} onclick={continueToPlan}><ClipboardList />Continue to plan</Button>
    {:else if planning && run.state === 'awaiting_dev'}
      {#if vmQueue}
        <QueueForVm
          {rpc}
          runId={run.id}
          recordId={run.record_id}
          repoId={run.repo_id}
          workspaceId={run.workspace_id}
          {follow}
          laptopSources={ledger.ledger?.design_sources ?? []}
          disabled={busy || !run.plan}
          ondone={() => {
            follow = [];
            void runs.load();
          }}
        />
      {/if}
      <Button size="sm" disabled={busy || !run.plan} onclick={approvePlan}><ListChecks />Approve plan &amp; implement</Button>
    {:else if nextAction}
      <Button size="sm" disabled={busy} onclick={() => void groupStep()}>
        {#if nextAction === 'next'}<SkipForward />Next card{:else if nextAction === 'finish'}<CircleCheck />Finish card{:else}<Play />Continue with {nextKey}{/if}
      </Button>
    {:else if run.state === 'awaiting_dev'}
      {#if files !== null && files.length === 0}<span class="muted" style="font-size:12px">No changes to push yet</span>{/if}
      <Popover.Root bind:open={blockedOpen}>
        <Popover.Trigger>
          {#snippet child({ props })}
            <Button
              {...props}
              size="sm"
              variant={gateNow.state === 'blocked' ? 'outline' : 'default'}
              disabled={busy || files === null || files.length === 0}
              title={gateNow.state === 'review_needed' ? 'Starts a review first. Push again when it finishes.' : undefined}
              onclick={(e: MouseEvent) => {
                e.preventDefault();
                publish();
              }}
            >
              {#if gateNow.state === 'blocked'}<Lock />{:else if gateNow.state === 'warn'}<TriangleAlert />{:else if gateNow.state === 'needs_confirmation'}<CircleQuestionMark />{:else if gateNow.state === 'review_needed'}<ScanSearch />{:else}<GitPullRequestArrow />{/if}
              {gateButtonLabel(gateNow)}
              {#if gateNow.state === 'review_needed'}<span class="gate-tag">Review needed</span>{/if}
              {#if gateNow.state === 'needs_confirmation'}<span class="gate-tag">Needs confirmation</span>{/if}
              {#if gateNow.state === 'blocked'}<span class="gate-tag" data-tone="failed">{failing.length} AC failed</span>{/if}
            </Button>
          {/snippet}
        </Popover.Trigger>
        <Popover.Content align="end" class="gate-pop w-[360px]">
          <div class="gp-head"><Lock aria-hidden="true" /><b>Push blocked by {failing.length} AC</b></div>
          {#each failing as r (r.ac_id)}
            <div class="gp-row"><span class="mono">{acLabel(r)}</span><span>{r.evidence ?? r.text}</span></div>
          {/each}
          <div class="gp-act">
            <Button variant="ghost" size="sm" onclick={() => ((blockedOpen = false), (tab = 'ac'))}>View in AC review</Button>
            <Button variant="outline" size="sm" onclick={() => ((blockedOpen = false), (gateDialog = 'anyway'))}>Push anyway…</Button>
            <Button size="sm" onclick={() => ((blockedOpen = false), sendFindings(failing.filter((r) => r.selectable).map((r) => r.ac_id)))}>Send to agent</Button>
          </div>
        </Popover.Content>
      </Popover.Root>
    {:else if run.state === 'wf_awaiting_comment'}
      <Button size="sm" disabled={busy} onclick={openFix}><Wrench />Start fix</Button>
    {:else if run.state === 'wf_quiet'}
      <Button variant="secondary" size="sm" disabled={busy} onclick={openFix}><Wrench />Start fix now</Button>
    {:else if run.state === 'failed'}
      <Button size="sm" disabled={busy} onclick={() => act(() => rpc.request('runs.resume', { run_id: run.id, text: '' }), 'Run resumed')}>
        <RotateCcw />Resume
      </Button>
    {:else if run.state === 'orphaned' || run.state === 'stopped'}
      <Button variant="outline" size="sm" disabled={busy} onclick={() => (closeOpen = true)}>Close run</Button>
    {/if}
    <DropdownMenu.Root>
      <DropdownMenu.Trigger>
        {#snippet child({ props })}
          <Button {...props} variant="ghost" size="icon-sm" aria-label="More run actions"><Ellipsis /></Button>
        {/snippet}
      </DropdownMenu.Trigger>
      <DropdownMenu.Content align="end" class="w-52">
        {#if run.branch}
          {@const branch = run.branch}
          <DropdownMenu.Item onSelect={() => copyText(branch, 'Copied branch name')}><Copy />Copy branch name</DropdownMenu.Item>
        {/if}
        <DropdownMenu.Item onSelect={() => copyText(key, `Copied ${key}`)}><Copy />Copy task key</DropdownMenu.Item>
        <DropdownMenu.Separator />
        {#if grouped && !working}
          <DropdownMenu.Item
            variant="destructive"
            onSelect={() => act(() => rpc.request('runs.stop_group', { run_id: run.id }), 'Group stopped. Finished cards were pushed without a PR.')}
            ><OctagonX />Stop group</DropdownMenu.Item
          >
        {/if}
        <DropdownMenu.Item variant="destructive" onSelect={() => (closeOpen = true)}><CircleX />Close run</DropdownMenu.Item>
      </DropdownMenu.Content>
    </DropdownMenu.Root>
  </span>
</div>

{#if flag || run.state === 'failed' || run.state === 'orphaned' || run.note || run.last_error}
  <div class="callout-wrap">
    {#if flag && FlagIcon}
      {@const meta = flagMeta(flag.kind)}
      <Alert.Root variant={meta.tone === 'failed' ? 'destructive' : 'default'}>
        <FlagIcon class={meta.tone === 'warning' ? 'text-status-warning' : ''} />
        <Alert.Title>{meta.label}</Alert.Title>
        {#if flag.detail}<Alert.Description>{flag.detail}</Alert.Description>{/if}
        {#if meta.action === 'retry'}
          <Alert.Action>
            <Button variant="outline" size="sm" onclick={() => act(() => rpc.request('card.retry', { record_id: run.record_id }), `Retried ${key}`)}>
              <RefreshCw />Retry
            </Button>
          </Alert.Action>
        {/if}
      </Alert.Root>
    {:else if run.state === 'failed' || run.state === 'orphaned'}
      <Alert.Root variant={run.state === 'failed' ? 'destructive' : 'default'}>
        <StateIcon class={run.state === 'orphaned' ? 'text-status-warning' : ''} />
        <Alert.Title>{run.state === 'failed' ? 'Run failed' : 'Parked: claim lost'}</Alert.Title>
        {#if run.last_error}<Alert.Description>{run.last_error}</Alert.Description>{/if}
      </Alert.Root>
    {:else if run.last_error}
      <Alert.Root variant="destructive">
        <CircleX />
        <Alert.Title>The last action failed</Alert.Title>
        <Alert.Description>{run.last_error}</Alert.Description>
      </Alert.Root>
    {/if}
    {#if run.note}
      <Alert.Root>
        <Info />
        <Alert.Title>{run.note}</Alert.Title>
      </Alert.Root>
    {/if}
  </div>
{/if}

{#if grouped}
  <GroupProgress
    cards={run.cards}
    busy={busy || working}
    onremove={(id) => act(() => rpc.request('runs.remove_card', { run_id: run.id, record_id: id }), 'Card removed. Move it back to To Do in Lark.')}
  />
{/if}

<Tabs.Root bind:value={tab} class="ws-tabs">
  <Tabs.List variant="line" aria-label="Workspace">
    <Tabs.Trigger value="chat"><MessagesSquare />Chat</Tabs.Trigger>
    {#if planning || run.plan}
      <Tabs.Trigger value="plan"><ClipboardList />Plan</Tabs.Trigger>
    {/if}
    <Tabs.Trigger value="changes">
      <FileDiff />Changes
      {#if files && files.length > 0}<span class="count mono">{files.length}</span>{/if}
    </Tabs.Trigger>
    <Tabs.Trigger value="task"><ListChecks />Task</Tabs.Trigger>
    <Tabs.Trigger value="comments">
      <MessageSquareText />Review comments
      {#if comments && comments.length > 0}<span class="count mono">{comments.length}</span>{/if}
    </Tabs.Trigger>
    <Tabs.Trigger value="ac">
      <ClipboardCheck />AC review
      {#if reviewing}
        <LoaderCircle class="spin icon-sm" aria-hidden="true" /><span class="sr-only">reviewing</span>
      {:else if acCount}
        <span class="count mono">{acCount.text}</span>
        {#if acCount.failed > 0}
          <span class="tab-fail" title="{acCount.failed} failed"><CircleX class="icon-sm" aria-hidden="true" /><span class="sr-only">{acCount.failed} failed</span></span>
        {/if}
      {/if}
    </Tabs.Trigger>
  </Tabs.List>
</Tabs.Root>

<div class="ws-body">
  <div class="ws-main">
    <!-- Chat tetap ter-mount saat pindah tab agar posisi gulir dan draft tidak hilang. -->
    <section class="ws-pane" hidden={tab !== 'chat'} aria-label="Chat">
      <ChatStream {run} />
      <Composer
        {run}
        onsend={(text, now) => act(() => rpc.request(now ? 'runs.send_now' : 'runs.send', { run_id: run.id, text }))}
        onstop={() => act(() => rpc.request('runs.stop', { run_id: run.id }))}
      />
    </section>
    {#if tab === 'plan'}
      <section class="ws-pane scroll-area" aria-label="Plan">
        {#if run.plan}
          {@const plan = run.plan}
          <div class="doc-bar">
            <span class="caps muted">{planning ? 'Plan · waiting for your approval' : 'Approved plan'}</span>
            <Button variant="ghost" size="xs" class="ml-auto" onclick={() => copyText(plan, 'Copied the plan as Markdown')}>
              <Copy />Copy Markdown
            </Button>
          </div>
          {#if planning && run.state === 'awaiting_dev' && ledger.ledger}
            <PlanApproval ledger={ledger.ledger} bind:follow />
          {/if}
          <MarkdownDoc source={plan} />
        {:else}
          <div class="task-doc">
            <p class="muted">The agent is still reading the code. The plan appears here when the turn finishes.</p>
          </div>
        {/if}
      </section>
    {:else if tab === 'changes'}
      <section class="ws-pane" aria-label="Changes">
        <DiffViewer
          {files}
          repoIds={run.workspace_id ? run.repos.map((r) => r.repo_id) : null}
          branch={run.branch}
          {patch}
          selected={selectedFile}
          onselect={selectFile}
          ondiscard={canDiscard ? discardFile : null}
        />
      </section>
    {:else if tab === 'task'}
      <section class="ws-pane scroll-area" aria-label="Task">
        <TaskTab {run} {card} {details} error={detailsError} />
      </section>
    {:else if tab === 'comments'}
      <section class="ws-pane scroll-area" aria-label="Review comments">
        <CommentsTab {run} {comments} error={commentsError} {now} />
      </section>
    {:else if tab === 'ac'}
      <section class="ws-pane scroll-area" aria-label="AC review">
        <AcReviewTab
          taskKey={key}
          store={ledger}
          {reviewing}
          agentBusy={(working && !reviewing) || run.phase !== 'implement' || run.state !== 'awaiting_dev'}
          {now}
          onreview={() => void requestReview('manual')}
          onsend={sendFindings}
          onfile={(p) => void openFile(p)}
          cards={run.cards}
        />
      </section>
    {/if}
  </div>
  {#if panelOpen}
    <aside class="ws-panel scroll-area" aria-label="Task details">
      <div style="display:flex;align-items:center">
        <h2 style="font-size:13px;font-weight:600">Details</h2>
        <Button variant="ghost" size="icon-xs" style="margin-left:auto" aria-label="Close details" onclick={() => (panelOpen = false)}>
          <X />
        </Button>
      </div>
      <TaskDetails {run} {card} {details} withRun />
    </aside>
  {/if}
</div>

{#if fixOpen}
  <StartFixDialog
    taskKey={key}
    recordId={run.record_id}
    comments={comments ?? []}
    oncancel={() => (fixOpen = false)}
    onsubmit={(params) =>
      act(async () => {
        await rpc.request('runs.start_fix', params);
        fixOpen = false;
      }, 'Fix started')}
  />
{/if}

{#if appChanges}
  <RunAppConfirm
    taskKey={key}
    changes={appChanges}
    {envKeys}
    otherRun={ownerKey}
    oncancel={() => (appChanges = null)}
    onconfirm={confirmRunApp}
    onfile={(p) => {
      appChanges = null;
      void openFile(p);
    }}
  />
{/if}

{#if appLogs}
  <AppLogsDrawer logs={appLogs} error={appRun.current?.error ?? null} onclose={() => (appLogs = null)} />
{/if}

{#if readiness}
  {@const r = readiness}
  <ReadinessDialog
    taskKey={key}
    trigger={r.trigger === 'push' ? 'Push & create PR' : 'Run review'}
    readiness={r.view}
    ownerLabel={ownerKey}
    {checking}
    oncancel={() => (readiness = null)}
    onstart={(c) => void startFromReadiness(c, r.trigger)}
    onanyway={() => void beginReview(r.trigger)}
    onrunapp={() => void openRunApp()}
    onrecheck={() => void recheckChrome()}
  />
{/if}

{#if gateDialog}
  <GateDialogs
    mode={gateDialog}
    runId={run.id}
    rows={acRows}
    {syncWarning}
    oncancel={() => (gateDialog = null)}
    onconfirm={(p) => void push(p)}
  />
{/if}

{#if changedOpen && changedList.length > 0}
  <ChangedCardsDialog
    cards={changedList}
    {busy}
    oncancel={() => (changedKeys = [])}
    ondecide={(id, d, reason) => void decideCard(id, d, reason)}
  />
{/if}

{#if closeOpen}
  <CloseRunDialog taskKey={key} branch={run.branch} oncancel={() => (closeOpen = false)} onconfirm={closeRun} />
{/if}
