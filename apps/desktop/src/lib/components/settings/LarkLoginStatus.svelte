<script lang="ts">
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import LogIn from '@lucide/svelte/icons/log-in';
  import LogOut from '@lucide/svelte/icons/log-out';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import UserRoundCheck from '@lucide/svelte/icons/user-round-check';
  import UserRoundX from '@lucide/svelte/icons/user-round-x';
  import { toast } from 'svelte-sonner';
  import * as Alert from '#lib/components/ui/alert/index.ts';
  import { Button } from '#lib/components/ui/button/index.ts';
  import { catchOAuthCode } from '#lib/native.ts';
  import { lark } from '#lib/stores/lark.svelte.ts';

  let waiting = $state(false);
  const status = $derived(lark.status);

  async function login(): Promise<void> {
    waiting = true;
    try {
      await lark.login(catchOAuthCode);
      toast.success('Logged in to Lark');
    } catch (err) {
      toast.error('Lark login failed', { description: err instanceof Error ? err.message : String(err) });
    } finally {
      waiting = false;
    }
  }

  async function logout(): Promise<void> {
    try {
      await lark.logout();
      toast.success('Logged out of Lark', { description: 'Board sync is paused.' });
    } catch (err) {
      toast.error('Could not log out', { description: err instanceof Error ? err.message : String(err) });
    }
  }
</script>

  {#if waiting}
    <Alert.Root>
      <LoaderCircle class="spin" />
      <Alert.Title>Waiting for the browser…</Alert.Title>
      <Alert.Description>
        Finish the Lark login in the browser window. Mandor listens on port {lark.settings?.redirect_port ?? 53682}.
      </Alert.Description>
    </Alert.Root>
  {:else if status?.reauth_required}
    <Alert.Root>
      <TriangleAlert class="text-status-warning" />
      <Alert.Title>Session expired</Alert.Title>
      <Alert.Description>Board sync is paused until you log in again. Agents keep running.</Alert.Description>
      <Alert.Action><Button size="sm" onclick={login}><LogIn />Log in again</Button></Alert.Action>
    </Alert.Root>
  {:else if status?.logged_in}
    <Alert.Root>
      <UserRoundCheck />
      <Alert.Title>Logged in as <span class="mono">{status.open_id}</span></Alert.Title>
      <Alert.Description>Token refreshes automatically. Status changes in Lark are written as you.</Alert.Description>
      <Alert.Action><Button variant="outline" size="sm" onclick={logout}><LogOut />Log out</Button></Alert.Action>
    </Alert.Root>
  {:else}
    <Alert.Root>
      <UserRoundX />
      <Alert.Title>Not logged in to Lark</Alert.Title>
      <Alert.Description>
        {status?.configured
          ? 'Log in to read Sprint Tasks and write status changes as you.'
          : 'Fill in and save the Custom App below first, then log in.'}
      </Alert.Description>
      <Alert.Action>
        <Button size="sm" disabled={!status?.configured} onclick={login}><LogIn />Log in with Lark</Button>
      </Alert.Action>
    </Alert.Root>
  {/if}
