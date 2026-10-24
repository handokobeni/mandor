<script lang="ts">
  import { vmLark } from '#lib/stores/lark.svelte.ts';
  import LarkAppForm from './LarkAppForm.svelte';

  const status = $derived(vmLark.status);
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
    <div class="content flex flex-wrap items-center gap-2">
      {#if status?.logged_in && !status.reauth_required}
        <span class="chip" data-tone="ok">Logged in</span> {status.name ?? status.open_id}
      {:else}
        <span class="chip" data-tone="warning">{status?.reauth_required ? 'Login expired' : 'Not logged in'}</span>
        Log in from a terminal on the VM: <code>mandord login lark</code>, with the callback port forwarded from this laptop
        (<code>ssh -L &lt;port&gt;:127.0.0.1:&lt;port&gt; &lt;vm&gt;</code>).
      {/if}
    </div>
  </div>
</section>

<section><LarkAppForm store={vmLark} /></section>
