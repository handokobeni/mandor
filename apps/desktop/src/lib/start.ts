// Langkah dialog Start; daemon melaporkan langkah aktif lewat event `start.progress`.

import type { Step } from '#lib/components/mandor/StepProgress.svelte';

export interface StepDef {
  id: string;
  label: string;
}

export const START_STEPS: StepDef[] = [
  { id: 'claiming', label: 'Claiming the task in Lark' },
  { id: 'creating_worktree', label: 'Creating the worktree' },
  { id: 'starting_agent', label: 'Starting the agent' },
  { id: 'moving_status', label: 'Moving Lark status to In Progress' }
];

export const TAKEOVER_STEPS: StepDef[] = [
  { id: 'claiming', label: 'Moving the claim to you in Lark' },
  { id: 'restoring', label: 'Restoring the branch from origin' },
  { id: 'starting_agent', label: 'Starting the agent' }
];

/** Status tiap langkah saat `active` berjalan; `done` = semua selesai; `err` menandai langkah aktif gagal. */
export function stepsAt(defs: StepDef[], active: string, err?: string): Step[] {
  if (active === 'done') return defs.map((d) => ({ label: d.label, s: 'done' }));
  const at = Math.max(0, defs.findIndex((d) => d.id === active));
  return defs.map((d, i) => {
    if (i < at) return { label: d.label, s: 'done' };
    if (i > at) return { label: d.label, s: 'todo' };
    return err ? { label: d.label, s: 'error', err } : { label: d.label, s: 'active' };
  });
}
