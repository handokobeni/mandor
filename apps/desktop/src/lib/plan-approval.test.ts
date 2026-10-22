import { describe, expect, it } from 'vitest';
import { followCandidates, toggleFollow } from '#lib/plan-approval.ts';
import type { AcRowView } from '#lib/protocol/AcRowView.ts';

function row(id: string, design: string, status = 'active'): AcRowView {
  return {
    ac_id: id,
    label: `AC-${id}`,
    text: id,
    design,
    design_note: null,
    verdict: 'not_reviewed',
    evidence: null,
    stale: false,
    status,
    reason: null,
    decided_by: null,
    in_gate: true,
    selectable: false
  };
}

describe('persetujuan plan', () => {
  it('hanya baris Missing, Partly, dan Conflicts; Undo mengembalikan daftar', () => {
    const rows = [row('1', 'in_design'), row('2', 'missing'), row('3', 'partly'), row('4', 'conflicts'), row('5', 'not_checked'), row('6', 'missing', 'removed')];
    expect(followCandidates(rows).map((r) => r.ac_id)).toEqual(['2', '3', '4']);
    let follow = toggleFollow([], '4');
    expect(follow).toEqual(['4']);
    follow = toggleFollow(follow, '4');
    expect(follow).toEqual([]);
  });
});
