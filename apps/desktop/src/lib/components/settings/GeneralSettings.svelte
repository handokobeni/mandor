<script lang="ts">
  import Monitor from '@lucide/svelte/icons/monitor';
  import Moon from '@lucide/svelte/icons/moon';
  import Sun from '@lucide/svelte/icons/sun';
  import { Switch } from '#lib/components/ui/switch/index.ts';
  import { NOTIFY_KINDS } from '#lib/settings.ts';
  import { notifyPrefs } from '#lib/stores/notify-prefs.svelte.ts';
  import { type ThemePreference, theme } from '#lib/stores/theme.svelte.ts';

  const OPTIONS: { value: ThemePreference; label: string; icon: typeof Sun }[] = [
    { value: 'system', label: 'System', icon: Monitor },
    { value: 'dark', label: 'Dark', icon: Moon },
    { value: 'light', label: 'Light', icon: Sun }
  ];
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
