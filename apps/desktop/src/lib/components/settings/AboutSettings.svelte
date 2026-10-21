<script lang="ts">
  import type { Rpc } from '#lib/rpc.ts';
  import PrereqList from './PrereqList.svelte';
  import { connection } from '#lib/stores/connection.svelte.ts';

  let { rpc }: { rpc: Rpc } = $props();

  const daemon = $derived(connection.state.kind === 'ready' ? connection.state.daemonVersion : 'not connected');
</script>

<section>
  <h1>About</h1>
  <p class="lede">Mandor runs Claude Code agents on your Lark Sprint Tasks.</p>
</section>

<section>
  <div class="settings-card">
    <div class="content">
      <dl class="versions">
        <dt class="muted">Mandor app</dt>
        <dd class="mono" style="margin:0">{__APP_VERSION__}</dd>
        <dt class="muted">mandord daemon</dt>
        <dd class="mono" style="margin:0">{daemon}</dd>
      </dl>
    </div>
  </div>
</section>

<section>
  <h2>System check</h2>
  <p class="lede">Mandor needs git, the GitHub CLI (logged in), and Claude Code on the machine that runs mandord.</p>
  <div style="margin-top:12px"><PrereqList {rpc} /></div>
</section>
