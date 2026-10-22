<script lang="ts">
  // Dialog gerbang Push & PR (brief §4.3): konfirmasi cek manual, Push anyway
  // (mode blokir, sekaligus konfirmasi Can't verify), dan peringatan (mode warn).
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import CircleQuestionMark from '@lucide/svelte/icons/circle-question-mark';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import GitPullRequestArrow from '@lucide/svelte/icons/git-pull-request-arrow';
  import { acTitle, label, publishParams, validReason } from '#lib/ac-review.ts';
  import * as Alert from '#lib/components/ui/alert/index.ts';
  import { Button } from '#lib/components/ui/button/index.ts';
  import { Checkbox } from '#lib/components/ui/checkbox/index.ts';
  import * as Dialog from '#lib/components/ui/dialog/index.ts';
  import { Label } from '#lib/components/ui/label/index.ts';
  import { Textarea } from '#lib/components/ui/textarea/index.ts';
  import type { AcRowView } from '#lib/protocol/AcRowView.ts';
  import type { PublishParams } from '#lib/protocol/PublishParams.ts';

  interface Props {
    mode: 'manual' | 'anyway' | 'warn';
    runId: string;
    rows: AcRowView[];
    syncWarning: string | null;
    oncancel: () => void;
    onconfirm: (params: PublishParams) => void;
  }
  let { mode, runId, rows, syncWarning, oncancel, onconfirm }: Props = $props();

  const fails = $derived(rows.filter((r) => r.in_gate && !r.stale && r.verdict === 'fail'));
  const cants = $derived(rows.filter((r) => r.in_gate && !r.stale && r.verdict === 'cant_verify'));
  let checked = $state<Record<string, boolean>>({});
  let reason = $state('');
  let tried = $state(false);

  const allChecked = $derived(cants.every((r) => checked[r.ac_id]));
  const reasonMissing = $derived(tried && !validReason(reason));

  function confirm(): void {
    tried = true;
    if (mode === 'warn') return onconfirm(publishParams(runId, [], null));
    if (mode === 'anyway' && !validReason(reason)) return;
    if (!allChecked) return;
    onconfirm(
      publishParams(
        runId,
        cants.map((r) => r.ac_id),
        mode === 'anyway' ? reason : null
      )
    );
  }

  const title = $derived(
    mode === 'manual'
      ? "Confirm the AC the reviewer couldn't verify"
      : mode === 'anyway'
        ? `Push with ${fails.length} failing AC?`
        : `Push with ${fails.length + cants.length} warning${fails.length + cants.length === 1 ? '' : 's'}?`
  );
</script>

<Dialog.Root open onOpenChange={(o) => !o && oncancel()}>
  <Dialog.Content class="sm:max-w-[560px] max-h-[90vh] overflow-y-auto">
    <Dialog.Header>
      <Dialog.Title class="text-[length:var(--text-md)]">{title}</Dialog.Title>
      <Dialog.Description>
        {#if mode === 'manual'}
          Check each one yourself before pushing. Your confirmation is recorded in the AC ledger and in the PR description.
        {:else if mode === 'anyway'}
          These rows become Skipped. Your reason is written next to them in the PR description.
        {:else}
          This run uses warn mode, so these do not block the push. They are listed in the PR description.
        {/if}
      </Dialog.Description>
    </Dialog.Header>
    {#if syncWarning}
      <Alert.Root>
        <CircleAlert class="text-status-warning" />
        <Alert.Title>Couldn't refresh the AC from Lark just now</Alert.Title>
        <Alert.Description>{syncWarning}. The PR records when the AC were last synced.</Alert.Description>
      </Alert.Root>
    {/if}
    {#if mode !== 'manual' && fails.length > 0}
      <div class="gate-list">
        {#each fails as r (r.ac_id)}
          <div class="gate-row">
            <CircleX class="gate-x" aria-hidden="true" />
            <span>
              <span class="mono">{label(r)}</span> <span title={r.text}>{acTitle(r.text)}</span>
              {#if r.evidence}<span class="muted gate-why" title={r.evidence}>Reviewer found: {r.evidence}</span>{/if}
            </span>
          </div>
        {/each}
      </div>
    {/if}
    {#if mode === 'anyway'}
      <div class="grid gap-1.5">
        <Label for="gd-reason">Reason <span class="muted font-normal">(required, covers all of them)</span></Label>
        <Textarea
          id="gd-reason"
          rows={3}
          bind:value={reason}
          aria-invalid={reasonMissing ? 'true' : undefined}
          aria-describedby={reasonMissing ? 'gd-err' : undefined}
          placeholder="e.g. AC-3 is handled in a follow-up task agreed with the PM."
        />
        {#if reasonMissing}
          <p class="field-error" id="gd-err"><CircleAlert class="icon-sm" aria-hidden="true" />Write a reason before pushing.</p>
        {/if}
      </div>
    {/if}
    {#if cants.length > 0}
      <div class="grid gap-1.5">
        {#if mode === 'anyway'}<span class="text-sm font-medium">Also not verified</span>{/if}
        <div class="gate-list">
          {#each cants as r (r.ac_id)}
            {#if mode === 'warn'}
              <div class="gate-row">
                <CircleQuestionMark class="gate-q" aria-hidden="true" />
                <span><span class="mono">{label(r)}</span> <span title={r.text}>{acTitle(r.text)}</span>{#if r.evidence}<span class="muted gate-why" title={r.evidence}>{r.evidence}</span>{/if}</span>
              </div>
            {:else}
              <label class="gate-row">
                <Checkbox
                  checked={!!checked[r.ac_id]}
                  onCheckedChange={(v) => (checked[r.ac_id] = v === true)}
                  aria-label="I checked {label(r)} myself"
                />
                <span>
                  <span class="mono">{label(r)}</span> <span title={r.text}>{acTitle(r.text)}</span>
                  {#if r.evidence}<span class="muted gate-why" title={r.evidence}>{r.evidence}</span>{/if}
                  <span class="gate-ck">I checked this myself</span>
                </span>
              </label>
            {/if}
          {/each}
        </div>
        {#if tried && !allChecked && mode !== 'warn'}
          <p class="field-error"><CircleAlert class="icon-sm" aria-hidden="true" />Confirm every AC the reviewer couldn't verify.</p>
        {/if}
      </div>
    {/if}
    <Dialog.Footer>
      <Button variant="ghost" onclick={oncancel}>Cancel</Button>
      {#if mode === 'anyway'}
        <Button variant="destructive" onclick={confirm}>Push anyway</Button>
      {:else}
        <Button disabled={mode === 'manual' && !allChecked} onclick={confirm}>
          <GitPullRequestArrow />{mode === 'manual' ? 'Confirm and push' : 'Push & create PR'}
        </Button>
      {/if}
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
