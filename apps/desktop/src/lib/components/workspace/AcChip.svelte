<script lang="ts">
  // Chip Design atau Verdict: ikon dan teks, warna hanya pada ikon (tidak pernah warna saja).
  import ArrowRightLeft from '@lucide/svelte/icons/arrow-right-left';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import CircleDashed from '@lucide/svelte/icons/circle-dashed';
  import CircleQuestionMark from '@lucide/svelte/icons/circle-question-mark';
  import CircleSlash from '@lucide/svelte/icons/circle-slash';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import ClockArrowLeft from '@lucide/svelte/icons/clock-arrow-left';
  import Contrast from '@lucide/svelte/icons/contrast';
  import Forward from '@lucide/svelte/icons/forward';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import SignpostBig from '@lucide/svelte/icons/signpost-big';
  import UserRoundCheck from '@lucide/svelte/icons/user-round-check';
  import Wrench from '@lucide/svelte/icons/wrench';
  import { DESIGN_META, VERDICT_META } from '#lib/ac-review.ts';

  interface Props {
    kind: 'design' | 'verdict';
    value: string;
    stale?: boolean;
    pending?: boolean;
  }
  let { kind, value, stale = false, pending = false }: Props = $props();

  const meta = $derived((kind === 'design' ? DESIGN_META : VERDICT_META)[value] ?? { label: value, tone: 'muted' });
  const ICONS = {
    not_checked: CircleDashed,
    in_design: CircleCheck,
    partly: Contrast,
    missing: CircleSlash,
    conflicts: ArrowRightLeft,
    follow_design: SignpostBig,
    not_reviewed: CircleDashed,
    pass: CircleCheck,
    fail: CircleX,
    cant_verify: CircleQuestionMark,
    checked_manually: UserRoundCheck,
    skipped: Forward,
    fix_in_progress: Wrench,
    follows_design: SignpostBig
  } as const;
  const Icon = $derived(ICONS[value as keyof typeof ICONS] ?? CircleDashed);
</script>

{#if pending}
  <span class="chip" data-tone="working"><LoaderCircle class="spin" aria-hidden="true" />Reviewing</span>
{:else}
  <span class="chip" data-tone={meta.tone} data-stale={stale ? 'true' : undefined}>
    <Icon aria-hidden="true" />{meta.label}
    {#if stale}
      <span class="chip-stale" title="The code changed after this verdict"><ClockArrowLeft aria-hidden="true" />Stale</span>
    {/if}
  </span>
{/if}
