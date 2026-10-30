<script lang="ts">
  import { Button } from '#lib/components/ui/button/index.ts';
  import { Input } from '#lib/components/ui/input/index.ts';
  import { connection, vmConnection } from '#lib/stores/connection.svelte.ts';
  import { overnight } from '#lib/stores/overnight.svelte.ts';
  import DesignSourceSettings from './DesignSourceSettings.svelte';
  import { vm } from '#lib/stores/vm.svelte.ts';
  import { supportsVmLink } from '#lib/vm.ts';

  let host = $state('');
  let confirmRemove = $state(false);

  const laptopProtocol = $derived(connection.state.kind === 'ready' ? connection.state.protocol : undefined);
  const supported = $derived(supportsVmLink(laptopProtocol));
  const linkState = $derived(vmConnection.state);
</script>

<section>
  <h1>Overnight VM</h1>
  <p class="lede">
    Connect this app to your Mandor daemon on the shared VM over SSH, next to the daemon on this laptop. The VM
    works through its queue overnight.
  </p>
</section>

{#if !supported}
  <section>
    <div class="settings-card">
      <div class="content">
        <span class="chip" data-tone="warning">Update needed</span>
        The daemon on this laptop is too old to share cards with a VM daemon safely. Update Mandor on this laptop first.
      </div>
    </div>
  </section>
{:else if vm.host}
  <section>
    <div class="settings-card">
      <div class="content">
        <div class="row-setting">
          <div class="txt">
            {vm.host}
            <span>
              {#if linkState.kind === 'ready'}
                Connected · daemon {linkState.daemonVersion}
              {:else if linkState.kind === 'error'}
                {linkState.message}
              {:else}
                Connecting…
              {/if}
            </span>
          </div>
          {#if linkState.kind === 'ready'}
            <span class="chip" data-tone="ok">Connected</span>
          {:else if linkState.kind === 'error'}
            <span class="chip" data-tone="failed">Not connected</span>
          {:else}
            <span class="chip" data-tone="muted">Connecting</span>
          {/if}
          {#if confirmRemove}
            <Button size="sm" variant="destructive" disabled={vm.busy} onclick={() => vm.remove()}>Remove</Button>
            <Button size="sm" variant="outline" disabled={vm.busy} onclick={() => (confirmRemove = false)}>Cancel</Button>
          {:else}
            <Button size="sm" variant="outline" onclick={() => (confirmRemove = true)}>Remove VM…</Button>
          {/if}
        </div>
        {#if linkState.kind === 'error'}
          <p class="field-help">
            If the VM's host key changed, remove the VM and add it again after your admin confirms the new key.
          </p>
        {/if}
      </div>
    </div>
  </section>
{:else if vm.scan}
  <section>
    <div class="settings-card">
      <div class="content">
        <p style="margin:0 0 8px">
          Check that this host key matches the one your admin gave you for <strong>{vm.scan.host}</strong>:
        </p>
        <ul class="fingerprints">
          {#each vm.scan.fingerprints as fp (fp)}
            <li><code>{fp}</code></li>
          {/each}
        </ul>
        <div class="flex gap-2" style="margin-top:8px">
          <Button size="sm" disabled={vm.busy} onclick={() => vm.trust()}>Trust and connect</Button>
          <Button size="sm" variant="outline" disabled={vm.busy} onclick={() => vm.cancel()}>Cancel</Button>
        </div>
      </div>
    </div>
  </section>
{:else}
  <section>
    <div class="settings-card">
      <div class="content">
        <div class="field">
          <label class="text-sm font-medium" for="vm-host">SSH host</label>
          <p class="field-help">
            An alias from your ~/.ssh/config or user@host. Your SSH key and config are used; the VM's host key is pinned
            after you confirm it.
          </p>
          <div class="flex gap-2">
            <Input id="vm-host" placeholder="beni@overnight-vm" bind:value={host} />
            <Button disabled={vm.busy || host.trim() === ''} onclick={() => vm.check(host.trim())}>Check host key</Button>
          </div>
        </div>
      </div>
    </div>
  </section>
{/if}

{#if supported && vm.host}
  {#if linkState.kind === 'ready' && vmConnection.client}
    <DesignSourceSettings rpc={vmConnection.client} vm onchange={() => void overnight.refresh()} />
  {:else}
    <section>
      <h2>Design sources on the VM</h2>
      <div class="settings-card">
        <div class="content muted">VM not connected. Its design sources show here once the VM link is ready.</div>
      </div>
    </section>
  {/if}
{/if}

{#if vm.error}
  <section>
    <div class="settings-card">
      <div class="content"><span class="chip" data-tone="failed">Error</span> {vm.error}</div>
    </div>
  </section>
{/if}

<style>
  .fingerprints {
    margin: 0;
    padding-left: 18px;
  }
  .fingerprints code {
    font-size: 12px;
    word-break: break-all;
  }
</style>
