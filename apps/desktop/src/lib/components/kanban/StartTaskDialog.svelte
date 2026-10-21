<script lang="ts">
  import ArrowLeftRight from '@lucide/svelte/icons/arrow-left-right';
  import ArrowRight from '@lucide/svelte/icons/arrow-right';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import FolderX from '@lucide/svelte/icons/folder-x';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import Play from '@lucide/svelte/icons/play';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import StepProgress, { type Step } from '#lib/components/mandor/StepProgress.svelte';
  import * as Alert from '#lib/components/ui/alert/index.ts';
  import { Badge } from '#lib/components/ui/badge/index.ts';
  import { Button } from '#lib/components/ui/button/index.ts';
  import * as Dialog from '#lib/components/ui/dialog/index.ts';
  import { Label } from '#lib/components/ui/label/index.ts';
  import * as Select from '#lib/components/ui/select/index.ts';
  import * as ToggleGroup from '#lib/components/ui/toggle-group/index.ts';
  import CheckList from '#lib/components/settings/CheckList.svelte';
  import { Textarea } from '#lib/components/ui/textarea/index.ts';
  import type { CardView } from '#lib/protocol/CardView.ts';
  import type { StartProgressView } from '#lib/protocol/StartProgressView.ts';
  import { START_STEPS, TAKEOVER_STEPS, stepsAt } from '#lib/start.ts';
  import type { RepoView } from '#lib/protocol/RepoView.ts';
  import type { SkillView } from '#lib/protocol/SkillView.ts';
  import type { WorkspaceView } from '#lib/protocol/WorkspaceView.ts';
  import type { Rpc } from '#lib/rpc.ts';
  import { DEFAULT_PLANNING, PLANNING_KEY, planningParams, restorePlanning, type PlanningChoice } from '#lib/planning.ts';
  import { startParams } from '#lib/workspaces.ts';
  import PlanningPicker from './PlanningPicker.svelte';

  interface Props {
    rpc: Rpc;
    card: CardView;
    action: 'start' | 'takeover';
    repos: RepoView[];
    workspaces: WorkspaceView[];
    onclose: () => void;
    /** Run berhasil dibuat; `open` = dev memilih membuka workspace. */
    ondone: (runId: string, open: boolean) => void;
    onaddrepo: () => void;
    onrefresh: () => void;
  }
  let { rpc, card, action, repos, workspaces, onclose, ondone, onaddrepo, onrefresh }: Props = $props();

  const key = $derived(card.key ?? card.record_id);
  const defs = $derived(action === 'start' ? START_STEPS : TAKEOVER_STEPS);
  // Langkah aktif terakhir dari event `start.progress` daemon.
  let current = 'claiming';

  // Repo terakhir untuk task ini dipilih lebih dulu; nilai awal saja, dev bisa menggantinya.
  function initialRepo(): string {
    return card.repo_id && repos.some((r) => r.id === card.repo_id) ? card.repo_id : (repos[0]?.id ?? '');
  }
  let repoId = $state(initialRepo());
  let note = $state('');
  let phase = $state<'form' | 'running' | 'done' | 'failed'>('form');
  let steps = $state<Step[]>([]);
  let runId = $state<string | null>(null);

  const repo = $derived(repos.find((r) => r.id === repoId));

  // Target workspace hanya untuk Start; nilai awal workspace yang memuat repo terakhir task ini.
  function initialWorkspace(): WorkspaceView | undefined {
    return workspaces.find((w) => card.repo_id && w.repo_ids.includes(card.repo_id)) ?? workspaces[0];
  }
  const firstWs = initialWorkspace();
  let mode = $state<'repo' | 'workspace'>('repo');
  let wsId = $state(firstWs?.id ?? '');
  let wsRepos = $state<string[]>(firstWs ? [...firstWs.repo_ids] : []);
  const ws = $derived(workspaces.find((w) => w.id === wsId));
  const multi = $derived(action === 'start' && mode === 'workspace');
  // Pilihan planning terakhir diingat per mesin; skill yang tidak terpasang lagi kembali ke bawaan.
  let planning = $state<PlanningChoice>({ ...DEFAULT_PLANNING });
  let skills = $state<SkillView[] | null>(null);
  $effect(() => {
    if (action !== 'start') return;
    rpc
      .request<SkillView[]>('agent.skills')
      .catch(() => [] as SkillView[])
      .then((list) => {
        skills = list;
        planning = restorePlanning(readSaved(), list.map((k) => k.name));
      });
  });

  function readSaved(): unknown {
    try {
      return JSON.parse(localStorage.getItem(PLANNING_KEY) ?? 'null');
    } catch {
      return null;
    }
  }

  function remember(c: PlanningChoice): void {
    try {
      localStorage.setItem(PLANNING_KEY, JSON.stringify(c));
    } catch {
      // Penyimpanan tidak tersedia: pilihan hanya tidak diingat.
    }
  }

  const canGo = $derived(
    (multi ? !!ws && wsRepos.length > 0 : !!repoId) && (action !== 'start' || skills !== null)
  );

  function chooseWorkspace(id: string): void {
    wsId = id;
    wsRepos = [...(workspaces.find((w) => w.id === id)?.repo_ids ?? [])];
  }

  async function go(): Promise<void> {
    phase = 'running';
    current = 'claiming';
    steps = stepsAt(defs, current);
    const off = rpc.on('start.progress', (p) => {
      const ev = p as StartProgressView;
      if (ev.record_id !== card.record_id || phase !== 'running') return;
      current = ev.step;
      steps = stepsAt(defs, current);
    });
    if (action === 'start') remember(planning);
    try {
      const params =
        action === 'start'
          ? startParams({
              recordId: card.record_id,
              note,
              planning: planningParams(planning),
              target:
                multi && ws ? { kind: 'workspace', workspace: ws, repoIds: wsRepos } : { kind: 'repo', repoId }
            })
          : { record_id: card.record_id, repo_id: repoId };
      const run = await rpc.request<{ id: string }>(action === 'start' ? 'runs.start' : 'runs.takeover', params);
      steps = stepsAt(defs, 'done');
      runId = run.id;
      phase = 'done';
    } catch (err) {
      steps = stepsAt(defs, current, err instanceof Error ? err.message : String(err));
      phase = 'failed';
    } finally {
      off();
    }
  }

  function onOpenChange(open: boolean): void {
    // Dialog tidak bisa ditutup saat langkah berjalan.
    if (!open && phase !== 'running') {
      if (phase === 'done' && runId) ondone(runId, false);
      else onclose();
    }
  }
</script>

<Dialog.Root open {onOpenChange}>
  <Dialog.Content class="sm:max-w-[520px]" showCloseButton={phase !== 'running'}>
    <Dialog.Header>
      <div class="flex items-center gap-2">
        <span class="mono muted">{key}</span>
        {#if action === 'start' && card.readiness}
          <Badge variant="outline"><CircleCheck />{card.readiness}</Badge>
        {/if}
      </div>
      <Dialog.Title class="text-[length:var(--text-md)]">
        {action === 'start' ? card.title : `Take over ${key}`}
      </Dialog.Title>
      <Dialog.Description>
        {#if action === 'start'}
          Mandor claims the task, creates a worktree, starts an agent with the title, description, and acceptance
          criteria, and moves Lark to In Progress.
        {:else}
          The claim by {card.owner} is inactive. Taking over moves the claim in Lark to you.
        {/if}
      </Dialog.Description>
    </Dialog.Header>

    {#if phase === 'form'}
      {#if repos.length === 0}
        <Alert.Root>
          <FolderX class="text-status-warning" />
          <Alert.Title>No repo added yet</Alert.Title>
          <Alert.Description>The agent needs a git repo on the daemon machine.</Alert.Description>
          <Alert.Action><Button variant="outline" size="sm" onclick={onaddrepo}>Add a repo</Button></Alert.Action>
        </Alert.Root>
      {:else}
        {#if action === 'start' && workspaces.length > 0}
          <ToggleGroup.Root
            type="single"
            size="sm"
            variant="outline"
            value={mode}
            onValueChange={(v) => v && (mode = v as 'repo' | 'workspace')}
            aria-label="Start in"
          >
            <ToggleGroup.Item value="repo">One repo</ToggleGroup.Item>
            <ToggleGroup.Item value="workspace">Workspace</ToggleGroup.Item>
          </ToggleGroup.Root>
        {/if}
        {#if multi}
          <div class="grid gap-1.5">
            <Label for="start-ws">Workspace</Label>
            <Select.Root type="single" value={wsId} onValueChange={chooseWorkspace}>
              <Select.Trigger id="start-ws" class="w-full">{ws ? `${ws.name} · ${ws.root}` : 'Choose a workspace'}</Select.Trigger>
              <Select.Content>
                {#each workspaces as w (w.id)}
                  <Select.Item value={w.id} label="{w.name} · {w.root}" />
                {/each}
              </Select.Content>
            </Select.Root>
          </div>
          {#if ws}
            <div class="grid gap-1.5">
              <span class="text-sm font-medium">Repos for this task</span>
              <CheckList
                items={ws.repo_ids.map((id) => ({
                  id,
                  label: id,
                  hint: `origin/${repos.find((r) => r.id === id)?.default_branch ?? 'default branch'}`
                }))}
                bind:selected={wsRepos}
                label="Repos for this task"
                empty="This workspace has no repos."
              />
              <p class="muted text-[length:var(--text-sm)]">
                One worktree per checked repo on the same branch. Each changed repo gets its own PR; Lark moves to
                Waiting Review after all PRs are open.
              </p>
            </div>
          {/if}
        {:else}
        <div class="grid gap-1.5">
          <Label for="start-repo">Repo</Label>
          <Select.Root type="single" bind:value={repoId}>
            <Select.Trigger id="start-repo" class="w-full">{repo ? `${repo.id} · ${repo.path}` : 'Choose a repo'}</Select.Trigger>
            <Select.Content>
              {#each repos as r (r.id)}
                <Select.Item value={r.id} label="{r.id} · {r.path}" />
              {/each}
            </Select.Content>
          </Select.Root>
          {#if repo}
            <p class="muted text-[length:var(--text-sm)]">
              Worktree is created from origin/{repo.default_branch ?? 'the default branch'}.
            </p>
          {/if}
        </div>
        {/if}
        {#if action === 'start'}
          <PlanningPicker {skills} bind:choice={planning} />
          <div class="grid gap-1.5">
            <Label for="start-note">Note to the agent <span class="muted font-normal">(optional)</span></Label>
            <Textarea
              id="start-note"
              bind:value={note}
              placeholder="Extra instructions, e.g. reuse the PaymentBadge component from the invoice list"
            />
          </div>
        {:else}
          <Alert.Root>
            <TriangleAlert class="text-status-warning" />
            <Alert.Title>The previous owner's local work is not visible here</Alert.Title>
            <Alert.Description>
              Mandor restores the task branch from origin. Unpushed commits on their machine stay there.
            </Alert.Description>
          </Alert.Root>
        {/if}
      {/if}
    {:else}
      <StepProgress {steps} />
    {/if}

    <Dialog.Footer>
      {#if phase === 'form'}
        <Button variant="ghost" onclick={onclose}>Cancel</Button>
        <Button disabled={!canGo} onclick={go}>
          {#if action === 'start'}<Play />Start{:else}<ArrowLeftRight />Take over{/if}
        </Button>
      {:else if phase === 'running'}
        <Button disabled><LoaderCircle class="spin" />{action === 'start' ? 'Starting' : 'Taking over'}</Button>
      {:else if phase === 'done' && runId}
        {@const id = runId}
        <Button variant="ghost" onclick={() => ondone(id, false)}>Stay on board</Button>
        <Button onclick={() => ondone(id, true)}>Open workspace<ArrowRight /></Button>
      {:else}
        <Button variant="ghost" onclick={onclose}>Close</Button>
        <Button
          variant="secondary"
          onclick={() => {
            onclose();
            onrefresh();
          }}><RefreshCw />Refresh board</Button
        >
      {/if}
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
