import { describe, expect, it } from 'vitest';
import {
  changedCards,
  groupAction,
  headerCard,
  initialOrder,
  moveCard,
  progress,
  progressText,
  rowsByCard,
  stateLabel,
  toggleCard
} from '#lib/group.ts';
import type { AcRowView } from '#lib/protocol/AcRowView.ts';
import type { GroupCandidateView } from '#lib/protocol/GroupCandidateView.ts';
import type { GroupCardView } from '#lib/protocol/GroupCardView.ts';

function card(id: string, state: string, extra: Partial<GroupCardView> = {}): GroupCardView {
  return {
    record_id: id,
    key: `PGS-${id}`,
    title: `Judul ${id}`,
    state,
    changed_in_lark: false,
    kept_reason: null,
    in_fix: false,
    ...extra
  };
}

function cand(id: string, blocked: string | null = null): GroupCandidateView {
  return { record_id: id, key: `PGS-${id}`, title: `Judul ${id}`, blocked };
}

function row(ac: string, cardId: string | null): AcRowView {
  return {
    ac_id: ac,
    label: ac.toUpperCase(),
    text: 'x',
    design: 'not_checked',
    design_note: null,
    verdict: 'pass',
    evidence: null,
    stale: false,
    status: 'active',
    reason: null,
    decided_by: null,
    in_gate: true,
    selectable: false,
    card: cardId
  };
}

describe('progres grup', () => {
  it('menghitung kartu per state', () => {
    const cards = [card('1401', 'done'), card('1402', 'done'), card('1403', 'active'), card('1404', 'pending')];
    expect(progress(cards)).toEqual({ done: 2, active: 1, pending: 1, removed: 0, total: 4 });
    expect(progressText(progress(cards))).toBe('2 of 4 done · 1 active · 1 waiting');
  });

  it('kartu dikeluarkan tidak dihitung dalam total', () => {
    const cards = [card('1', 'done'), card('2', 'removed_changed'), card('3', 'pending')];
    expect(progressText(progress(cards))).toBe('1 of 2 done · 1 waiting · 1 removed');
  });
});

describe('aksi grup', () => {
  const base = { state: 'awaiting_dev', phase: 'implement' };

  it('Next card bila masih ada kartu berikutnya', () => {
    expect(groupAction({ ...base, cards: [card('1', 'active'), card('2', 'pending')] })).toBe('next');
  });

  it('Finish pada kartu terakhir', () => {
    expect(groupAction({ ...base, cards: [card('1', 'done'), card('2', 'active')] })).toBe('finish');
  });

  it('Continue setelah kartu dikeluarkan tanpa kartu aktif', () => {
    expect(
      groupAction({ ...base, cards: [card('1', 'done'), card('2', 'removed_changed'), card('3', 'pending')] })
    ).toBe('continue');
  });

  it('tidak ada aksi grup saat agent bekerja, plan belum disetujui, semua selesai, atau run tunggal', () => {
    const cards = [card('1', 'active'), card('2', 'pending')];
    expect(groupAction({ ...base, state: 'running', cards })).toBeNull();
    expect(groupAction({ ...base, phase: 'plan', cards })).toBeNull();
    expect(groupAction({ ...base, cards: [card('1', 'done'), card('2', 'done')] })).toBeNull();
    expect(groupAction({ ...base, cards: [] })).toBeNull();
  });
});

describe('pemilihan kartu', () => {
  it('awalnya semua kandidat yang bisa dipilih, urut kandidat', () => {
    expect(initialOrder([cand('1401'), cand('1402', 'Task already has an active run'), cand('1403')])).toEqual([
      '1401',
      '1403'
    ]);
  });

  it('urutan dev dipertahankan saat kartu dicentang ulang', () => {
    let order = ['1401', '1402', '1403'];
    order = moveCard(order, '1403', -1);
    expect(order).toEqual(['1401', '1403', '1402']);
    order = toggleCard(order, '1401');
    expect(order).toEqual(['1403', '1402']);
    order = toggleCard(order, '1401');
    expect(order).toEqual(['1403', '1402', '1401']);
    expect(moveCard(order, '1403', -1)).toEqual(order);
  });
});

describe('judul workspace grup', () => {
  it('kartu aktif, atau kartu utama bila tidak ada yang aktif', () => {
    const run = { key: 'PGS-1', title: 'Utama', record_id: '1' };
    const cards = [card('1', 'done'), card('2', 'active'), card('3', 'pending')];
    expect(headerCard({ ...run, cards })).toEqual({ key: 'PGS-2', title: 'Judul 2' });
    expect(headerCard({ ...run, cards: [card('1', 'done'), card('2', 'done')] })).toEqual({ key: 'PGS-1', title: 'Utama' });
    expect(headerCard({ ...run, key: null, cards: [] })).toEqual({ key: '1', title: 'Utama' });
  });
});

describe('teks kartu', () => {
  it('label state dan kartu yang menunggu keputusan', () => {
    expect(stateLabel('removed_reverted')).toBe('Reverted');
    expect(stateLabel('pending')).toBe('Waiting');
    const cards = [
      card('1', 'done', { changed_in_lark: true }),
      card('2', 'done', { changed_in_lark: true, kept_reason: 'QA' }),
      card('3', 'done')
    ];
    expect(changedCards(cards).map((c) => c.record_id)).toEqual(['1']);
  });

  it('baris AC dikelompokkan per kartu sesuai urutan grup', () => {
    const cards = [card('1', 'done'), card('2', 'done')];
    const groups = rowsByCard([row('ac2', '2'), row('ac1', '1'), row('ac9', null)], cards);
    expect(groups.map((g) => [g.card?.record_id ?? null, g.rows.map((r) => r.ac_id)])).toEqual([
      ['1', ['ac1']],
      ['2', ['ac2']],
      [null, ['ac9']]
    ]);
  });
});
