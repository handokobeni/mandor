<script lang="ts">
  // Reviewer tepercaya dipilih dari orang yang dikenal daemon (`people.known`)
  // dan ditampilkan dengan nama; open_id hanya untuk orang yang belum dikenal.
  import Plus from '@lucide/svelte/icons/plus';
  import UserPlus from '@lucide/svelte/icons/user-plus';
  import X from '@lucide/svelte/icons/x';
  import { Button } from '#lib/components/ui/button/index.ts';
  import * as Command from '#lib/components/ui/command/index.ts';
  import * as Popover from '#lib/components/ui/popover/index.ts';
  import type { Rpc } from '#lib/rpc.ts';
  import { type KnownPerson, reviewerCandidates, reviewerLabel, validateListItem } from '#lib/settings.ts';

  let { rpc, items = $bindable() }: { rpc: Rpc; items: string[] } = $props();

  let known = $state<KnownPerson[]>([]);
  let open = $state(false);
  let query = $state('');

  $effect(() => {
    const r = rpc;
    r.request<KnownPerson[]>('people.known')
      .then((list) => {
        if (r === rpc) known = list;
      })
      .catch(() => {
        // Tanpa sesi Lark atau scope Contact: open_id masih bisa ditempel.
      });
  });

  const candidates = $derived(reviewerCandidates(known, items, query));
  // open_id yang ditempel langsung, untuk orang yang belum pernah muncul di Base.
  const pasted = $derived(validateListItem('reviewers', query, items) === null ? query.trim() : null);

  function add(openId: string): void {
    items = [...items, openId];
    query = '';
    open = false;
  }
</script>

<div class="list-editor">
  {#each items as item, i (item)}
    <div class="item">
      <span title={item}>{reviewerLabel(known, item)}</span>
      <Button
        variant="ghost"
        size="icon-xs"
        aria-label="Remove {reviewerLabel(known, item)}"
        onclick={() => (items = items.filter((_, j) => j !== i))}><X /></Button
      >
    </div>
  {/each}
  <Popover.Root bind:open>
    <Popover.Trigger>
      {#snippet child({ props })}
        <Button {...props} variant="outline" size="sm" class="w-fit"><UserPlus />Add reviewer</Button>
      {/snippet}
    </Popover.Trigger>
    <Popover.Content class="w-80 p-0" align="start">
      <Command.Root shouldFilter={false}>
        <Command.Input placeholder="Search by name" bind:value={query} />
        <Command.List>
          {#if candidates.length === 0 && !pasted}
            <Command.Empty>
              No one matches. People appear here once they are on a card in the Base; you can also paste an open_id (ou_…).
            </Command.Empty>
          {/if}
          {#each candidates as p (p.open_id)}
            <Command.Item value={p.open_id} onSelect={() => add(p.open_id)}>
              <span class="min-w-0 flex-1 truncate">{p.name}</span>
              <span class="text-muted-foreground shrink-0 font-mono text-xs">{p.open_id}</span>
            </Command.Item>
          {/each}
          {#if pasted}
            <Command.Item value={pasted} onSelect={() => add(pasted)}>
              <Plus />Add <span class="font-mono">{pasted}</span>
            </Command.Item>
          {/if}
        </Command.List>
      </Command.Root>
    </Popover.Content>
  </Popover.Root>
</div>
