<script lang="ts">
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import Plus from '@lucide/svelte/icons/plus';
  import X from '@lucide/svelte/icons/x';
  import { Button } from '#lib/components/ui/button/index.ts';
  import { Input } from '#lib/components/ui/input/index.ts';
  import { type RepoList, validateListItem } from '#lib/settings.ts';

  interface Props {
    list: RepoList;
    label: string;
    items: string[];
    placeholder: string;
  }
  let { list, label, items = $bindable(), placeholder }: Props = $props();

  let draft = $state('');
  let error = $state<string | null>(null);

  function add(): void {
    error = validateListItem(list, draft, items);
    if (error) return;
    items = [...items, draft.trim()];
    draft = '';
  }
</script>

<div class="list-editor">
  {#each items as item, i (item)}
    <div class="item">
      <span>{item}</span>
      <Button
        variant="ghost"
        size="icon-xs"
        aria-label="Remove {item}"
        onclick={() => (items = items.filter((_, j) => j !== i))}><X /></Button
      >
    </div>
  {/each}
  <div class="flex gap-2">
    <Input
      class="h-7 font-mono text-xs"
      {placeholder}
      aria-label="Add to {label}"
      aria-invalid={error ? 'true' : undefined}
      bind:value={draft}
      onkeydown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          add();
        }
      }}
    />
    <Button variant="outline" size="sm" onclick={add}><Plus />Add</Button>
  </div>
  {#if error}
    <p class="field-error"><CircleAlert class="icon-sm" aria-hidden="true" />{error}</p>
  {/if}
</div>
