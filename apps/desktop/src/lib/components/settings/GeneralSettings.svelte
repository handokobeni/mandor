<script lang="ts">
  import Monitor from '@lucide/svelte/icons/monitor';
  import Moon from '@lucide/svelte/icons/moon';
  import Sun from '@lucide/svelte/icons/sun';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import { toast } from 'svelte-sonner';
  import { Switch } from '#lib/components/ui/switch/index.ts';
  import { NOTIFY_KINDS, validateQuiet } from '#lib/settings.ts';
  import { lark } from '#lib/stores/lark.svelte.ts';
  import { notifyPrefs } from '#lib/stores/notify-prefs.svelte.ts';
  import { type ThemePreference, theme } from '#lib/stores/theme.svelte.ts';

  const OPTIONS: { value: ThemePreference; label: string; icon: typeof Sun }[] = [
    { value: 'system', label: 'System', icon: Monitor },
    { value: 'dark', label: 'Dark', icon: Moon },
    { value: 'light', label: 'Light', icon: Sun }
  ];

  // Jeda tenang disimpan daemon (berlaku untuk mesin daemon, bukan hanya komputer ini).
  const savedQuiet = $derived(lark.settings?.quiet_secs ?? null);
  let quietDraft = $state<string | null>(null);
  const quiet = $derived(quietDraft ?? (savedQuiet === null ? '' : String(savedQuiet)));
  const quietError = $derived(quietDraft === null ? null : validateQuiet(quietDraft));
  const quietDirty = $derived(quietDraft !== null && quietDraft.trim() !== String(savedQuiet));
  let savingQuiet = $state(false);

  async function saveQuiet(): Promise<void> {
    if (quietDraft === null || quietError || !quietDirty) return;
    savingQuiet = true;
    try {
      await lark.setQuiet(Number(quietDraft.trim()));
      quietDraft = null;
      toast.success('Quiet period saved', { description: 'Used for the next Waiting Fix comment.' });
    } catch (err) {
      toast.error('Could not save the quiet period', { description: err instanceof Error ? err.message : String(err) });
    } finally {
      savingQuiet = false;
    }
  }
</script>

<section>
  <h1>General</h1>
  <p class="lede">Preferences for this computer.</p>
</section>

<section>
  <div class="row-setting">
    <div class="txt">
      Theme
      <span>Dark is the default. System follows your operating system.</span>
    </div>
    <div class="radio-row" role="radiogroup" aria-label="Theme">
      {#each OPTIONS as o (o.value)}
        {@const Icon = o.icon}
        <label class="radio-card">
          <input
            type="radio"
            name="theme"
            value={o.value}
            checked={theme.preference === o.value}
            onchange={() => theme.set(o.value)}
          />
          <Icon class="icon-sm" aria-hidden="true" />{o.label}
        </label>
      {/each}
    </div>
  </div>
</section>

<section>
  <h2>Desktop notifications</h2>
  <p class="lede">Every event is still listed in the notification center. These switches only control the desktop pop-up.</p>
  <div style="margin-top:8px">
    {#each NOTIFY_KINDS as k (k.kind)}
      <div class="row-setting">
        <label class="txt" for="notify-{k.kind}">{k.label}</label>
        <Switch
          id="notify-{k.kind}"
          checked={notifyPrefs.prefs[k.kind] ?? true}
          onCheckedChange={(on) => notifyPrefs.set(k.kind, on)}
        />
      </div>
    {/each}
  </div>
</section>

<section>
  <h2>Waiting Fix</h2>
  <div class="row-setting">
    <div class="txt">
      <label for="g-quiet">Quiet period</label>
      <span
        >Seconds to wait after a trusted comment or Feedback QA before forwarding it to the agent, so a reviewer can
        finish writing. Applies to the daemon on this machine.</span
      >
      {#if quietError}
        <p class="field-error" id="g-quiet-err"><CircleAlert class="icon-sm" aria-hidden="true" />{quietError}</p>
      {:else if savedQuiet === null}
        <span>Set up the Lark app first.</span>
      {/if}
    </div>
    <div class="quiet-input" data-invalid={quietError ? 'true' : undefined}>
      <input
        id="g-quiet"
        class="mono"
        type="text"
        inputmode="numeric"
        autocomplete="off"
        disabled={savedQuiet === null || savingQuiet}
        value={quiet}
        oninput={(e) => (quietDraft = (e.currentTarget as HTMLInputElement).value)}
        onblur={saveQuiet}
        onkeydown={(e) => e.key === 'Enter' && (e.currentTarget as HTMLInputElement).blur()}
        aria-invalid={quietError ? 'true' : undefined}
        aria-describedby={quietError ? 'g-quiet-err' : undefined}
      />
      <span class="muted">sec</span>
    </div>
  </div>
</section>

<style>
  .quiet-input {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 120px;
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--input);
    border-radius: var(--radius);
    background: transparent;
  }
  .quiet-input:focus-within {
    border-color: var(--ring);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--ring) 40%, transparent);
  }
  .quiet-input[data-invalid='true'] {
    border-color: var(--destructive);
  }
  .quiet-input input {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: 0;
    outline: none;
    color: inherit;
    text-align: right;
  }
  .quiet-input input:disabled {
    opacity: 0.5;
  }
</style>
