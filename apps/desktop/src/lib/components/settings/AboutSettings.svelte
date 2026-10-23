<script lang="ts">
  import { Button } from '#lib/components/ui/button/index.ts';
  import type { ChromeCheckView } from '#lib/protocol/ChromeCheckView.ts';
  import type { Rpc } from '#lib/rpc.ts';
  import PrereqList from './PrereqList.svelte';
  import { connection } from '#lib/stores/connection.svelte.ts';

  let { rpc }: { rpc: Rpc } = $props();

  const daemon = $derived(connection.state.kind === 'ready' ? connection.state.daemonVersion : 'not connected');

  let chrome = $state<ChromeCheckView | null>(null);
  let checking = $state(false);

  async function checkChrome(): Promise<void> {
    checking = true;
    try {
      chrome = await rpc.request<ChromeCheckView>('system.chrome_check', { force: true });
    } catch (err) {
      chrome = { connected: false, detail: err instanceof Error ? err.message : String(err), checked_at_ms: Date.now() };
    } finally {
      checking = false;
    }
  }
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

<section>
  <h2>Claude in Chrome</h2>
  <p class="lede">
    AC reviews test UI criteria in your Chrome through the Claude in Chrome extension. The check runs a short Claude
    session and costs a few cents.
  </p>
  <div class="settings-card" style="margin-top:12px">
    <div class="content flex items-center gap-3">
      {#if chrome}
        <span class="chip" data-tone={chrome.connected ? 'ok' : 'failed'}>
          {chrome.connected ? 'Connected' : 'Not connected'}
        </span>
        {#if !chrome.connected && chrome.detail}<span class="muted">{chrome.detail}</span>{/if}
      {:else}
        <span class="muted">Not checked yet.</span>
      {/if}
      <Button variant="outline" size="sm" class="ml-auto" disabled={checking} onclick={checkChrome}>
        {checking ? 'Checking…' : 'Check connection'}
      </Button>
    </div>
  </div>
</section>
