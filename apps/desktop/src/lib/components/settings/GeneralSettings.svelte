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
  import { Button } from '#lib/components/ui/button/index.ts';
  import { Input } from '#lib/components/ui/input/index.ts';
  import type { ReviewerSettingsView } from '#lib/protocol/ReviewerSettingsView.ts';
  import type { Rpc } from '#lib/rpc.ts';
  import Field from './Field.svelte';

  let { rpc }: { rpc: Rpc } = $props();

  // Setelan reviewer AC (global, disimpan daemon; KTD-30).
  let reviewer = $state<{ model: string; budget: string; minutes: string } | null>(null);
  let reviewerError = $state<string | null>(null);
  $effect(() => {
    rpc
      .request<ReviewerSettingsView>('settings.reviewer.get')
      .then((s) => {
        reviewer = { model: s.model ?? '', budget: (s.budget_cents / 100).toFixed(2), minutes: String(Math.round(s.timeout_secs / 60)) };
      })
      .catch(() => (reviewer = null));
  });

  async function saveReviewer(): Promise<void> {
    if (!reviewer) return;
    const budget = Number(reviewer.budget);
    const minutes = Number(reviewer.minutes);
    if (!Number.isFinite(budget) || budget < 0.1 || budget > 50) {
      reviewerError = 'Use a cost limit between $0.10 and $50.';
      return;
    }
    if (!Number.isInteger(minutes) || minutes < 1 || minutes > 120) {
      reviewerError = 'Use a time limit between 1 and 120 minutes.';
      return;
    }
    reviewerError = null;
    try {
      await rpc.request('settings.reviewer.set', {
        model: reviewer.model.trim() || null,
        budget_cents: Math.round(budget * 100),
        timeout_secs: minutes * 60
      });
      toast.success('Reviewer settings saved', { description: 'Used by the next review.' });
    } catch (err) {
      reviewerError = err instanceof Error ? err.message : String(err);
    }
  }

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

<section>
  <h2>AC reviewer</h2>
  <p class="lede">Global settings for every review. Each review stops at the cost or time limit and is marked Failed.</p>
  {#if reviewer}
    <div class="grid gap-3" style="grid-template-columns:repeat(auto-fill,minmax(200px,1fr));margin-top:8px">
      <Field id="g-rv-model" label="Model">
        <Input id="g-rv-model" placeholder="Same as the agent" bind:value={reviewer.model} />
      </Field>
      <Field id="g-rv-budget" label="Cost limit per review (USD)">
        <Input id="g-rv-budget" class="mono" inputmode="decimal" bind:value={reviewer.budget} />
      </Field>
      <Field id="g-rv-time" label="Time limit (minutes)">
        <Input id="g-rv-time" class="mono" inputmode="numeric" bind:value={reviewer.minutes} />
      </Field>
    </div>
    {#if reviewerError}<p class="field-error"><CircleAlert class="icon-sm" aria-hidden="true" />{reviewerError}</p>{/if}
    <div style="margin-top:8px"><Button size="sm" onclick={saveReviewer}>Save reviewer settings</Button></div>
  {:else}
    <p class="muted">Log in to Lark to change reviewer settings.</p>
  {/if}
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
