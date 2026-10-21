import { describe, expect, it } from 'vitest';
import { DEFAULT_PLANNING, planningParams, restorePlanning } from '#lib/planning.ts';

describe('pilihan planning', () => {
  it('bawaan: brainstorm lalu plan dengan prompt Mandor', () => {
    expect(DEFAULT_PLANNING).toEqual({ mode: 'brainstorm', brainstormSkill: '', planSkill: '' });
    expect(planningParams(DEFAULT_PLANNING)).toEqual({ kind: 'brainstorm', brainstorm_skill: null, plan_skill: null });
  });

  it('skill hanya dikirim untuk tahap yang dipakai', () => {
    const c = { mode: 'plan' as const, brainstormSkill: 'sp:brainstorming', planSkill: 'ce:plan' };
    expect(planningParams(c)).toEqual({ kind: 'plan', brainstorm_skill: null, plan_skill: 'ce:plan' });
    expect(planningParams({ ...c, mode: 'skip' })).toEqual({ kind: 'skip', brainstorm_skill: null, plan_skill: null });
  });

  it('pilihan tersimpan dipulihkan; skill yang tidak terpasang lagi kembali ke bawaan', () => {
    const saved = { mode: 'plan', brainstormSkill: 'hilang:x', planSkill: 'ce:plan' };
    expect(restorePlanning(saved, ['ce:plan'])).toEqual({ mode: 'plan', brainstormSkill: '', planSkill: 'ce:plan' });
    expect(restorePlanning('rusak', ['ce:plan'])).toEqual(DEFAULT_PLANNING);
    expect(restorePlanning({ mode: 'aneh' }, [])).toEqual(DEFAULT_PLANNING);
  });
});
