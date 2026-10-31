<script lang="ts">
  // "Approve & queue for VM" (plan 2026-10-30-001, R1, R2, R11): plan yang
  // disetujui dikerjakan VM malam hari. Tiga langkah: push di laptop, antre di
  // VM, klaim pindah ke VM.
  import { toast } from 'svelte-sonner';
  import MoonStar from '@lucide/svelte/icons/moon-star';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import CheckList from '#lib/components/settings/CheckList.svelte';
  import { Button } from '#lib/components/ui/button/index.ts';
  import * as Popover from '#lib/components/ui/popover/index.ts';
  import * as Select from '#lib/components/ui/select/index.ts';
  import { defaultVmTarget, queueForVmBlocker, queueStepLabel, vmTargetRepos } from '#lib/overnight.ts';
  import type { Rpc } from '#lib/rpc.ts';
  import { background } from '#lib/stores/background.svelte.ts';
  import { nav } from '#lib/stores/nav.svelte.ts';
  import { overnight, type QueueStep, type QueueTarget } from '#lib/stores/overnight.svelte.ts';

  interface Props {
    rpc: Rpc;
    runId: string;
    recordId: string;
    /** Target run laptop: repo, atau workspace dengan anggotanya. */
    repoIds: string[];
    workspaceId: string | null;
    follow: string[];
    /** Nama sumber desain laptop yang dipakai saat plan disusun. */
    laptopSources: string[];
    disabled: boolean;
    ondone: () => void;
  }
  let { rpc, runId, recordId, repoIds, workspaceId, follow, laptopSources, disabled, ondone }: Props = $props();

  let open = $state(false);
  let target = $state('');
  let design = $state<string[]>([]);
  let step = $state<QueueStep | null>(null);

  // Target VM bawaan: repo yang sama, atau workspace VM yang memuat semua
  // repo run workspace laptop.
  $effect(() => {
    if (!open || target) return;
    target = defaultVmTarget(repoIds, workspaceId, overnight);
  });

  const targetRepos = $derived(vmTargetRepos(target, repoIds, overnight.workspaces));
  const blocker = $derived(
    queueForVmBlocker({ target: target || null, follow, designSources: design, laptopRepos: repoIds, targetRepos })
  );
  const targetText = $derived.by(() => {
    if (target.startsWith('repo:')) return target.slice(5);
    const ws = overnight.workspaces.find((w) => `ws:${w.id}` === target);
    return ws ? `Workspace ${ws.name}` : 'Repo on the VM…';
  });

  function targetOf(value: string): QueueTarget | null {
    if (value.startsWith('repo:')) return { repo_id: value.slice(5) };
    const ws = overnight.workspaces.find((w) => `ws:${w.id}` === value);
    return ws ? { workspace: { id: ws.id, repo_ids: vmTargetRepos(value, repoIds, overnight.workspaces) } } : null;
  }

  async function queue(): Promise<void> {
    const t = targetOf(target);
    const laptopDaemonId = background.status?.daemon_id ?? null;
    if (!t) return;
    if (!laptopDaemonId) {
      toast.error('Update Mandor on this laptop to queue cards for the VM');
      return;
    }
    try {
      await overnight.queuePlanned(
        rpc,
        { runId, recordId, follow, target: t, designSources: design, laptopDaemonId },
        (s) => (step = s)
      );
      open = false;
      toast.success('Queued for the VM', { description: 'The VM implements the approved plan in the overnight window.' });
      ondone();
    } catch (err) {
      const description = err instanceof Error ? err.message : String(err);
      if (step === 'handing_over') {
        open = false;
        toast.error('The card is queued but the claim did not move', {
          description: `${description}. Retry handing over in the Overnight tab.`
        });
        ondone();
      } else {
        toast.error('Could not queue for the VM', { description });
      }
    } finally {
      step = null;
    }
  }
</script>

<Popover.Root bind:open>
  <Popover.Trigger {disabled}>
    {#snippet child({ props })}
      <Button {...props} variant="outline" size="sm" title="Approve the plan and let the VM implement it overnight"><MoonStar />Approve &amp; queue for VM</Button>
    {/snippet}
  </Popover.Trigger>
  <Popover.Content class="w-80" align="end">
    {#if step}
      <p class="step" role="status"><LoaderCircle class="spin icon-sm" aria-hidden="true" />{queueStepLabel(step)}</p>
    {:else}
      <div class="grid gap-3">
        <div class="grid gap-1.5">
          <span class="text-sm font-medium">Repo on the VM</span>
          <Select.Root type="single" bind:value={target}>
            <Select.Trigger class="w-full" aria-label="Repo on the VM">
              <span class="min-w-0 truncate">{targetText}</span>
            </Select.Trigger>
            <Select.Content>
              {#each overnight.repos as r (r.id)}
                <Select.Item value={`repo:${r.id}`} label={r.id} />
              {/each}
              {#each overnight.workspaces as w (w.id)}
                <Select.Item value={`ws:${w.id}`} label={`Workspace ${w.name}`} />
              {/each}
            </Select.Content>
          </Select.Root>
        </div>
        <div class="grid gap-1.5">
          <span class="text-sm font-medium">Design sources on the VM</span>
          {#if overnight.designSources.length === 0}
            <p class="muted text-[length:var(--text-sm)]">
              No design sources on the VM.
              <button class="link" type="button" onclick={() => nav.settings('vm')}>Add one in Settings › VM</button>
            </p>
          {:else}
            <CheckList
              items={overnight.designSources.map((s) => ({
                id: s.id,
                label: s.name,
                hint: s.ready ? s.path : `${s.status} · ${s.path}`,
                disabled: !s.ready
              }))}
              bind:selected={design}
              label="Design sources on the VM"
              empty=""
            />
          {/if}
          {#if laptopSources.length > 0}
            <p class="field-help">On this laptop the plan used: <b>{laptopSources.join(', ')}</b></p>
          {/if}
        </div>
        {#if blocker && target}
          <p class="field-help warn"><TriangleAlert class="icon-sm" aria-hidden="true" />{blocker}</p>
        {/if}
        <div class="flex justify-end gap-2">
          <Button variant="ghost" size="sm" onclick={() => (open = false)}>Cancel</Button>
          <Button size="sm" disabled={blocker !== null} title={blocker ?? undefined} onclick={queue}><MoonStar />Queue</Button>
        </div>
      </div>
    {/if}
  </Popover.Content>
</Popover.Root>

<style>
  .step {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
  }
  .warn {
    display: flex;
    gap: 4px;
    align-items: center;
    color: var(--status-warning);
  }
</style>
