<script lang="ts">
  import FolderGit2 from '@lucide/svelte/icons/folder-git-2';
  import Plus from '@lucide/svelte/icons/plus';
  import Trash from '@lucide/svelte/icons/trash';
  import { toast } from 'svelte-sonner';
  import EmptyState from '#lib/components/mandor/EmptyState.svelte';
  import * as AlertDialog from '#lib/components/ui/alert-dialog/index.ts';
  import { Button, buttonVariants } from '#lib/components/ui/button/index.ts';
  import { Input } from '#lib/components/ui/input/index.ts';
  import type { RepoView } from '#lib/protocol/RepoView.ts';
  import type { Rpc } from '#lib/rpc.ts';
  import { lark } from '#lib/stores/lark.svelte.ts';
  import AddRepoDialog from './AddRepoDialog.svelte';
  import Field from './Field.svelte';
  import ListEditor from './ListEditor.svelte';
  import { Switch } from '#lib/components/ui/switch/index.ts';
  import { type RepoAppForm, validateRepoApp } from '#lib/settings.ts';

  let { rpc }: { rpc: Rpc } = $props();

  let selectedId = $state<string | null>(null);
  const repo = $derived(lark.repos.find((r) => r.id === selectedId) ?? lark.repos[0]);

  // Salinan yang bisa diedit; diganti setiap repo terpilih berubah.
  interface Edit {
    gh: string;
    companions: string[];
    reviewers: string[];
    allowed: string[];
    app: RepoAppForm;
    browserTests: boolean;
    autoRestart: boolean;
    unattendedLogin: boolean;
    gate: 'inherit' | 'block' | 'warn';
  }
  let edit = $state<Edit | null>(null);
  let editFor = $state<string | null>(null);
  let tried = $state(false);
  $effect(() => {
    if (repo && editFor !== repo.id) {
      editFor = repo.id;
      tried = false;
      edit = {
        gh: repo.gh_profile ?? '',
        companions: [...repo.companions],
        reviewers: [...repo.trusted_reviewers],
        allowed: [...repo.extra_allowed],
        app: {
          testMode: repo.app.test_mode === 'web' ? 'web' : 'no_app',
          setupCommand: repo.app.setup_command ?? '',
          startCommand: repo.app.start_command ?? '',
          port: repo.app.port === null ? '' : String(repo.app.port),
          appUrl: repo.app.app_url ?? '',
          envKeys: [...repo.app.env_keys]
        },
        browserTests: repo.app.browser_tests,
        autoRestart: repo.app.auto_restart,
        unattendedLogin: repo.app.unattended_login,
        gate: repo.gate_mode === 'block' || repo.gate_mode === 'warn' ? repo.gate_mode : 'inherit'
      };
    }
  });
  const appErrors = $derived(edit && tried ? validateRepoApp(edit.app) : {});

  let adding = $state(false);
  let removing = $state<RepoView | null>(null);

  function message(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
  }

  async function save(r: RepoView): Promise<void> {
    if (!edit) return;
    tried = true;
    if (Object.keys(validateRepoApp(edit.app)).length > 0) return;
    const a = edit.app;
    const web = a.testMode === 'web';
    try {
      await rpc.request('repos.update', {
        id: r.id,
        gh_profile: edit.gh.trim() || null,
        companions: edit.companions,
        trusted_reviewers: edit.reviewers,
        extra_allowed: edit.allowed,
        gate_mode: edit.gate,
        app: {
          test_mode: a.testMode,
          setup_command: web ? a.setupCommand.trim() || null : null,
          start_command: web ? a.startCommand.trim() || null : null,
          port: web && a.port.trim() ? Number(a.port.trim()) : null,
          app_url: web ? a.appUrl.trim() || null : null,
          env_keys: web ? a.envKeys : [],
          browser_tests: edit.browserTests,
          auto_restart: web && edit.autoRestart,
          unattended_login: web && edit.unattendedLogin
        }
      });
      await lark.refresh();
      toast.success('Repo saved', { description: `${r.id} · used on the next agent start` });
    } catch (err) {
      toast.error('Could not save the repo', { description: message(err) });
    }
  }

  async function remove(r: RepoView): Promise<void> {
    removing = null;
    try {
      await rpc.request('repos.remove', { id: r.id });
      await lark.refresh();
      selectedId = null;
      toast.success('Repo removed', { description: r.id });
    } catch (err) {
      toast.error('Could not remove the repo', { description: message(err) });
    }
  }

  function openAdd(): void {
    adding = true;
  }
</script>

<section class="flex items-end gap-3" style="max-width:none">
  <div class="flex-1">
    <h1>Repos</h1>
    <p class="lede">Repos on the daemon machine that agents can work in.</p>
  </div>
  {#if lark.repos.length > 0}
    <Button variant="secondary" size="sm" onclick={openAdd}><Plus />Add repo</Button>
  {/if}
</section>

{#if lark.repos.length === 0}
  <section>
    <div class="settings-card">
      <EmptyState icon={FolderGit2} title="No repos yet">
        Add a git repo on the daemon machine so agents have somewhere to work.
        {#snippet actions()}<Button onclick={openAdd}><Plus />Add a repo</Button>{/snippet}
      </EmptyState>
    </div>
  </section>
{:else}
  <section>
    <div class="settings-card" style="overflow:hidden">
      <table class="table">
        <thead><tr><th>Repo</th><th>Path</th><th>Default branch</th><th>gh profile</th></tr></thead>
        <tbody>
          {#each lark.repos as r (r.id)}
            <tr
              aria-selected={repo?.id === r.id}
              tabindex="0"
              onclick={() => (selectedId = r.id)}
              onkeydown={(e) => e.key === 'Enter' && (selectedId = r.id)}
            >
              <td><b>{r.id}</b></td>
              <td class="mono">{r.path}</td>
              <td class="mono">{r.default_branch ?? '—'}</td>
              <td>{r.gh_profile ?? 'default'}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </section>

  {#if repo && edit}
    <section>
      <div class="settings-card">
        <header>
          <h2>{repo.id}</h2>
          <p class="mono">{repo.path} · origin/{repo.default_branch ?? '?'}</p>
        </header>
        <div class="content">
          <div style="max-width:280px">
            <Field id="r-gh" label="gh profile">
              <Input id="r-gh" placeholder="default" bind:value={edit.gh} />
              {#snippet help()}The GitHub CLI account that pushes and opens PRs for this repo.{/snippet}
            </Field>
          </div>
          <div class="field">
            <span class="text-sm font-medium">Read-only companion paths</span>
            <p class="field-help">Extra folders the agent may read but never write, e.g. a shared design-token repo.</p>
            <ListEditor list="companions" label="companion paths" placeholder="/home/you/work/shared-config" bind:items={edit.companions} />
          </div>
          <div class="field">
            <span class="text-sm font-medium">Trusted reviewers and QA</span>
            <p class="field-help">
              Lark open_ids whose record comments are forwarded to the agent automatically. Assignees and the reporter are
              always trusted.
            </p>
            <ListEditor list="reviewers" label="trusted reviewers" placeholder="ou_51aa6c" bind:items={edit.reviewers} />
          </div>
          <div class="field">
            <span class="text-sm font-medium">Extra agent allowlist</span>
            <p class="field-help">
              Tool patterns the agent can run without asking. Anything not listed is denied, and
              <span class="mono">git push</span> is always run by the daemon.
            </p>
            <ListEditor list="allowed" label="agent allowlist" placeholder="Bash(just *)" bind:items={edit.allowed} />
          </div>
          <hr class="separator" />
          <div class="field">
            <span class="text-sm font-medium">Test mode</span>
            <p class="field-help">
              Web: Mandor can run the app from the run's worktree and the AC reviewer can test it in your browser. No app:
              UI acceptance criteria are rated Can't verify.
            </p>
            <div class="radio-row" role="radiogroup" aria-label="Test mode">
              <label class="radio-card"><input type="radio" name="r-mode" value="web" bind:group={edit.app.testMode} />Web</label>
              <label class="radio-card"><input type="radio" name="r-mode" value="no_app" bind:group={edit.app.testMode} />No app</label>
            </div>
          </div>
          {#if edit.app.testMode === 'web'}
            <div class="grid gap-3" style="grid-template-columns:repeat(auto-fill,minmax(240px,1fr))">
              <Field id="r-setup" label="Setup command (optional)">
                <Input id="r-setup" class="mono" placeholder="bun install" bind:value={edit.app.setupCommand} />
                {#snippet help()}Runs before start when the lockfile changed.{/snippet}
              </Field>
              <Field id="r-start" label="Start command" error={appErrors.startCommand}>
                <Input id="r-start" class="mono" placeholder="bun run dev" bind:value={edit.app.startCommand} />
              </Field>
              <Field id="r-port" label="Port" error={appErrors.port}>
                <Input id="r-port" class="mono" inputmode="numeric" placeholder="5173" bind:value={edit.app.port} />
              </Field>
              <Field id="r-url" label="App URL (optional)" error={appErrors.appUrl}>
                <Input id="r-url" class="mono" placeholder="http://localhost:5173" bind:value={edit.app.appUrl} />
                {#snippet help()}Where you log in. Login cookies are tied to this host.{/snippet}
              </Field>
            </div>
            <div class="field">
              <span class="text-sm font-medium">.env keys passed to the app</span>
              <p class="field-help">
                Key names only, read from this repo's .env. Values are never shown or copied into the worktree. Use local or
                dev credentials only.
              </p>
              <ListEditor list="env" label=".env keys" placeholder="VITE_API_URL" bind:items={edit.app.envKeys} />
              {#if appErrors.envKeys}<p class="field-error">{appErrors.envKeys}</p>{/if}
            </div>
          {/if}
          <div class="row-setting">
            <div class="txt">
              Browser testing
              <span>For single-repo runs. Page content is read by the AI model. Workspace runs use the workspace setting.</span>
            </div>
            <Switch bind:checked={edit.browserTests} aria-label="Browser testing" />
          </div>
          {#if edit.app.testMode === 'web' && edit.browserTests}
            <div class="row-setting">
              <div class="txt">
                Restart the app before reviews
                <span>
                  When this run's app runs older code, Mandor restarts it from the latest code before a browser review,
                  including automatic reviews after Waiting Fix. The agent's code then runs with the selected .env keys
                  without you checking the changes first. Use only with local or dev credentials. Needs a login you
                  confirmed once; another run's app is never stopped.
                </span>
              </div>
              <Switch bind:checked={edit.autoRestart} aria-label="Restart the app before reviews" />
            </div>
            <div class="row-setting">
              <div class="txt">
                Setup command prepares a test login
                <span>
                  For overnight work on the VM. The setup command creates a test user and signs in by itself, so the
                  headless browser review can run without you confirming the login. Without this, browser checks on the
                  VM are marked Can't verify and the card is held for you.
                </span>
              </div>
              <Switch bind:checked={edit.unattendedLogin} aria-label="Setup command prepares a test login" />
            </div>
          {/if}
          <div class="field">
            <span class="text-sm font-medium">Push &amp; PR gate</span>
            <p class="field-help">
              Repo override &gt; workspace &gt; default (Block). A multi-repo run uses the strictest member.
            </p>
            <div class="radio-row" role="radiogroup" aria-label="Gate">
              <label class="radio-card"><input type="radio" name="r-gate" value="inherit" bind:group={edit.gate} />Use workspace</label>
              <label class="radio-card"><input type="radio" name="r-gate" value="block" bind:group={edit.gate} />Block</label>
              <label class="radio-card"><input type="radio" name="r-gate" value="warn" bind:group={edit.gate} />Warn</label>
            </div>
          </div>
        </div>
        <footer style="justify-content:space-between">
          <Button variant="ghost" class="text-status-failed" onclick={() => (removing = repo)}><Trash />Remove repo</Button>
          <Button onclick={() => save(repo)}>Save changes</Button>
        </footer>
      </div>
    </section>
  {/if}
{/if}

{#if adding}
  <AddRepoDialog
    {rpc}
    oncancel={() => (adding = false)}
    onadded={async (r) => {
      adding = false;
      await lark.refresh();
      selectedId = r.id;
      toast.success('Repo added', { description: `${r.id} · default branch ${r.default_branch ?? 'unknown'}` });
    }}
  />
{/if}

{#if removing}
  {@const r = removing}
  <AlertDialog.Root open onOpenChange={(o) => !o && (removing = null)}>
    <AlertDialog.Content>
      <AlertDialog.Header>
        <AlertDialog.Title>Remove {r.id}?</AlertDialog.Title>
        <AlertDialog.Description>
          Mandor forgets this repo and its settings. The folder at {r.path} is not deleted. Runs already working in it keep
          running.
        </AlertDialog.Description>
      </AlertDialog.Header>
      <AlertDialog.Footer>
        <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
        <AlertDialog.Action class={buttonVariants({ variant: 'destructive' })} onclick={() => remove(r)}>Remove repo</AlertDialog.Action>
      </AlertDialog.Footer>
    </AlertDialog.Content>
  </AlertDialog.Root>
{/if}
