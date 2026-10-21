// Pilihan planning di dialog Start: brainstorm lalu plan, plan saja, atau langsung ngoding.
// Skill kosong = prompt bawaan Mandor (tanpa plugin).

import type { StartPlanning } from '#lib/protocol/StartPlanning.ts';

export type PlanningMode = 'brainstorm' | 'plan' | 'skip';

export interface PlanningChoice {
  mode: PlanningMode;
  brainstormSkill: string;
  planSkill: string;
}

export const DEFAULT_PLANNING: PlanningChoice = { mode: 'brainstorm', brainstormSkill: '', planSkill: '' };

/** Kunci penyimpanan pilihan terakhir (kenyamanan per mesin, bukan state penting). */
export const PLANNING_KEY = 'mandor.planning';

/** Parameter `planning` untuk `runs.start`. */
export function planningParams(c: PlanningChoice): StartPlanning {
  const skill = (s: string) => s || null;
  return {
    kind: c.mode,
    brainstorm_skill: c.mode === 'brainstorm' ? skill(c.brainstormSkill) : null,
    plan_skill: c.mode === 'skip' ? null : skill(c.planSkill)
  };
}

/** Pilihan tersimpan yang masih valid; skill yang tidak terpasang lagi menjadi bawaan. */
export function restorePlanning(saved: unknown, installed: string[]): PlanningChoice {
  if (!saved || typeof saved !== 'object') return { ...DEFAULT_PLANNING };
  const s = saved as Partial<Record<keyof PlanningChoice, unknown>>;
  if (s.mode !== 'brainstorm' && s.mode !== 'plan' && s.mode !== 'skip') return { ...DEFAULT_PLANNING };
  const skill = (v: unknown) => (typeof v === 'string' && installed.includes(v) ? v : '');
  return { mode: s.mode, brainstormSkill: skill(s.brainstormSkill), planSkill: skill(s.planSkill) };
}
