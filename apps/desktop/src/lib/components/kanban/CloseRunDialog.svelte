<script lang="ts">
  import * as AlertDialog from '#lib/components/ui/alert-dialog/index.ts';
  import { buttonVariants } from '#lib/components/ui/button/index.ts';

  interface Props {
    taskKey: string;
    branch: string | null;
    /** Alasan tambahan, mis. label flag kartu. */
    reason?: string;
    onconfirm: () => void;
    oncancel: () => void;
  }
  let { taskKey, branch, reason, onconfirm, oncancel }: Props = $props();
</script>

<AlertDialog.Root open onOpenChange={(open) => !open && oncancel()}>
  <AlertDialog.Content>
    <AlertDialog.Header>
      <AlertDialog.Title>Close the run for {taskKey}?</AlertDialog.Title>
      <AlertDialog.Description>
        {reason ? `${reason}. ` : ''}Mandor stops the agent and removes the worktree.
        {branch ? `The branch ${branch} stays on origin, and Lark is not changed.` : 'Lark is not changed.'}
      </AlertDialog.Description>
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
      <AlertDialog.Action class={buttonVariants({ variant: 'destructive' })} onclick={onconfirm}>Close run</AlertDialog.Action>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
