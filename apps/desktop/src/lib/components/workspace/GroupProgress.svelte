<script lang="ts">
  // Progres run grup (plan run grup U9): kartu dalam urutan pengerjaan dengan
  // state, tanda berubah di Lark atau Waiting Fix, dan Remove untuk kartu yang
  // belum dimulai.
  import Layers from '@lucide/svelte/icons/layers';
  import { Badge } from '#lib/components/ui/badge/index.ts';
  import { Button } from '#lib/components/ui/button/index.ts';
  import { cardKey, isRemoved, progress, progressText, stateLabel } from '#lib/group.ts';
  import type { GroupCardView } from '#lib/protocol/GroupCardView.ts';

  interface Props {
    cards: GroupCardView[];
    busy: boolean;
    onremove: (recordId: string) => void;
  }
  let { cards, busy, onremove }: Props = $props();

  const summary = $derived(progressText(progress(cards)));

  function tone(c: GroupCardView): string {
    if (isRemoved(c)) return 'muted';
    if (c.state === 'active') return 'working';
    if (c.state === 'done') return 'done';
    return 'waiting';
  }
</script>

<section class="group-progress" aria-label="Cards in this group">
  <div class="gp-title">
    <Layers class="icon-sm" aria-hidden="true" />
    <b>Group</b>
    <span class="muted">{summary}</span>
  </div>
  <ol class="gp-list">
    {#each cards as c, i (c.record_id)}
      <li class="gp-card" data-tone={tone(c)}>
        <span class="mono muted">{i + 1}.</span>
        <span class="mono">{cardKey(c)}</span>
        <span class="gp-name" class:removed={isRemoved(c)}>{c.title}</span>
        <Badge variant="outline">{stateLabel(c.state)}</Badge>
        {#if c.changed_in_lark && !c.kept_reason}
          <Badge variant="outline" class="text-status-warning">Changed in Lark</Badge>
        {:else if c.kept_reason}
          <Badge variant="outline" title={c.kept_reason}>Kept after change</Badge>
        {/if}
        {#if c.in_fix}<Badge variant="outline" class="text-status-warning">Waiting Fix</Badge>{/if}
        {#if c.state === 'pending'}
          <Button variant="ghost" size="xs" class="ml-auto" disabled={busy} onclick={() => onremove(c.record_id)}>Remove</Button>
        {/if}
      </li>
    {/each}
  </ol>
</section>

<style>
  .group-progress { padding: 10px 16px 0; display: grid; gap: 6px; }
  .gp-title { display: flex; align-items: center; gap: 8px; font-size: var(--text-sm); }
  .gp-list { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  .gp-card { display: flex; align-items: center; gap: 8px; min-height: 30px; padding: 2px 8px; border-radius: var(--radius-sm); font-size: var(--text-sm); }
  .gp-card[data-tone='working'] { background: var(--secondary); box-shadow: inset 2px 0 0 var(--primary); }
  .gp-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; flex: 1; }
  .gp-name.removed { text-decoration: line-through; opacity: 0.6; }
</style>
