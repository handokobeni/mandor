<script lang="ts">
  import { Label } from '#lib/components/ui/label/index.ts';
  import * as Select from '#lib/components/ui/select/index.ts';
  import * as ToggleGroup from '#lib/components/ui/toggle-group/index.ts';
  import type { PlanningChoice, PlanningMode } from '#lib/planning.ts';
  import type { SkillView } from '#lib/protocol/SkillView.ts';

  interface Props {
    /** `null` saat daftar skill masih dimuat. */
    skills: SkillView[] | null;
    choice: PlanningChoice;
  }
  let { skills, choice = $bindable() }: Props = $props();

  const BUILTIN = 'Mandor built-in';

  // Skill yang cocok dengan tahapnya di urutan atas; semua skill tetap bisa dipilih.
  function ordered(hint: string): SkillView[] {
    const list = skills ?? [];
    return [...list.filter((k) => k.name.includes(hint)), ...list.filter((k) => !k.name.includes(hint))];
  }
  const describe = (name: string) => skills?.find((k) => k.name === name)?.description ?? '';
  // Nilai kosong berarti "belum dipilih" bagi Select; prompt bawaan memakai nilai tetap.
  const asValue = (skill: string) => skill || 'builtin';
  const asSkill = (value: string) => (value === 'builtin' ? '' : value);
</script>

<div class="grid gap-1.5">
  <Label>Before coding</Label>
  <ToggleGroup.Root
    type="single"
    size="sm"
    variant="outline"
    value={choice.mode}
    onValueChange={(v) => v && (choice = { ...choice, mode: v as PlanningMode })}
    aria-label="Before coding"
  >
    <ToggleGroup.Item value="brainstorm">Brainstorm, then plan</ToggleGroup.Item>
    <ToggleGroup.Item value="plan">Plan only</ToggleGroup.Item>
    <ToggleGroup.Item value="skip">Skip planning</ToggleGroup.Item>
  </ToggleGroup.Root>
  <p class="muted text-[length:var(--text-sm)]">
    {#if choice.mode === 'brainstorm'}
      The agent reads the code and asks you what is unclear, one question at a time, in chat. Then it writes a plan you
      approve before any code is written.
    {:else if choice.mode === 'plan'}
      The agent reads the code and writes a plan you approve before any code is written.
    {:else}
      The agent writes code from the first turn.
    {/if}
  </p>
</div>

{#if choice.mode !== 'skip'}
  <div class="grid gap-3">
    {#if choice.mode === 'brainstorm'}
      <div class="grid gap-1.5 content-start">
        <Label for="start-brainstorm">Brainstorm with</Label>
        <Select.Root
          type="single"
          value={asValue(choice.brainstormSkill)}
          onValueChange={(v) => (choice = { ...choice, brainstormSkill: asSkill(v) })}
        >
          <Select.Trigger id="start-brainstorm" class="w-full min-w-0"
            ><span class="truncate" title={choice.brainstormSkill || BUILTIN}>{choice.brainstormSkill || BUILTIN}</span></Select.Trigger
          >
          <Select.Content>
            <Select.Item value="builtin" label={BUILTIN} />
            {#each ordered('brainstorm') as k (k.name)}<Select.Item value={k.name} label={k.name} />{/each}
          </Select.Content>
        </Select.Root>
        {#if choice.brainstormSkill}<p class="muted skill-desc">{describe(choice.brainstormSkill)}</p>{/if}
      </div>
    {/if}
    <div class="grid gap-1.5 content-start">
      <Label for="start-plan">Plan with</Label>
      <Select.Root
        type="single"
        value={asValue(choice.planSkill)}
        onValueChange={(v) => (choice = { ...choice, planSkill: asSkill(v) })}
      >
        <Select.Trigger id="start-plan" class="w-full min-w-0"
          ><span class="truncate" title={choice.planSkill || BUILTIN}>{choice.planSkill || BUILTIN}</span></Select.Trigger
        >
        <Select.Content>
          <Select.Item value="builtin" label={BUILTIN} />
          {#each ordered('plan') as k (k.name)}<Select.Item value={k.name} label={k.name} />{/each}
        </Select.Content>
      </Select.Root>
      {#if choice.planSkill}<p class="muted skill-desc">{describe(choice.planSkill)}</p>{/if}
    </div>
  </div>
  {#if skills !== null && skills.length === 0}
    <p class="muted text-[length:var(--text-sm)]">
      No Claude plugin skills on the daemon machine, so Mandor's built-in prompts are used.
    </p>
  {/if}
{/if}
