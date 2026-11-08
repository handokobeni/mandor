// Langkah dialog Start; daemon melaporkan langkah aktif lewat event `start.progress`.

import type { Step } from '#lib/components/mandor/StepProgress.svelte';
import type { RunView } from '#lib/protocol/RunView.ts';
import type { Rpc } from '#lib/rpc.ts';

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

/**
 * Batas tunggu Start, Start grup, dan Take over. Klaim banyak kartu di Lark
 * dan fetch repo besar bisa jauh lebih lama dari batas bawaan 30 detik.
 */
export const START_TIMEOUT_MS = 5 * 60_000;

/** Pesan bila Start melewati batas tunggu dan run-nya belum terlihat. */
export const START_STILL_RUNNING =
  'Mandor is taking longer than usual to start this card. It may still finish: refresh the board in a moment before starting again.';

/**
 * Run terbuka untuk salah satu kartu ini, bila ada. Dipakai setelah Start
 * melewati batas tunggu: daemon bisa saja sudah membuat run-nya.
 */
export async function findStartedRun(rpc: Rpc, recordIds: string[]): Promise<string | null> {
  try {
    const runs = await rpc.request<RunView[]>('runs.list');
    const run = runs.find(
      (r) => recordIds.includes(r.record_id) || r.cards.some((c) => recordIds.includes(c.record_id))
    );
    return run?.id ?? null;
  } catch {
    return null;
  }
}
