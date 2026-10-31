<script lang="ts">
  // Sumber desain VM untuk kartu yang diantre (plan 2026-10-30-001, R11).
  import ChevronsUpDown from '@lucide/svelte/icons/chevrons-up-down';
  import CheckList from '#lib/components/settings/CheckList.svelte';
  import * as Popover from '#lib/components/ui/popover/index.ts';
  import { designPickerLabel } from '#lib/overnight.ts';
  import type { DesignSourceView } from '#lib/protocol/DesignSourceView.ts';
  import { nav } from '#lib/stores/nav.svelte.ts';

  let { sources, selected = $bindable() }: { sources: DesignSourceView[]; selected: string[] } = $props();
  let open = $state(false);
  const label = $derived(designPickerLabel(sources, selected));
</script>

<Popover.Root bind:open>
  <Popover.Trigger
    class="border-input dark:bg-input/30 dark:hover:bg-input/50 flex h-8 w-52 max-w-full items-center justify-between gap-1.5 rounded-lg border bg-transparent py-2 pr-2 pl-2.5 text-sm"
    aria-label="Design sources on the VM"
  >
    <span class="min-w-0 truncate" class:placeholder={selected.length === 0}>{label}</span>
    <ChevronsUpDown class="text-muted-foreground size-4 shrink-0" />
  </Popover.Trigger>
  <Popover.Content class="w-72" align="start">
    {#if sources.length === 0}
      <p class="muted text-[length:var(--text-sm)]">
        No design sources on the VM.
        <button class="link" type="button" onclick={() => nav.settings('vm')}>Add one in Settings › VM</button>
      </p>
    {:else}
      <CheckList
        items={sources.map((s) => ({
          id: s.id,
          label: s.name,
          hint: s.ready ? s.path : `${s.status} · ${s.path}`,
          disabled: !s.ready
        }))}
        bind:selected
        label="Design sources on the VM"
        empty=""
      />
    {/if}
  </Popover.Content>
</Popover.Root>

<style>
  .placeholder {
    color: var(--muted-foreground);
  }
</style>
