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

/** Pilihan review di dialog Start: sumber desain dan skill reviewer (R5, R14). */
export interface ReviewChoice {
  designSources: string[];
  reviewerSkill: string;
}

export const REVIEW_KEY = 'mandor.review';

/**
 * Pilihan review tersimpan yang masih valid. Sumber desain yang sudah dihapus
 * dibuang dan disebut di `dropped` untuk peringatan; skill yang tidak terpasang
 * menjadi bawaan.
 */
export function restoreReview(
  saved: unknown,
  sources: { id: string; name: string; ready: boolean }[],
  installed: string[]
): { choice: ReviewChoice; dropped: string[] } {
  const s = (saved && typeof saved === 'object' ? saved : {}) as Partial<Record<keyof ReviewChoice, unknown>>;
  const ids = Array.isArray(s.designSources) ? s.designSources.filter((x): x is string => typeof x === 'string') : [];
  const kept = ids.filter((id) => sources.some((src) => src.id === id && src.ready));
  const dropped = ids
    .filter((id) => !kept.includes(id))
    .map((id) => sources.find((src) => src.id === id)?.name ?? 'A removed design source');
  const skill = typeof s.reviewerSkill === 'string' && installed.includes(s.reviewerSkill) ? s.reviewerSkill : '';
  return { choice: { designSources: kept, reviewerSkill: skill }, dropped };
}
