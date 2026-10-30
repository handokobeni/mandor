<script lang="ts">
  import { untrack } from 'svelte';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { toast } from 'svelte-sonner';
  import { Button } from '#lib/components/ui/button/index.ts';
  import { Input } from '#lib/components/ui/input/index.ts';
  import { type LarkErrors, type LarkForm, larkFormDirty, validateLark } from '#lib/settings.ts';
  import { lark, type LarkStore } from '#lib/stores/lark.svelte.ts';
  import Field from './Field.svelte';

  function fromSettings(): LarkForm {
    const s = store.settings;
    return {
      appId: s?.app_id ?? '',
      appSecret: '',
      appToken: s?.app_token ?? '',
      tableId: s?.table_id ?? '',
      acTableId: s?.ac_table_id ?? '',
      port: String(s?.redirect_port ?? 53682)
    };
  }

  // Daemon VM memakai store miliknya sendiri (Settings › This laptop / VM).
  let { onsaved, store = lark }: { onsaved?: () => void; store?: LarkStore } = $props();

  let form = $state<LarkForm>(fromSettings());
  // Pengaturan bisa tiba setelah layar dibuka; isi form sekali saat itu.
  let filled = untrack(() => store.settings !== null);
  $effect(() => {
    if (!filled && store.settings) {
      form = fromSettings();
      filled = true;
    }
  });
  let errors = $state<LarkErrors>({});
  let saving = $state(false);
  let replaceSecret = $state(false);

  const secretSaved = $derived(!!store.settings?.has_secret && !replaceSecret);
  // Reset hanya berarti bila ada perubahan yang belum disimpan.
  const dirty = $derived(larkFormDirty(form, fromSettings(), replaceSecret));

  async function save(e: SubmitEvent): Promise<void> {
    e.preventDefault();
    errors = validateLark(form, secretSaved);
    if (Object.keys(errors).length > 0) return;
    saving = true;
    try {
      await store.save({
        app_id: form.appId.trim(),
        app_secret: secretSaved ? null : form.appSecret.trim(),
        app_token: form.appToken.trim(),
        table_id: form.tableId.trim(),
        ac_table_id: form.acTableId.trim(),
        redirect_port: Number(form.port),
        open_base: null,
        accounts_base: null,
        quiet_secs: null
      });
      form.appSecret = '';
      replaceSecret = false;
      toast.success('Lark settings saved', { description: 'The daemon reloaded the Base binding.' });
      onsaved?.();
    } catch (err) {
      toast.error('Could not save Lark settings', { description: err instanceof Error ? err.message : String(err) });
    } finally {
      saving = false;
    }
  }
</script>

  <form class="settings-card" novalidate onsubmit={save}>
    <header>
      <h2>Custom App</h2>
      <p>From the Lark Developer Console, under Credentials &amp; Basic Info.</p>
    </header>
    <div class="content grid-2">
      <Field id="f-appid" label="App ID" error={errors.appId}>
        <Input id="f-appid" class="font-mono" autocomplete="off" spellcheck="false" bind:value={form.appId} aria-invalid={errors.appId ? 'true' : undefined} />
        {#snippet help()}Starts with <span class="mono">cli_</span>.{/snippet}
      </Field>
      <Field id="f-secret" label="App Secret" error={errors.appSecret}>
        <div class="flex gap-2">
          <Input
            id="f-secret"
            type="password"
            class="font-mono"
            autocomplete="off"
            disabled={secretSaved}
            placeholder={secretSaved ? '•••••••• saved' : 'Paste the App Secret'}
            bind:value={form.appSecret}
            aria-invalid={errors.appSecret ? 'true' : undefined}
          />
          {#if store.settings?.has_secret}
            <Button variant="outline" type="button" onclick={() => (replaceSecret = !replaceSecret)}>
              {replaceSecret ? 'Keep saved' : 'Replace'}
            </Button>
          {/if}
        </div>
        {#snippet help()}Stored by the daemon. Never shown again after saving.{/snippet}
      </Field>
    </div>
    <header style="padding-top:0">
      <h2>Base</h2>
      <p>Open Sprint Tasks in the browser: <span class="mono">…/base/<b>app_token</b>?table=<b>tbl…</b></span></p>
    </header>
    <div class="content grid-2">
      <Field id="f-token" label="Base app_token" error={errors.appToken}>
        <Input id="f-token" class="font-mono" autocomplete="off" spellcheck="false" bind:value={form.appToken} aria-invalid={errors.appToken ? 'true' : undefined} />
        {#snippet help()}The part after <span class="mono">/base/</span> in the URL.{/snippet}
      </Field>
      <Field id="f-tasks" label="Sprint Tasks table ID" error={errors.tableId}>
        <Input id="f-tasks" class="font-mono" autocomplete="off" spellcheck="false" bind:value={form.tableId} aria-invalid={errors.tableId ? 'true' : undefined} />
        {#snippet help()}The <span class="mono">table=</span> value with Sprint Tasks open.{/snippet}
      </Field>
      <Field id="f-ac" label="Acceptance Criteria table ID" error={errors.acTableId}>
        <Input id="f-ac" class="font-mono" autocomplete="off" spellcheck="false" bind:value={form.acTableId} aria-invalid={errors.acTableId ? 'true' : undefined} />
        {#snippet help()}The <span class="mono">table=</span> value with the AC table open.{/snippet}
      </Field>
      <Field id="f-port" label="Callback port" error={errors.port}>
        <Input id="f-port" class="font-mono" inputmode="numeric" bind:value={form.port} aria-invalid={errors.port ? 'true' : undefined} />
        {#snippet help()}Must match the redirect URL in the Lark app. Default 53682.{/snippet}
      </Field>
    </div>
    <footer>
      <Button
        variant="ghost"
        type="button"
        disabled={!dirty || saving}
        title={dirty ? 'Discard unsaved changes' : 'No unsaved changes'}
        onclick={() => {
          form = fromSettings();
          errors = {};
          replaceSecret = false;
        }}>Reset</Button
      >
      <Button type="submit" disabled={saving}>{#if saving}<LoaderCircle class="spin" />{/if}Save</Button>
    </footer>
  </form>
