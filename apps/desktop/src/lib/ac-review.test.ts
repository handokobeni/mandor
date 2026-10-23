import { describe, expect, it } from 'vitest';
import {
  acTitle,
  canSend,
  counts,
  evidenceParts,
  gate,
  gateButtonLabel,
  moveFocus,
  noDesignSource,
  publishParams,
  rowMarks,
  shownVerdict,
  tabCount,
  toggleSelection,
  validReason
} from '#lib/ac-review.ts';
import type { AcRowView } from '#lib/protocol/AcRowView.ts';

function row(id: string, verdict: string, extra: Partial<AcRowView> = {}): AcRowView {
  return {
    ac_id: id,
    label: `AC-${id}`,
    text: `teks ${id}`,
    design: 'not_checked',
    design_note: null,
    verdict,
    evidence: null,
    stale: false,
    status: 'active',
    reason: null,
    decided_by: null,
    in_gate: true,
    selectable: verdict === 'fail',
    card: null,
    ...extra
  };
}

describe('gerbang AC', () => {
  it('state untuk setiap hasil, termasuk reviewing dan mode peringatan', () => {
    expect(gate([], 'block', false).state).toBe('no_ac');
    expect(gate([row('1', 'pass')], 'block', true).state).toBe('reviewing');
    expect(gate([row('1', 'not_reviewed')], 'block', false).state).toBe('review_needed');
    expect(gate([row('1', 'pass', { stale: true })], 'block', false).text).toContain('code changed');
    expect(gate([row('1', 'fix_in_progress')], 'block', false).state).toBe('review_needed');
    const blocked = gate([row('1', 'pass'), row('2', 'fail')], 'block', false);
    expect(blocked.state).toBe('blocked');
    expect(blocked.text).toBe('Push blocked by 1 AC');
    expect(gateButtonLabel(blocked)).toBe('Push blocked');
    expect(gate([row('1', 'cant_verify')], 'block', false).state).toBe('needs_confirmation');
    expect(gate([row('1', 'pass'), row('2', 'checked_manually'), row('3', 'skipped')], 'block', false).state).toBe(
      'ready'
    );
  });

  it('mode peringatan dengan Fail tidak pernah Blocked', () => {
    const g = gate([row('1', 'fail'), row('2', 'cant_verify')], 'warn', false);
    expect(g.state).toBe('warn');
    expect(g.warnings).toBe(2);
    expect(gateButtonLabel(g)).toBe('Push with 2 warnings');
    expect(g.text).toBe("1 AC failed, 1 can't be verified (warn mode)");
  });

  it('hitungan tab "5/7" dan penanda gagal; baris Removed tidak dihitung', () => {
    const rows = [
      row('1', 'pass'),
      row('2', 'pass'),
      row('3', 'checked_manually'),
      row('4', 'skipped'),
      row('5', 'not_reviewed', { design: 'follow_design', in_gate: false }),
      row('6', 'fail'),
      row('7', 'cant_verify'),
      row('8', 'pass', { status: 'removed', in_gate: false })
    ];
    expect(tabCount(rows)).toEqual({ text: '5/7', failed: 1 });
    expect(counts(rows).total).toBe(7);
    expect(shownVerdict(rows[4])).toBe('follows_design');
    expect(tabCount([])).toBeNull();
  });
});

describe('pemilihan dan navigasi', () => {
  it('hanya baris Fail yang bisa dikirim ke agent', () => {
    expect(canSend(row('1', 'fail'))).toBe(true);
    expect(canSend(row('1', 'pass'))).toBe(false);
    expect(canSend(row('1', 'fail', { design: 'follow_design', in_gate: false, selectable: false }))).toBe(false);
    expect(canSend(row('1', 'fail', { stale: true }))).toBe(false);
    let sel = toggleSelection(new Set(), row('1', 'fail'));
    expect([...sel]).toEqual(['1']);
    sel = toggleSelection(sel, row('2', 'pass'));
    expect([...sel]).toEqual(['1']);
    sel = toggleSelection(sel, row('1', 'fail'));
    expect(sel.size).toBe(0);
  });

  it('panah memindah fokus dalam batas, Home dan End ke ujung', () => {
    expect(moveFocus(0, 'ArrowDown', 3)).toBe(1);
    expect(moveFocus(2, 'ArrowDown', 3)).toBe(2);
    expect(moveFocus(0, 'ArrowUp', 3)).toBe(0);
    expect(moveFocus(1, 'End', 3)).toBe(2);
    expect(moveFocus(2, 'Home', 3)).toBe(0);
    expect(moveFocus(1, 'x', 3)).toBe(1);
  });

  it('penanda New, Changed, Removed dan No design source', () => {
    expect(rowMarks(row('1', 'not_reviewed', { status: 'new' }))).toEqual(['new']);
    expect(rowMarks(row('1', 'not_reviewed', { status: 'changed' }))).toEqual(['changed']);
    expect(rowMarks(row('1', 'pass', { status: 'removed' }))).toEqual(['removed']);
    expect(rowMarks(row('1', 'pass'))).toEqual([]);
    expect(noDesignSource({ design_sources: [] })).toBe(true);
    expect(noDesignSource({ design_sources: ['kelola-mock'] })).toBe(false);
  });
});

describe('dialog gerbang', () => {
  it('alasan kosong atau spasi ditolak; publish memuat alasan dan konfirmasi', () => {
    expect(validReason('   ')).toBe(false);
    expect(validReason('API dependency down')).toBe(true);
    expect(publishParams('r1', ['ac2'], '  API dependency down ')).toEqual({
      run_id: 'r1',
      confirmed: ['ac2'],
      push_anyway_reason: 'API dependency down'
    });
    expect(publishParams('r1', [], '  ')).toEqual({ run_id: 'r1' });
  });

  it('bukti dipecah menjadi teks dan rujukan berkas:baris', () => {
    expect(evidenceParts('See kelola-web/src/App.svelte:42 and tests pass')).toEqual([
      { kind: 'text', text: 'See ' },
      { kind: 'file', path: 'kelola-web/src/App.svelte', line: 42, text: 'kelola-web/src/App.svelte:42' },
      { kind: 'text', text: ' and tests pass' }
    ]);
    expect(evidenceParts('<img src=x onerror=alert(1)>')).toEqual([
      { kind: 'text', text: '<img src=x onerror=alert(1)>' }
    ]);
  });
});

describe('acTitle', () => {
  it('mengambil baris pertama teks AC (judul sebelum skenario Gherkin)', () => {
    expect(acTitle('AC-1 — Fibonacci\nGiven pengguna membuka halaman\nWhen …')).toBe('AC-1 — Fibonacci');
    expect(acTitle('  Satu baris saja  ')).toBe('Satu baris saja');
    expect(acTitle('\nGiven tanpa judul')).toBe('Given tanpa judul');
  });
});
