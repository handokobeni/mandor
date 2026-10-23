<script lang="ts">
  // Gerbang Push & PR run grup (R13): kartu yang sudah di-commit lalu berubah di
  // Lark. Dev me-revert commit kartu itu atau tetap menyertakannya dengan alasan
  // yang dicatat di PR.
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import Undo2 from '@lucide/svelte/icons/undo-2';
  import { validReason } from '#lib/ac-review.ts';
  import * as Alert from '#lib/components/ui/alert/index.ts';
  import { Button } from '#lib/components/ui/button/index.ts';
  import * as Dialog from '#lib/components/ui/dialog/index.ts';
  import { Label } from '#lib/components/ui/label/index.ts';
  import { Textarea } from '#lib/components/ui/textarea/index.ts';
  import { cardKey } from '#lib/group.ts';
  import type { GroupCardView } from '#lib/protocol/GroupCardView.ts';

  interface Props {
    cards: GroupCardView[];
    busy: boolean;
    oncancel: () => void;
    ondecide: (recordId: string, decision: 'revert' | 'keep', reason: string | null) => void;
  }
  let { cards, busy, oncancel, ondecide }: Props = $props();

  let reasons = $state<Record<string, string>>({});
  let tried = $state<Record<string, boolean>>({});

  function keep(id: string): void {
    tried[id] = true;
    const reason = reasons[id] ?? '';
    if (!validReason(reason)) return;
    ondecide(id, 'keep', reason.trim());
  }
</script>

<Dialog.Root open onOpenChange={(o) => !o && oncancel()}>
  <Dialog.Content class="sm:max-w-[600px] max-h-[90vh] overflow-y-auto">
    <Dialog.Header>
      <Dialog.Title class="text-[length:var(--text-md)]">
        {cards.length === 1 ? 'A card changed in Lark' : `${cards.length} cards changed in Lark`}
      </Dialog.Title>
      <Dialog.Description>
        The code for these cards is already in the branch, but their status or PIC changed after they were committed.
        Revert a card to drop its commit, or keep it with a reason. The AC review and the PR wait until you decide.
      </Dialog.Description>
    </Dialog.Header>
    {#each cards as c (c.record_id)}
      {@const missing = tried[c.record_id] && !validReason(reasons[c.record_id] ?? '')}
      <div class="grid gap-2 rounded-md border p-3">
        <div class="flex items-center gap-2">
          <TriangleAlert class="text-status-warning size-4" aria-hidden="true" />
          <span class="mono">{cardKey(c)}</span><span class="truncate">{c.title}</span>
        </div>
        <Label for="keep-{c.record_id}">Reason to keep it <span class="muted font-normal">(written in the PR)</span></Label>
        <Textarea
          id="keep-{c.record_id}"
          rows={2}
          bind:value={reasons[c.record_id]}
          aria-invalid={missing ? 'true' : undefined}
          placeholder="e.g. QA blocked it for a test environment issue; the code is still needed"
        />
        {#if missing}<p class="text-destructive text-[length:var(--text-sm)]">Write a reason to keep the card.</p>{/if}
        <div class="flex justify-end gap-2">
          <Button variant="outline" size="sm" disabled={busy} onclick={() => ondecide(c.record_id, 'revert', null)}>
            <Undo2 />Revert its commit
          </Button>
          <Button size="sm" disabled={busy} onclick={() => keep(c.record_id)}>Keep in the PR</Button>
        </div>
      </div>
    {/each}
    <Alert.Root>
      <TriangleAlert class="text-status-warning" />
      <Alert.Description>
        A revert that conflicts with later cards is cancelled and the push stays held. Resolve it in the branch, or keep
        the card.
      </Alert.Description>
    </Alert.Root>
    <Dialog.Footer>
      <Button variant="ghost" onclick={oncancel}>Close</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
