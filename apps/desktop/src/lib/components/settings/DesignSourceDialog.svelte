<script lang="ts">
  // Tambah atau ubah sumber desain berjenis folder atau repo (Rilis 2.0).
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import { untrack } from 'svelte';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { Button } from '#lib/components/ui/button/index.ts';
  import * as Dialog from '#lib/components/ui/dialog/index.ts';
  import { Input } from '#lib/components/ui/input/index.ts';
  import type { DesignSourceCheckView } from '#lib/protocol/DesignSourceCheckView.ts';
  import type { DesignSourceView } from '#lib/protocol/DesignSourceView.ts';
  import type { Rpc } from '#lib/rpc.ts';
  import { Switch } from '#lib/components/ui/switch/index.ts';
  import { prototypeForm, prototypePayload } from '#lib/prototype.ts';
  import { validateRepoApp } from '#lib/settings.ts';
  import EnvKeyPicker from './EnvKeyPicker.svelte';
  import Field from './Field.svelte';
  import ListEditor from './ListEditor.svelte';
  import FolderBrowser from './FolderBrowser.svelte';

  interface Props {
    rpc: Rpc;
    source: DesignSourceView | null;
    oncancel: () => void;
    onsaved: (s: DesignSourceView) => void;
  }
  let { rpc, source, oncancel, onsaved }: Props = $props();

  // Nilai awal dari sumber yang diedit; dialog dibuat ulang untuk sumber lain.
  let name = $state(untrack(() => source?.name ?? ''));
  let path = $state(untrack(() => source?.path ?? ''));
  let view = $state<'main' | 'browse'>('main');
  let busy = $state(false);
  let error = $state<string | null>(null);
  let check = $state<DesignSourceCheckView | null>(null);
  // Runnable prototype: Run app menyalakannya bersama app run (R6–R8).
  let runnable = $state(untrack(() => source?.app != null));
  let app = $state(untrack(() => prototypeForm(source?.app ?? null)));
  let tried = $state(false);
  let picking = $state(false);
  const appErrors = $derived(runnable && tried ? validateRepoApp(app) : {});

  async function test(): Promise<void> {
    check = await rpc.request<DesignSourceCheckView>('design_sources.test', { path: path.trim() });
  }

  async function save(): Promise<void> {
    tried = true;
    if (runnable && Object.keys(validateRepoApp(app)).length > 0) return;
    busy = true;
    error = null;
    const appPayload = runnable ? prototypePayload(app) : null;
    try {
      const saved = source
        ? await rpc.request<DesignSourceView>('design_sources.update', {
            id: source.id,
            name,
            path: path.trim(),
            app: appPayload
          })
        : await rpc.request<DesignSourceView>('design_sources.add', { name, path: path.trim(), app: appPayload });
      onsaved(saved);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog.Root open onOpenChange={(o) => !o && oncancel()}>
  <Dialog.Content class="sm:max-w-[560px] max-h-[90vh] overflow-y-auto">
    {#if view === 'main'}
      <Dialog.Header>
        <Dialog.Title class="text-[length:var(--text-md)]">{source ? 'Edit design source' : 'Add a design source'}</Dialog.Title>
        <Dialog.Description>
          A code repo or folder on the daemon machine, such as a prototype. Agents read it during planning and review and
          never change it.
        </Dialog.Description>
      </Dialog.Header>
      <Field id="ds-name" label="Name">
        <Input id="ds-name" placeholder="kelola-mock" bind:value={name} />
      </Field>
      <Field id="ds-path" label="Path" error={error ?? undefined}>
        <div class="flex gap-2">
          <Input id="ds-path" class="font-mono" placeholder="/home/you/work/kelola-mock" bind:value={path} />
          <Button variant="outline" type="button" onclick={() => (view = 'browse')}><FolderOpen />Browse…</Button>
        </div>
        {#snippet help()}
          {#if check}
            <span class={check.ready ? 'text-status-ok' : 'text-status-failed'}>{check.status}</span>
          {:else}Absolute path on the daemon machine.{/if}
        {/snippet}
      </Field>
      <div class="row-setting">
        <div class="txt">
          Runnable prototype
          <span>Run app also starts this prototype so the AC reviewer can compare pages side by side.</span>
        </div>
        <Switch bind:checked={runnable} aria-label="Runnable prototype" />
      </div>
      {#if runnable}
        <div class="grid grid-cols-2 gap-3">
          <Field id="ds-setup" label="Setup command (optional)">
            <Input id="ds-setup" class="mono" placeholder="pnpm install" bind:value={app.setupCommand} />
          </Field>
          <Field id="ds-start" label="Start command" error={appErrors.startCommand}>
            <Input id="ds-start" class="mono" placeholder="pnpm dev --port 3001" bind:value={app.startCommand} />
          </Field>
          <Field id="ds-port" label="Port" error={appErrors.port}>
            <Input id="ds-port" class="mono" placeholder="3001" bind:value={app.port} />
          </Field>
          <Field id="ds-url" label="URL (optional)" error={appErrors.appUrl}>
            <Input id="ds-url" class="mono" placeholder="http://127.0.0.1:3001" bind:value={app.appUrl} />
            {#snippet help()}Use a different host from the app (127.0.0.1 when the app uses localhost).{/snippet}
          </Field>
        </div>
        <div class="field">
          <span class="text-sm font-medium">.env keys passed to the prototype</span>
          <ListEditor list="env" label=".env keys" placeholder="KELOLA_API_MODE" bind:items={app.envKeys} />
          {#if appErrors.envKeys}<p class="field-error">{appErrors.envKeys}</p>{/if}
          {#if source}
            <div>
              <Button variant="outline" size="sm" onclick={() => (picking = true)}>Pick from .env</Button>
            </div>
          {/if}
        </div>
        {#if picking && source}
          <EnvKeyPicker
            {rpc}
            target={{ design_source_id: source.id }}
            saved={app.envKeys}
            oncancel={() => (picking = false)}
            onpick={(keys) => {
              app.envKeys = keys;
              picking = false;
            }}
          />
        {/if}
      {/if}
      <Dialog.Footer>
        <Button variant="ghost" disabled={busy} onclick={oncancel}>Cancel</Button>
        <Button variant="outline" disabled={busy || !path.trim()} onclick={test}>Test</Button>
        <Button disabled={busy || !path.trim() || !name.trim()} onclick={save}>
          {#if busy}<LoaderCircle class="spin" />Checking{:else}<CircleCheck />{source ? 'Save' : 'Add source'}{/if}
        </Button>
      </Dialog.Footer>
    {:else}
      <FolderBrowser
        {rpc}
        start={path.trim() || null}
        pickRepos={false}
        description="Folders on the daemon machine."
        onpick={(p) => {
          path = p;
          check = null;
          view = 'main';
        }}
        onback={() => (view = 'main')}
      />
    {/if}
  </Dialog.Content>
</Dialog.Root>
