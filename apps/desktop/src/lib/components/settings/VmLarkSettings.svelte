<script lang="ts">
  import { vmLark } from '#lib/stores/lark.svelte.ts';
  import LarkAppForm from './LarkAppForm.svelte';

  const status = $derived(vmLark.status);
  const configured = $derived(!!status?.configured);
  const port = $derived(vmLark.settings?.redirect_port ?? null);
</script>

<section>
  <h1>Lark on the VM</h1>
  <p class="lede">
    The VM daemon needs the same Lark App and Base as this laptop. Use a login callback port in your VM port range and
    register it in the Lark app.
  </p>
</section>

<section>
  <div class="settings-card">
    <div class="content">
      {#if status?.logged_in && !status.reauth_required}
        <p class="status"><span class="chip" data-tone="ok">Logged in</span> {status.name ?? status.open_id}</p>
      {:else}
        <p class="status">
          <span class="chip" data-tone="warning">{status?.reauth_required ? 'Login expired' : 'Not logged in'}</span>
        </p>
        <ol class="steps">
          <li class:muted={configured}>
            Fill in the Custom App below with a <b>callback port in your VM port range</b> (for example 5199), and save.
          </li>
          <li class:muted={!configured}>
            On this laptop, open a terminal and connect to the VM with the callback port forwarded:
            <code class="cmd">ssh -L {port ?? '<port>'}:127.0.0.1:{port ?? '<port>'} &lt;vm&gt;</code>
          </li>
          <li class:muted={!configured}>
            In that VM session, run <code>mandord login lark</code> and open the URL it prints in your browser.
          </li>
        </ol>
      {/if}
    </div>
  </div>
</section>

<section><LarkAppForm store={vmLark} /></section>

<style>
  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
  }
  .steps {
    margin: 0;
    padding-left: 20px;
    display: grid;
    gap: 8px;
    line-height: 1.5;
  }
  .steps .muted {
    color: var(--muted-foreground);
  }
  .cmd {
    display: block;
    margin-top: 4px;
    width: fit-content;
  }
</style>
