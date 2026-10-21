<script lang="ts">
  import CircleQuestionMark from '@lucide/svelte/icons/circle-question-mark';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import OctagonX from '@lucide/svelte/icons/octagon-x';
  import Unplug from '@lucide/svelte/icons/unplug';
  import type { ConnectionState } from '#lib/api/client.ts';
  import StepProgress from '#lib/components/mandor/StepProgress.svelte';
  import TitleBar from './TitleBar.svelte';

  let { state }: { state: Exclude<ConnectionState, { kind: 'ready' }> } = $props();
</script>

<div class="conn-screen">
  <TitleBar />
  <div class="conn-center">
    <div class="conn-card" role="status" aria-live="polite">
      {#if state.kind === 'connecting'}
        <div class="empty" style="padding:0;justify-items:start;text-align:left">
          <span class="ico"><LoaderCircle class="spin" aria-hidden="true" /></span>
          <h1>Connecting to the daemon…</h1>
          <p>Starting <span class="mono">mandord</span>. The first launch after a reboot can take a few seconds on Windows.</p>
        </div>
        <div class="checklist">
          <StepProgress
            steps={[
              { label: 'Starting mandord', s: 'active' },
              { label: 'Checking the protocol version', s: 'todo' },
              { label: 'Loading the board', s: 'todo' }
            ]}
          />
        </div>
      {:else if state.fatal}
        <div class="conn-head">
          <span class="conn-ico"><OctagonX aria-hidden="true" /></span>
          <div>
            <h1>App and daemon versions differ</h1>
            <p>Mandor can't talk to this daemon safely. Update both to the same version, then reopen Mandor.</p>
          </div>
        </div>
        <div class="checklist"><span class="mono">{state.message}</span></div>
      {:else}
        <div class="conn-head">
          <span class="conn-ico"><Unplug aria-hidden="true" /></span>
          <div>
            <h1>Can't reach the daemon</h1>
            <p>Mandor keeps retrying. Agents that were already running inside the daemon are not affected.</p>
          </div>
        </div>
        <div class="checklist">
          <span>Last attempt failed: <span class="mono">{state.message}</span></span>
        </div>
        <div class="checklist">
          <b>What to check</b>
          <ul>
            <li>
              <CircleQuestionMark class="icon-sm" aria-hidden="true" />
              <span>Is mandord installed? Run <code>mandord --version</code> (in your WSL terminal on Windows).</span>
            </li>
            <li>
              <CircleQuestionMark class="icon-sm" aria-hidden="true" />
              <span>Is <code>MANDOR_LINK</code> pointing at the right binary or WSL distro?</span>
            </li>
            <li>
              <CircleQuestionMark class="icon-sm" aria-hidden="true" />
              <span>The daemon log is <code>~/.mandor/mandord.log</code>.</span>
            </li>
          </ul>
        </div>
      {/if}
    </div>
  </div>
</div>
