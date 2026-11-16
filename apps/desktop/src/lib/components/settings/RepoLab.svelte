<script lang="ts">
  // Bagian "Review lab" di pengaturan repo (plan 2026-11-10-001, U9): akun uji
  // per role, form login, path POST baca-saja, folder berkas uji, usulan dari
  // agent pengusul, simpan, dan setujui. Password tersimpan tidak pernah
  // ditampilkan; field kosong mempertahankannya.
  import FlaskConical from '@lucide/svelte/icons/flask-conical';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import Plus from '@lucide/svelte/icons/plus';
  import Sparkles from '@lucide/svelte/icons/sparkles';
  import X from '@lucide/svelte/icons/x';
  import { toast } from 'svelte-sonner';
  import * as AlertDialog from '#lib/components/ui/alert-dialog/index.ts';
  import { Button, buttonVariants } from '#lib/components/ui/button/index.ts';
  import { Input } from '#lib/components/ui/input/index.ts';
  import {
    emptyLabForm,
    type LabForm,
    labApproveParams,
    labForm,
    labSetParams,
    labStatus,
    validateLabForm
  } from '#lib/lab.ts';
  import type { LabView } from '#lib/protocol/LabView.ts';
  import type { Rpc } from '#lib/rpc.ts';
  import Field from './Field.svelte';
  import ListEditor from './ListEditor.svelte';

  let { rpc, repoId }: { rpc: Rpc; repoId: string } = $props();

  let view = $state<LabView | null>(null);
  let form = $state<LabForm>(emptyLabForm());
  let loadedFor = $state<string | null>(null);
  let loadError = $state<string | null>(null);
  let tried = $state(false);
  let busy = $state(false);
  let approving = $state(false);
  /** Form berisi usulan agent yang belum disimpan. */
  let fromProposal = $state(false);

  const status = $derived(labStatus(view));
  const errors = $derived(tried ? validateLabForm(form) : {});

  function message(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
  }

  async function load(): Promise<void> {
    try {
      view = await rpc.request<LabView>('lab.get', { repo_id: repoId });
      loadError = null;
    } catch (err) {
      loadError = message(err);
    }
  }

  $effect(() => {
    if (loadedFor === repoId) return;
    loadedFor = repoId;
    view = null;
    tried = false;
    fromProposal = false;
    void load().then(() => {
      form = labForm(view?.equipment ?? null);
    });
  });

  // Usulan berjalan di daemon; status dimuat ulang sampai selesai.
  $effect(() => {
    if (!view?.proposing) return;
    const timer = setInterval(() => void load(), 3000);
    return () => clearInterval(timer);
  });

  async function propose(): Promise<void> {
    busy = true;
    try {
      view = await rpc.request<LabView>('lab.propose', { repo_id: repoId });
    } catch (err) {
      toast.error('Could not start the proposal', { description: message(err) });
    } finally {
      busy = false;
    }
  }

  function useProposal(): void {
    if (!view?.proposal) return;
    form = labForm(view.proposal);
    fromProposal = true;
    tried = true;
  }

  async function save(): Promise<void> {
    tried = true;
    if (Object.keys(validateLabForm(form)).length > 0) return;
    busy = true;
    try {
      view = await rpc.request<LabView>('lab.set', labSetParams(repoId, form));
      form = labForm(view.equipment);
      fromProposal = false;
      toast.success('Lab equipment saved', { description: 'Approve it to let reviews use it.' });
    } catch (err) {
      toast.error('Could not save the lab equipment', { description: message(err) });
    } finally {
      busy = false;
    }
  }

  async function approve(): Promise<void> {
    approving = false;
    if (!view?.equipment) return;
    busy = true;
    try {
      view = await rpc.request<LabView>('lab.approve', labApproveParams(repoId, labForm(view.equipment)));
      toast.success('Lab equipment approved', { description: 'Reviews of this repo sign in with these accounts.' });
    } catch (err) {
      toast.error('Could not approve', { description: message(err) });
    } finally {
      busy = false;
    }
  }

  function addAccount(): void {
    form.accounts = [
      ...form.accounts,
      { role: '', username: '', default: form.accounts.length === 0, hasPassword: false, password: '' }
    ];
  }

  function setDefault(i: number): void {
    form.accounts = form.accounts.map((a, j) => ({ ...a, default: i === j }));
  }
</script>

<div class="field lab">
  <div class="lab-head">
    <span class="lab-title text-sm font-medium"><FlaskConical class="icon-sm" aria-hidden="true" /> Review lab</span>
    <span class="chip" data-tone={status.tone}>
      {#if view?.proposing}<LoaderCircle class="spin" aria-hidden="true" />{/if}{status.text}
    </span>
  </div>
  <p class="field-help">
    Test accounts and the login form Mandor uses to sign in before reviews, so criteria that need another role, a
    simulated failure, or a test file can be checked without you. Use local test accounts only; passwords stay on the
    daemon machine and are never shown again.
  </p>
  {#if loadError}<p class="field-error">{loadError}</p>{/if}

  <div class="lab-actions">
    <Button variant="outline" size="sm" disabled={busy || view?.proposing} onclick={() => void propose()}>
      <Sparkles />Propose from repo
    </Button>
    {#if view?.proposal && !fromProposal}
      <Button variant="secondary" size="sm" onclick={useProposal}>Review the proposal</Button>
    {/if}
  </div>
  {#if fromProposal}
    <p class="field-help">The form shows the agent's proposal. Check every field, enter the passwords, then save.</p>
  {/if}

  <div class="field">
    <span class="text-sm font-medium">Test accounts</span>
    {#if errors.accounts}<p class="field-error">{errors.accounts}</p>{/if}
    <table class="table lab-accounts">
      <thead><tr><th>Role</th><th>User name</th><th>Password</th><th>Default</th><th><span class="sr-only">Remove</span></th></tr></thead>
      <tbody>
        {#each form.accounts as a, i (i)}
          <tr>
            <td><Input class="mono" aria-label="Role" placeholder="admin" bind:value={a.role} /></td>
            <td><Input aria-label="User name for {a.role || 'this account'}" bind:value={a.username} /></td>
            <td>
              <Input
                type="password"
                autocomplete="new-password"
                aria-label="Password for {a.role || 'this account'}"
                placeholder={a.hasPassword ? 'Saved; leave empty to keep' : 'Required'}
                bind:value={a.password}
              />
            </td>
            <td>
              <input
                type="radio"
                name="lab-default-{repoId}"
                checked={a.default}
                aria-label="Use {a.role || 'this account'} for ordinary review"
                onchange={() => setDefault(i)}
              />
            </td>
            <td>
              <Button
                variant="ghost"
                size="icon-xs"
                aria-label="Remove {a.role || 'account'}"
                onclick={() => (form.accounts = form.accounts.filter((_, j) => j !== i))}><X /></Button
              >
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    <div><Button variant="outline" size="sm" disabled={form.accounts.length >= 4} onclick={addAccount}><Plus />Add account</Button></div>
  </div>

  <div class="grid gap-3" style="grid-template-columns:repeat(auto-fill,minmax(240px,1fr))">
    <Field id="lab-url" label="Login page URL" error={errors.loginUrl}>
      <Input id="lab-url" class="mono" placeholder="http://localhost:5173/login" bind:value={form.loginUrl} />
    </Field>
    <Field id="lab-user" label="User name field" error={errors.usernameField}>
      <Input id="lab-user" class="mono" placeholder="#email" bind:value={form.usernameField} />
      {#snippet help()}#id, [name=x], or text=…{/snippet}
    </Field>
    <Field id="lab-pass" label="Password field" error={errors.passwordField}>
      <Input id="lab-pass" class="mono" placeholder="#password" bind:value={form.passwordField} />
    </Field>
    <Field id="lab-submit" label="Sign-in button" error={errors.submit}>
      <Input id="lab-submit" class="mono" placeholder="text=Sign in" bind:value={form.submit} />
    </Field>
    <Field id="lab-submit-path" label="Login request path" error={errors.submitPath}>
      <Input id="lab-submit-path" class="mono" placeholder="/api/login" bind:value={form.submitPath} />
      {#snippet help()}The form's POST; the review proxy lets only this save through during sign-in.{/snippet}
    </Field>
    <Field id="lab-success" label="Signed in when" error={errors.success}>
      <div class="lab-success">
        <select class="native-select" aria-label="Signed-in marker" bind:value={form.successKind}>
          <option value="path">the path starts with</option>
          <option value="text">the page shows</option>
        </select>
        <Input id="lab-success" class="mono" placeholder="/dashboard" bind:value={form.success} />
      </div>
    </Field>
    <Field id="lab-auth" label="Page that needs a login" error={errors.authenticatedUrl}>
      <Input id="lab-auth" class="mono" placeholder="http://localhost:5173/dashboard" bind:value={form.authenticatedUrl} />
    </Field>
    <Field id="lab-fixtures" label="Test files folder (optional)">
      <Input id="lab-fixtures" class="mono" placeholder="review-fixtures" bind:value={form.fixturesDir} />
      {#snippet help()}Relative to the repo. Mandor copies regular files only, never dotfiles or links.{/snippet}
    </Field>
  </div>
  <div class="field">
    <span class="text-sm font-medium">POST requests that only read data</span>
    <p class="field-help">The review proxy blocks every other save request, so reviews never change data.</p>
    <ListEditor list="posts" label="read-only POST paths" placeholder="/api/search" bind:items={form.safePosts} />
  </div>

  <div class="lab-actions">
    <Button size="sm" disabled={busy} onclick={() => void save()}>Save lab equipment</Button>
    {#if view?.equipment && !view.approved}
      <Button variant="secondary" size="sm" disabled={busy || fromProposal} onclick={() => (approving = true)}>Approve…</Button>
    {/if}
  </div>
</div>

{#if approving && view?.equipment}
  {@const eq = view.equipment}
  <AlertDialog.Root open onOpenChange={(o) => !o && (approving = false)}>
    <AlertDialog.Content>
      <AlertDialog.Header>
        <AlertDialog.Title>Approve the review lab for {repoId}?</AlertDialog.Title>
        <AlertDialog.Description>
          Reviews sign in with these accounts and may run JavaScript in the app's pages. Saving any change later asks for
          approval again.
        </AlertDialog.Description>
      </AlertDialog.Header>
      <dl class="lab-review">
        <dt>Accounts</dt>
        <dd>
          {#each eq.accounts as a (a.role)}
            <div class="mono">{a.role}: {a.username}{a.default ? ' (default)' : ''}{a.has_password ? '' : ' · no password'}</div>
          {/each}
        </dd>
        <dt>Login</dt>
        <dd class="mono">{eq.login.url} → POST {eq.login.submit_path}</dd>
        <dt>Signed in when</dt>
        <dd class="mono">{eq.login.success_kind === 'text' ? 'page shows' : 'path starts with'} {eq.login.success}</dd>
        <dt>Read-only POST</dt>
        <dd class="mono">{eq.safe_posts.length > 0 ? eq.safe_posts.join(', ') : 'none'}</dd>
        <dt>Test files</dt>
        <dd class="mono">{eq.fixtures_dir ?? 'none'}</dd>
      </dl>
      <AlertDialog.Footer>
        <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
        <AlertDialog.Action class={buttonVariants({ variant: 'default' })} onclick={() => void approve()}>Approve</AlertDialog.Action>
      </AlertDialog.Footer>
    </AlertDialog.Content>
  </AlertDialog.Root>
{/if}

<style>
  .lab-title { display: inline-flex; align-items: center; gap: 6px; }
  .lab-head { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .lab-actions { display: flex; gap: 8px; flex-wrap: wrap; }
  .lab-accounts td { padding: 4px; vertical-align: middle; }
  .lab-success { display: flex; gap: 6px; }
  .native-select { border: 1px solid var(--border); border-radius: var(--radius); background: var(--background); padding: 0 6px; font-size: var(--text-sm); }
  .lab-review { display: grid; grid-template-columns: max-content 1fr; gap: 4px 12px; font-size: var(--text-sm); }
  .lab-review dt { color: var(--muted-foreground); }
  .lab-review dd { margin: 0; overflow-wrap: anywhere; }
</style>
