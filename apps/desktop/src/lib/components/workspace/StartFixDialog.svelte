<script lang="ts">
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import ShieldQuestionMark from '@lucide/svelte/icons/shield-question-mark';
  import Wrench from '@lucide/svelte/icons/wrench';
  import { startFixNeedsInstruction, startFixParams } from '#lib/chat.ts';
  import { Button } from '#lib/components/ui/button/index.ts';
  import { Checkbox } from '#lib/components/ui/checkbox/index.ts';
  import * as Dialog from '#lib/components/ui/dialog/index.ts';
  import { Label } from '#lib/components/ui/label/index.ts';
  import { Textarea } from '#lib/components/ui/textarea/index.ts';
  import type { CommentView } from '#lib/protocol/CommentView.ts';
  import { people } from '#lib/stores/people.svelte.ts';

  interface Props {
    taskKey: string;
    recordId: string;
    comments: CommentView[];
    onsubmit: (params: ReturnType<typeof startFixParams>) => void;
    oncancel: () => void;
  }
  let { taskKey, recordId, comments, onsubmit, oncancel }: Props = $props();

  let instruction = $state('');
  let selected = $state<Record<string, boolean>>({});
  let tried = $state(false);

  const needsInstruction = $derived(startFixNeedsInstruction(instruction, comments, selected));
  const missing = $derived(tried && needsInstruction);

  function submit(): void {
    tried = true;
    if (needsInstruction) return;
    onsubmit(startFixParams(recordId, instruction, selected));
  }
</script>

<Dialog.Root open onOpenChange={(o) => !o && oncancel()}>
  <Dialog.Content class="sm:max-w-[600px]">
    <Dialog.Header>
      <span class="mono muted">{taskKey}</span>
      <Dialog.Title class="text-[length:var(--text-md)]">Start fix</Dialog.Title>
      <Dialog.Description>
        The agent gets your instruction plus the comments below. Lark status moves from Waiting Fix to In Progress, and
        back to Waiting Review after the push.
      </Dialog.Description>
    </Dialog.Header>
    <div class="grid gap-1.5">
      <Label for="fx-instr"
        >Instruction <span class="muted font-normal"
          >({comments.some((c) => c.trusted && !c.sent) ? 'optional, trusted comments are sent' : 'required'})</span
        ></Label
      >
      <Textarea
        id="fx-instr"
        rows={4}
        bind:value={instruction}
        aria-invalid={missing ? 'true' : undefined}
        aria-describedby={missing ? 'fx-err' : undefined}
        placeholder="What should the agent fix? e.g. Show the policy limit in the error message, keep the current validation."
      />
      {#if missing}
        <p class="field-error" id="fx-err">
          <CircleAlert class="icon-sm" aria-hidden="true" />Write an instruction or select a comment before starting the fix.
        </p>
      {/if}
    </div>
    {#if comments.length > 0}
      <div class="grid gap-1.5">
        <span class="text-sm font-medium">Comments from Lark</span>
        <div class="grid gap-1.5" style="max-height:280px;overflow:auto">
          {#each comments as c (c.reply_id)}
            <label class="comment flex gap-2.5" style="cursor:pointer">
              <Checkbox
                checked={c.trusted || !!selected[c.reply_id]}
                disabled={c.trusted}
                onCheckedChange={(v) => (selected[c.reply_id] = v === true)}
                aria-label="Include the comment from {people.name(c.author)}"
              />
              <span class="grid gap-1" style="min-width:0">
                <span class="flex items-center gap-2 text-[length:var(--text-sm)]">
                  <b>{people.name(c.author)}</b>
                  {#if c.trusted}
                    <span class="trust inline-flex items-center gap-1" data-trusted="true"><ShieldCheck class="icon-sm" />Trusted, always sent</span>
                  {:else}
                    <span class="trust inline-flex items-center gap-1" data-trusted="false"><ShieldQuestionMark class="icon-sm" />Not trusted</span>
                  {/if}
                </span>
                <span style="white-space:pre-wrap">{c.text}</span>
              </span>
            </label>
          {/each}
        </div>
      </div>
    {/if}
    <Dialog.Footer class="items-center">
      <span class="muted mr-auto text-[length:var(--text-sm)]">Only trusted comments are sent by default.</span>
      <Button variant="ghost" onclick={oncancel}>Cancel</Button>
      <Button onclick={submit}><Wrench />Start fix</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
