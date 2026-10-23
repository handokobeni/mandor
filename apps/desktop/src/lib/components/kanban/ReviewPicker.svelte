<script lang="ts">
  // Dialog Start (brief §4.5): sumber desain dan skill reviewer, diingat per mesin.
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import CheckList from '#lib/components/settings/CheckList.svelte';
  import * as Select from '#lib/components/ui/select/index.ts';
  import { Label } from '#lib/components/ui/label/index.ts';
  import type { ReviewChoice } from '#lib/planning.ts';
  import type { DesignSourceView } from '#lib/protocol/DesignSourceView.ts';
  import type { SkillView } from '#lib/protocol/SkillView.ts';
  import { nav } from '#lib/stores/nav.svelte.ts';

  interface Props {
    sources: DesignSourceView[] | null;
    skills: SkillView[] | null;
    dropped: string[];
    choice: ReviewChoice;
  }
  let { sources, skills, dropped, choice = $bindable() }: Props = $props();

  const BUILT_IN = '__builtin__';
</script>

<div class="grid gap-1.5">
  <span class="text-sm font-medium">Design sources <span class="muted font-normal">(optional)</span></span>
  {#if sources === null}
    <p class="muted text-[length:var(--text-sm)]">Loading…</p>
  {:else if sources.length === 0}
    <p class="muted text-[length:var(--text-sm)]">
      No design sources yet.
      <button class="link" type="button" onclick={() => nav.settings('design-sources')}>Add one in Settings</button>
    </p>
  {:else}
    <CheckList
      items={sources.map((s) => ({
        id: s.id,
        label: s.name,
        hint: s.ready ? s.path : `${s.status} · ${s.path}`,
        disabled: !s.ready
      }))}
      bind:selected={choice.designSources}
      label="Design sources"
      empty=""
    />
  {/if}
  {#if dropped.length > 0}
    <p class="field-help inline-flex items-center gap-1">
      <TriangleAlert class="icon-sm text-status-warning" aria-hidden="true" />
      {dropped.join(', ')} {dropped.length === 1 ? 'is' : 'are'} missing or unreachable and no longer selected.
    </p>
  {/if}
</div>
<div class="grid gap-1.5">
  <Label for="start-reviewer">AC reviewer</Label>
  <Select.Root
    type="single"
    value={choice.reviewerSkill || BUILT_IN}
    onValueChange={(v) => (choice.reviewerSkill = v === BUILT_IN ? '' : v)}
  >
    <Select.Trigger id="start-reviewer" class="w-full">{choice.reviewerSkill || 'Mandor built-in'}</Select.Trigger>
    <Select.Content>
      <Select.Item value={BUILT_IN} label="Mandor built-in" />
      {#each skills ?? [] as k (k.name)}
        <Select.Item value={k.name} label={k.name} />
      {/each}
    </Select.Content>
  </Select.Root>
</div>
