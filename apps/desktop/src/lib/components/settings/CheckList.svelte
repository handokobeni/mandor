<script lang="ts">
  import { Checkbox } from '#lib/components/ui/checkbox/index.ts';

  interface Props {
    items: { id: string; label: string; hint?: string }[];
    /** ID yang dicentang. */
    selected: string[];
    label: string;
    empty: string;
  }
  let { items, selected = $bindable(), label, empty }: Props = $props();

  function toggle(id: string, on: boolean): void {
    selected = on ? [...selected.filter((s) => s !== id), id] : selected.filter((s) => s !== id);
  }
</script>

<div class="repo-pick scroll-area" role="group" aria-label={label}>
  {#each items as item (item.id)}
    <label class="pick-row">
      <Checkbox checked={selected.includes(item.id)} onCheckedChange={(v) => toggle(item.id, v === true)} />
      <span class="pick-txt"><b>{item.label}</b>{#if item.hint}<span class="mono">{item.hint}</span>{/if}</span>
    </label>
  {:else}
    <p class="muted pick-note">{empty}</p>
  {/each}
</div>
