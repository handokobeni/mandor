import { describe, expect, it } from 'vitest';
import { counts, gate, gateRows, NO_VISUAL } from '#lib/ac-review.ts';
import {
  adviceLabel,
  emptyLabForm,
  evidenceBadges,
  labApproveParams,
  labForm,
  labNeedLabel,
  labSetParams,
  labStatus,
  validateLabForm
} from '#lib/lab.ts';
import type { AcRowView } from '#lib/protocol/AcRowView.ts';
import type { EvidenceView } from '#lib/protocol/EvidenceView.ts';
import type { LabEquipmentDto } from '#lib/protocol/LabEquipmentDto.ts';
import type { LabView } from '#lib/protocol/LabView.ts';

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

function evidence(extra: Partial<EvidenceView> = {}): EvidenceView {
  return { steps: [], screenshots: [], prepared: null, simulated: 'none', blocked_requests: [], lab: false, ...extra };
}

const EQ: LabEquipmentDto = {
  accounts: [
    { role: 'admin', username: 'admin@uji.test', default: true, has_password: true },
    { role: 'manager', username: 'manager@uji.test', default: false, has_password: false }
  ],
  login: {
    url: 'http://localhost:5173/login',
    username_field: '#email',
    password_field: '#password',
    submit: 'text=Sign in',
    submit_path: '/api/login',
    success_kind: 'path',
    success: '/admin',
    authenticated_url: 'http://localhost:5173/admin'
  },
  safe_posts: ['/api/search'],
  fixtures_dir: 'review-fixtures',
  data: null
};

function view(extra: Partial<LabView> = {}): LabView {
  return {
    repo_id: 'kelola-web',
    equipment: null,
    approved: false,
    approved_at_ms: null,
    proposing: false,
    proposal: null,
    proposal_error: null,
    ...extra
  };
}

describe('bukti lab di baris AC', () => {
  it('Pass lab adalah Pass penuh; Pass bertanda simulasi tidak butuh konfirmasi', () => {
    const rows = [
      row('1', 'pass', { evidence_detail: evidence({ lab: true, steps: ['Open /x'] }) }),
      row('2', 'pass', { evidence_detail: evidence({ lab: true, simulated: 'request' }) })
    ];
    expect(gate(rows, 'block', false).state).toBe('ready');
    expect(gateRows(rows).cants).toEqual([]);
    expect(counts(rows).pass).toBe(2);
  });

  it('badge Lab dan tanda simulasi', () => {
    expect(evidenceBadges(row('1', 'pass'))).toEqual([]);
    expect(evidenceBadges(row('1', 'pass', { evidence_detail: evidence({ lab: true }) }))).toEqual(['Lab']);
    expect(evidenceBadges(row('1', 'pass', { evidence_detail: evidence({ lab: true, simulated: 'request' }) }))).toEqual([
      'Lab',
      'Simulated request'
    ]);
    expect(evidenceBadges(row('1', 'pass', { evidence_detail: evidence({ simulated: 'sql' }) }))).toEqual([
      'Prepared with SQL'
    ]);
  });

  it('label kebutuhan lab dan rekomendasi', () => {
    expect(labNeedLabel('role')).toBe('Needs another test account');
    expect(labNeedLabel(null)).toBeNull();
    expect(labNeedLabel('lain')).toBe('lain');
    expect(adviceLabel('accept')).toBe('Difference accepted: the app follows the AC');
    expect(adviceLabel('follow_prototype')).toBe('Change the app to follow the prototype');
  });

  it('rekomendasi Approve accept memindahkan baris ke difference accepted; Reject tetap menahan push', () => {
    const differs = row('1', 'pass', {
      visual: 'differs',
      look_recommendation: { advice: 'accept', reason: 'Key Behavior follows the AC' }
    });
    expect(gate([differs], 'block', false).state).toBe('blocked');
    // Hasil daemon setelah Approve (ledger.look_decision).
    const approved = { ...differs, visual: 'accepted', look_recommendation: null };
    expect(gate([approved], 'block', false).state).toBe('ready');
    // Setelah Reject: rekomendasi hilang, baris tetap berbeda.
    const rejected = { ...differs, look_recommendation: null };
    expect(gate([rejected], 'block', false).state).toBe('blocked');
  });

  it('temuan di luar AC tidak memengaruhi hitungan', () => {
    const rows = [row('1', 'pass'), row('2', 'fail')];
    expect(counts(rows)).toEqual(counts([...rows]));
  });
});

describe('form perlengkapan lab', () => {
  it('password tersimpan tidak pernah ditampilkan', () => {
    const f = labForm(EQ);
    expect(f.accounts.map((a) => a.password)).toEqual(['', '']);
    expect(f.accounts.map((a) => a.hasPassword)).toEqual([true, false]);
    expect(JSON.stringify(f)).not.toContain('rahasia');
  });

  it('mengosongkan field password tidak menghapusnya; yang diisi dikirim', () => {
    const f = labForm(EQ);
    f.accounts[1].password = 'rahasia';
    const p = labSetParams('kelola-web', f);
    expect(p.passwords).toEqual([{ role: 'manager', password: 'rahasia' }]);
    expect(p.equipment.accounts[0]).toEqual({
      role: 'admin',
      username: 'admin@uji.test',
      default: true,
      has_password: true
    });
    expect(p.equipment.fixtures_dir).toBe('review-fixtures');
    const blank = labForm(EQ);
    blank.fixturesDir = '  ';
    expect(labSetParams('kelola-web', blank).equipment.fixtures_dir).toBeNull();
    expect(labApproveParams('kelola-web', labForm(EQ)).equipment).toEqual(EQ);
  });

  it('validasi dasar form', () => {
    expect(validateLabForm(labForm(EQ))).toEqual({});
    const f = emptyLabForm();
    const errs = validateLabForm(f);
    expect(errs.accounts).toBeDefined();
    expect(errs.loginUrl).toBeDefined();
    const two = labForm(EQ);
    two.accounts[1].default = true;
    expect(validateLabForm(two).accounts).toContain('one default');
    const dup = labForm(EQ);
    dup.accounts[1].role = 'admin';
    expect(validateLabForm(dup).accounts).toContain('admin');
  });
});

describe('status perlengkapan', () => {
  it('repo tanpa perlengkapan menawarkan penyiapan', () => {
    const s = labStatus(view());
    expect(s.text).toBe('No test login is prepared for unattended review');
    expect(s.setup).toBe(true);
    expect(labStatus(null).setup).toBe(true);
  });

  it('usulan, menunggu persetujuan, dan disetujui', () => {
    expect(labStatus(view({ proposing: true })).text).toBe('Proposing lab equipment from the repo…');
    expect(labStatus(view({ proposal: EQ })).text).toBe('A proposal is ready for you to review');
    expect(labStatus(view({ proposal_error: 'The agent stopped at its budget limit' })).text).toContain('budget');
    expect(labStatus(view({ equipment: EQ })).text).toBe('Saved; waiting for your approval');
    const ok = labStatus(view({ equipment: EQ, approved: true, approved_at_ms: 1 }));
    expect(ok.text).toBe('Approved');
    expect(ok.setup).toBe(false);
  });
});
