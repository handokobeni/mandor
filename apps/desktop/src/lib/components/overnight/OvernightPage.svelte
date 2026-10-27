<script lang="ts">
  import { toast } from 'svelte-sonner';
  import { Button } from '#lib/components/ui/button/index.ts';
  import { Input } from '#lib/components/ui/input/index.ts';
  import { canActivate, loginNotices } from '#lib/overnight.ts';
  import type { OvernightSettingsView } from '#lib/protocol/OvernightSettingsView.ts';
  import type { Rpc } from '#lib/rpc.ts';
  import { vmConnection } from '#lib/stores/connection.svelte.ts';
  import { nav } from '#lib/stores/nav.svelte.ts';
  import { overnight } from '#lib/stores/overnight.svelte.ts';
  import { vm } from '#lib/stores/vm.svelte.ts';
  import QueueSection from './QueueSection.svelte';
  import ReportSection from './ReportSection.svelte';

  let { laptop, laptopDaemonId }: { laptop: Rpc; laptopDaemonId: string | null } = $props();

  const link = $derived(vmConnection.state);
  const logins = $derived(loginNotices(overnight.logins));

  let form = $state<OvernightSettingsView>({ start: '19:00', end: '06:00', offset: '+07:00', token_limit: null });
  let limitText = $state('');
  let formFor = $state<string | null>(null);
  // Formulir diisi ulang hanya saat setelan dari VM berubah.
  $effect(() => {
    const s = overnight.settings;
    const key = JSON.stringify(s);
    if (key !== formFor) {
      formFor = key;
      if (s) {
        form = { ...s };
        limitText = s.token_limit === null ? '' : String(s.token_limit);
      }
    }
  });

  async function saveSettings(): Promise<void> {
    const raw = limitText.trim();
    const limit = raw === '' ? null : Number(raw.replaceAll(',', ''));
    if (limit !== null && (!Number.isInteger(limit) || limit < 0)) {
      toast.error('The usage limit must be a whole number of tokens');
      return;
    }
    try {
      await overnight.saveSettings({ ...form, token_limit: limit });
      toast.success('Overnight settings saved');
    } catch (err) {
      toast.error('Could not save the settings', { description: err instanceof Error ? err.message : String(err) });
    }
  }
</script>

<div class="overnight settings-main scroll-area">
  <section>
    <h1>Overnight</h1>
    <p class="lede">Cards your VM works on while you are away, and what it left for you to decide.</p>
  </section>

  {#if !vm.host}
    <section>
      <div class="settings-card">
        <div class="content flex items-center gap-3">
          <span>No VM is connected to this app.</span>
          <Button size="sm" onclick={() => nav.settings('vm')}>Add your VM</Button>
        </div>
      </div>
    </section>
  {:else if link.kind !== 'ready'}
    <section>
      <div class="settings-card">
        <div class="content">
          <p style="margin:0 0 6px">
            <span class="chip" data-tone={link.kind === 'error' ? 'failed' : 'muted'}>
              {link.kind === 'error' ? 'VM offline' : 'Connecting'}
            </span>
            {vm.host}
          </p>
          {#if link.kind === 'error'}
            <p class="field-help">{link.message}</p>
            <p class="field-help">
              Check that the VM is on, that you can run <code>ssh {vm.host}</code> from this machine, and that the
              Mandor service is running there (<code>systemctl --user status mandord</code>).
            </p>
          {/if}
        </div>
      </div>
    </section>
  {:else if !overnight.statusLoaded}
    <section><p class="field-help">Loading…</p></section>
  {:else}
    {#if overnight.error}
      <section>
        <div class="settings-card"><div class="content"><span class="chip" data-tone="failed" style="justify-self:start">Error</span> {overnight.error}</div></div>
      </section>
    {/if}

    {#if !overnight.vmRole}
      <section>
        <div class="settings-card">
          <div class="content">
            <p class="line"><span class="chip" data-tone="warning">Laptop mode</span> The daemon on the VM is not running as a VM daemon yet, so it can't run overnight work.</p>
            <p class="field-help">
              Add the <code>vm.conf</code> drop-in from the VM runbook (section 5), then on the VM run
              <code>systemctl --user daemon-reload &amp;&amp; systemctl --user restart mandord</code>.
            </p>
          </div>
        </div>
      </section>
    {:else}
      <section>
        <h2>Logins on the VM</h2>
        <div class="settings-card">
          <div class="content">
            {#each logins as l (l.label)}
              <div class="row-setting">
                <div class="txt">
                  {l.label}
                  {#if l.fix}<span>{l.fix}</span>{/if}
                </div>
                <span class="chip" data-tone={l.tone}>
                  {l.state === 'ok' ? 'Logged in' : l.state === 'expired' ? 'Expired' : 'Not logged in'}
                </span>
              </div>
            {/each}
          </div>
        </div>
      </section>
    {/if}

    {#if !overnight.larkReady}
      <section>
        <div class="settings-card">
          <div class="content">
            <p class="line"><span class="chip" data-tone="warning">Not logged in to Lark</span> The queue, hours, and report appear after the VM daemon logs in to Lark.</p>
            <div><Button size="sm" onclick={() => nav.settings('lark', 'vm')}>Set up Lark on the VM</Button></div>
          </div>
        </div>
      </section>
    {:else}
    <section>
      <h2>Hours and limit</h2>
      <div class="settings-card">
        <div class="content">
          {#if !canActivate(overnight.settings)}
            <p style="margin:0 0 8px">
              <span class="chip" data-tone="warning">Not active</span>
              The VM does not start any card until you set a usage limit per cycle.
            </p>
          {/if}
          <div class="grid">
            <label class="field"><span class="text-sm font-medium">Starts</span><Input type="time" bind:value={form.start} /></label>
            <label class="field"><span class="text-sm font-medium">Ends</span><Input type="time" bind:value={form.end} /></label>
            <label class="field">
              <span class="text-sm font-medium">UTC offset</span><Input class="mono" placeholder="+07:00" bind:value={form.offset} />
            </label>
            <label class="field">
              <span class="text-sm font-medium">Usage limit per cycle (tokens)</span>
              <Input class="mono" inputmode="numeric" placeholder="e.g. 2000000" bind:value={limitText} />
            </label>
          </div>
          <p class="field-help">
            A cycle runs from one start time to the next. Start now and Waiting Fix runs during the day count toward the
            same limit.
          </p>
          <Button size="sm" onclick={saveSettings}>Save</Button>
        </div>
      </div>
    </section>

    <QueueSection />
    <ReportSection {laptop} {laptopDaemonId} />
    {/if}
  {/if}
</div>

<style>
  .overnight {
    height: 100%;
  }
  h2 {
    font-size: 14px;
    font-weight: 600;
    margin: 0 0 8px;
  }
  section {
    margin-bottom: 20px;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin: 0;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 12px;
    margin-bottom: 8px;
  }
</style>
