import { describe, expect, it } from 'vitest';
import {
  reviewMeta,
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
  validReason,
  NO_VISUAL,
  visualMeta,
  cantReasons,
  gateRows,
  visualLinks
} from '#lib/ac-review.ts';
import type { AcRowView } from '#lib/protocol/AcRowView.ts';

function row(id: string, verdict: string, extra: Partial<AcRowView> = {}): AcRowView {
  return {
    ...NO_VISUAL,
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

describe('keterangan review terakhir', () => {
  it('tanpa riwayat: verdict yang ada berasal dari VM; tanpa verdict sama sekali: belum direview', () => {
    expect(reviewMeta([row('1', 'pass'), row('2', 'not_reviewed')])).toBe('Verdicts from the VM review');
    expect(reviewMeta([row('1', 'not_reviewed')])).toBe('No review yet');
    expect(reviewMeta([])).toBe('No review yet');
  });
});

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

  it('label tombol push mengikuti ada tidaknya PR', () => {
    const ready = gate([row('1', 'pass')], 'block', false);
    expect(gateButtonLabel(ready)).toBe('Push & create PR');
    expect(gateButtonLabel(ready, true)).toBe('Push to PR');
    const review = gate([row('1', 'not_reviewed')], 'block', false);
    expect(gateButtonLabel(review, true)).toBe('Push to PR');
    expect(gateButtonLabel(gate([row('1', 'fail')], 'block', false), true)).toBe('Push blocked');
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

describe('hasil tampilan dengan prototype (plan 2026-11-03-002)', () => {
  const look = (verdict: string, visual: string, extra: Partial<AcRowView> = {}) =>
    row('v', verdict, { ...NO_VISUAL, visual, ...extra });

  it('counts a passing row that looks different as failed and a row whose look was not verified as cant verify', () => {
    const c = counts([look('pass', 'differs'), look('pass', 'cant_verify'), look('pass', 'accepted'), look('pass', 'match')]);
    expect([c.pass, c.fail, c.cant]).toEqual([2, 1, 1]);
    expect(gate([look('pass', 'differs')], 'block', false).state).toBe('blocked');
    expect(gate([look('pass', 'cant_verify')], 'block', false).state).toBe('needs_confirmation');
  });

  it('lets a row that only looks different be sent to the agent', () => {
    expect(canSend(look('pass', 'differs', { selectable: true }))).toBe(true);
    expect(canSend(look('pass', 'match', { selectable: false }))).toBe(false);
  });

  it('labels the look chip and hides it when the row was not compared', () => {
    expect(visualMeta('differs')?.label).toBe('Looks different');
    expect(visualMeta('accepted')?.label).toBe('Difference accepted');
    expect(visualMeta('not_compared')).toBeNull();
  });

  it('collects rows the gate dialog lists for push anyway and manual checks', () => {
    const rows = [look('pass', 'differs'), { ...look('cant_verify', 'match'), ac_id: 'c' }, { ...look('pass', 'cant_verify'), ac_id: 'd' }];
    expect(gateRows(rows).fails.map((r) => r.ac_id)).toEqual(['v']);
    expect(gateRows(rows).cants.map((r) => r.ac_id)).toEqual(['c', 'd']);
  });

  it('explains why the gate asks: behaviour evidence or the reason the look was not verified', () => {
    const lookOnly = look('pass', 'cant_verify', {
      evidence: 'Code renders the marker.',
      visual_detail: { ...NO_VISUAL.visual_detail, description: 'Demo data has no critical job.' }
    });
    expect(cantReasons(lookOnly)).toEqual(['Look not verified: Demo data has no critical job.']);
    expect(cantReasons(look('pass', 'cant_verify'))).toEqual(['Look not verified']);
    const both = look('cant_verify', 'cant_verify', {
      evidence: 'Needs a database.',
      visual_detail: { ...NO_VISUAL.visual_detail, description: 'Prototype page missing.' }
    });
    expect(cantReasons(both)).toEqual(['Needs a database.', 'Look not verified: Prototype page missing.']);
    expect(cantReasons(look('cant_verify', 'match', { evidence: 'Needs a database.' }))).toEqual(['Needs a database.']);
  });

  it('builds page links on the running app and prototype only for same-origin paths', () => {
    const r = look('pass', 'differs', {
      visual_detail: { ...NO_VISUAL.visual_detail, app_path: '/jabatan?x=1', prototype_path: '/proto/jabatan' }
    });
    expect(visualLinks(r, 'http://localhost:3000', 'http://127.0.0.1:3001')).toEqual({
      app: 'http://localhost:3000/jabatan?x=1',
      prototype: 'http://127.0.0.1:3001/proto/jabatan'
    });
    const bad = look('pass', 'differs', {
      visual_detail: { ...NO_VISUAL.visual_detail, app_path: '//evil.example/x', prototype_path: '/a' }
    });
    expect(visualLinks(bad, 'http://localhost:3000', null)).toEqual({ app: null, prototype: null });
  });
});
