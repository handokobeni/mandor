import { describe, expect, it } from 'vitest';
import { START_STEPS, stepsAt } from '#lib/start.ts';

describe('progres Start', () => {
  it('langkah sebelum yang aktif selesai, sesudahnya menunggu', () => {
    expect(stepsAt(START_STEPS, 'starting_agent').map((s) => s.s)).toEqual(['done', 'done', 'active', 'todo']);
    expect(stepsAt(START_STEPS, 'langkah_lain').map((s) => s.s)).toEqual(['active', 'todo', 'todo', 'todo']);
  });

  it('selesai menandai semua; gagal menandai langkah aktif dengan pesan', () => {
    expect(stepsAt(START_STEPS, 'done').every((s) => s.s === 'done')).toBe(true);
    const failed = stepsAt(START_STEPS, 'creating_worktree', 'origin/main not found');
    expect(failed.map((s) => s.s)).toEqual(['done', 'error', 'todo', 'todo']);
    expect(failed[1].err).toBe('origin/main not found');
  });
});
