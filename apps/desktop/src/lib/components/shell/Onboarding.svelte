<script lang="ts">
  import ArrowRight from '@lucide/svelte/icons/arrow-right';
  import Check from '@lucide/svelte/icons/check';
  import PrereqList from '#lib/components/settings/PrereqList.svelte';
  import LarkAppForm from '#lib/components/settings/LarkAppForm.svelte';
  import LarkLoginStatus from '#lib/components/settings/LarkLoginStatus.svelte';
  import RepoSettings from '#lib/components/settings/RepoSettings.svelte';
  import { Button } from '#lib/components/ui/button/index.ts';
  import type { Rpc } from '#lib/rpc.ts';
  import type { OnboardingStep } from '#lib/settings.ts';
  import { lark } from '#lib/stores/lark.svelte.ts';

  interface Props {
    rpc: Rpc;
    start: OnboardingStep;
    /** Selesai atau dilewati; papan ditampilkan. */
    onfinish: () => void;
  }
  let { rpc, start, onfinish }: Props = $props();

  const STEPS: { id: OnboardingStep; label: string }[] = [
    { id: 'welcome', label: 'Welcome' },
    { id: 'prereq', label: 'Prerequisites' },
    { id: 'lark', label: 'Lark app' },
    { id: 'login', label: 'Log in' },
    { id: 'repo', label: 'Repo' },
    { id: 'done', label: 'Done' }
  ];

  // Langkah awal hanya nilai pembuka; setelah itu dev yang menentukan.
  function initial(): OnboardingStep {
    return start;
  }
  let step = $state<OnboardingStep>(initial());
  const index = $derived(STEPS.findIndex((s) => s.id === step));

  const canNext = $derived(
    step === 'lark'
      ? !!lark.status?.configured
      : step === 'login'
        ? !!lark.status?.logged_in
        : step === 'repo'
          ? lark.repos.length > 0
          : true
  );

  function next(): void {
    step = STEPS[Math.min(STEPS.length - 1, index + 1)].id;
  }
</script>

<div class="onboarding scroll-area">
  <div class="onboarding-inner">
    <ol class="stepper" aria-label="Setup steps">
      {#each STEPS as s, i (s.id)}
        {#if i > 0}<li class="sep" aria-hidden="true"></li>{/if}
        <li data-s={i < index ? 'done' : undefined} aria-current={i === index ? 'step' : undefined}>
          <span class="num">{#if i < index}<Check class="icon-sm" />{:else}{i + 1}{/if}</span>{s.label}
        </li>
      {/each}
    </ol>

    {#if step === 'welcome'}
      <section>
        <h1>Run several agents without losing track of them</h1>
        <p class="lede">
          Mandor watches the Claude Code agents working on your Lark Sprint Tasks, one worktree each, and keeps Lark in
          sync. Setup takes a few minutes.
        </p>
      </section>
    {:else if step === 'prereq'}
      <section>
        <h1>Check what Mandor needs</h1>
        <p class="lede">The daemon checks these on the machine where agents run (inside WSL on Windows).</p>
        <div style="margin-top:14px"><PrereqList {rpc} /></div>
      </section>
    {:else if step === 'lark'}
      <section>
        <h1>Connect your Lark app</h1>
        <p class="lede">Values from the Lark Developer Console and from the Sprint Tasks URL.</p>
        <div style="margin-top:14px"><LarkAppForm onsaved={next} /></div>
      </section>
    {:else if step === 'login'}
      <section>
        <h1>Log in with Lark</h1>
        <p class="lede">
          Mandor opens Lark in your browser. Status changes are written as you, so reviewers see who moved a task.
        </p>
        <div style="margin-top:14px"><LarkLoginStatus /></div>
      </section>
    {:else if step === 'repo'}
      <section>
        <p class="lede">A git clone on the daemon machine. Agents create their worktrees next to it.</p>
      </section>
      <RepoSettings {rpc} />
    {:else}
      <section>
        <h1>Mandor is ready</h1>
        <p class="lede">Your Lark tasks are loading on the board. Press Start on a ready task to launch the first agent.</p>
      </section>
    {/if}

    <div class="onboarding-foot">
      {#if step !== 'done'}
        <Button variant="ghost" onclick={onfinish}>Skip setup</Button>
      {/if}
      <span style="flex:1"></span>
      {#if index > 0 && step !== 'done'}
        <Button variant="ghost" onclick={() => (step = STEPS[index - 1].id)}>Back</Button>
      {/if}
      {#if step === 'done'}
        <Button onclick={onfinish}>Open the board<ArrowRight /></Button>
      {:else}
        <Button disabled={!canNext} onclick={next}>{step === 'welcome' ? 'Get started' : 'Continue'}<ArrowRight /></Button>
      {/if}
    </div>
  </div>
</div>
