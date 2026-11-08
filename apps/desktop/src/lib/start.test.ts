import { describe, expect, it } from 'vitest';
import { START_STEPS, START_TIMEOUT_MS, findStartedRun, stepsAt } from '#lib/start.ts';
import type { Rpc } from '#lib/rpc.ts';

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

describe('Start yang melewati batas tunggu', () => {
  const rpcWith = (runs: unknown[] | Error): Rpc => ({
    request: async <T>() => {
      if (runs instanceof Error) throw runs;
      return runs as T;
    },
    on: () => () => undefined
  });

  it('menemukan run kartu tunggal atau run grup yang memuat kartu itu', async () => {
    const runs = [
      { id: 'r-a', record_id: 'recA', cards: [] },
      { id: 'r-g', record_id: 'rec1', cards: [{ record_id: 'rec1' }, { record_id: 'rec2' }] }
    ];
    expect(await findStartedRun(rpcWith(runs), ['recA'])).toBe('r-a');
    expect(await findStartedRun(rpcWith(runs), ['rec2', 'rec3'])).toBe('r-g');
    expect(await findStartedRun(rpcWith(runs), ['recX'])).toBeNull();
  });

  it('daftar run gagal dimuat: dianggap belum ada', async () => {
    expect(await findStartedRun(rpcWith(new Error('down')), ['recA'])).toBeNull();
  });

  it('batas tunggu Start jauh di atas batas bawaan', () => {
    expect(START_TIMEOUT_MS).toBeGreaterThanOrEqual(5 * 60_000);
  });
});
