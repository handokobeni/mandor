<script lang="ts">
  import { toast } from 'svelte-sonner';
  import { Button } from '#lib/components/ui/button/index.ts';
  import { backgroundView, type Row, type Tone } from '#lib/background.ts';
  import { background } from '#lib/stores/background.svelte.ts';

  const CHIP: Record<Tone, string> = { ok: 'ok', warn: 'warning', error: 'failed', unknown: 'muted' };
  const LABEL: Record<Tone, string> = { ok: 'Ready', warn: 'Needs attention', error: 'Failing', unknown: 'Unknown' };

  const view = $derived(
    backgroundView({
      wsl: background.wsl,
      provision: background.provision,
      status: background.status,
      keepAlive: background.keepAlive
    })
  );
  const busy = $derived(background.status?.busy === true);
  let saving = $state(false);
  let restartHint = $state(false);

  $effect(() => {
    if (background.wsl) void background.loadKeepAlive();
  });

  async function act(row: Row): Promise<void> {
    if (!row.action) return;
    saving = true;
    const enable = row.action === 'enable_keep_alive';
    await background.setKeepAlive(enable);
    saving = false;
    if (background.keepAliveError) {
      toast.error('Could not update .wslconfig', { description: background.keepAliveError });
      return;
    }
    restartHint = true;
  }
</script>

<section>
  <h1>Background work</h1>
  <p class="lede">
    What lets agents keep working when you close or quit Mandor and leave the laptop plugged in.
  </p>
</section>

{#if view.notices.length > 0}
  <section>
    {#each view.notices as n (n.id)}
      <div class="settings-card" style="margin-bottom:8px">
        <div class="content flex items-center gap-3">
          <span class="chip" data-tone={CHIP[n.tone]}>{LABEL[n.tone]}</span>
          <span>{n.text}</span>
        </div>
      </div>
    {/each}
  </section>
{/if}

<section>
  <div class="settings-card">
    <div class="content">
      <p style="margin:0 0 4px">
        {#if view.ready}
          <span class="chip" data-tone="ok">Ready for overnight work</span>
        {:else}
          <span class="chip" data-tone="warning">Not ready for overnight work</span>
        {/if}
      </p>
      {#each view.rows as row (row.id)}
        <div class="row-setting">
          <div class="txt">
            {row.title}
            <span>{row.detail}</span>
          </div>
          <span class="chip" data-tone={CHIP[row.tone]}>{LABEL[row.tone]}</span>
          {#if row.action === 'enable_keep_alive'}
            <Button size="sm" disabled={saving} onclick={() => act(row)}>Keep WSL on</Button>
          {:else if row.action === 'disable_keep_alive'}
            <Button size="sm" variant="outline" disabled={saving} onclick={() => act(row)}>Turn off</Button>
          {/if}
        </div>
      {/each}
    </div>
  </div>
</section>

{#if restartHint}
  <section>
    <div class="settings-card">
      <div class="content">
        {#if busy}
          <strong>Restart WSL later.</strong>
          The change applies after "wsl --shutdown", which stops the agents that are working now. Wait until they finish.
        {:else}
          <strong>Restart WSL to apply.</strong>
          Run "wsl --shutdown" in PowerShell, then reopen Mandor.
        {/if}
      </div>
    </div>
  </section>
{/if}
