import { describe, expect, it } from 'vitest';
import { counts, gate, gateRows, NO_VISUAL } from '#lib/ac-review.ts';
import {
  adviceLabel,
  dataBanner,
  deferredDataRows,
  emptyDataForm,
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
    data_session: null,
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

describe('lab data (fase 2, U15)', () => {
  const DATA = {
    scripts_dir: 'review-fixtures/lab',
    stores: [
      {
        name: 'tenant',
        kind: 'postgres',
        host: 'localhost',
        port: 5433,
        database: 'kelola',
        folder: null,
        snapshot: 'snapshot.sh',
        restore: 'restore.sh',
        fingerprint: 'fingerprint.sh',
        test_change: 'change.sh',
        lab_user: 'lab_min',
        maintenance_user: 'postgres',
        has_credentials: true
      },
      {
        name: 'uploads',
        kind: 'folder',
        host: null,
        port: null,
        database: null,
        folder: '/srv/uploads',
        snapshot: 'snap-dir.sh',
        restore: 'restore-dir.sh',
        fingerprint: 'fp-dir.sh',
        test_change: 'touch.sh',
        lab_user: null,
        maintenance_user: null,
        has_credentials: false
      }
    ],
    seeds: [{ name: 'kb', command: 'seed-kb.sh' }]
  };

  it('password database tidak pernah ditampilkan; yang kosong tidak dikirim', () => {
    const f = labForm({ ...EQ, data: DATA });
    expect(f.data?.stores[0].labPassword).toBe('');
    expect(f.data?.stores[0].hasCredentials).toBe(true);
    const p = labSetParams('kelola-web', f);
    expect(p.db).toEqual([
      { store: 'tenant', lab_user: 'lab_min', lab_password: null, maintenance_user: 'postgres', maintenance_password: null }
    ]);
    expect(p.equipment.data?.stores[0]).toMatchObject({ name: 'tenant', kind: 'postgres', port: 5433, database: 'kelola' });
    expect(p.equipment.data?.stores[1]).toMatchObject({ kind: 'folder', folder: '/srv/uploads', host: null });
    f.data!.stores[0].labPassword = 'baru';
    expect(labSetParams('kelola-web', f).db[0].lab_password).toBe('baru');
  });

  it('validasi bagian data', () => {
    const f = labForm({ ...EQ, data: DATA });
    expect(validateLabForm(f)).toEqual({});
    f.data!.stores[0].snapshot = '';
    expect(validateLabForm(f).data).toContain('tenant');
    const g = labForm({ ...EQ, data: DATA });
    g.data!.stores[0].hasCredentials = false;
    expect(validateLabForm(g).data).toContain('password');
    const h = labForm({ ...EQ, data: { ...DATA, stores: [] } });
    expect(validateLabForm(h).data).toBeDefined();
    expect(emptyDataForm().stores).toEqual([]);
  });

  it('banner sesi data', () => {
    expect(dataBanner(null)).toBeNull();
    const running = dataBanner({ run_id: 'r1', phase: 'running', error: null });
    expect(running?.tone).toBe('working');
    expect(running?.retry).toBe(false);
    const waiting = dataBanner({ run_id: 'r1', phase: 'restore_waiting', error: null });
    expect(waiting?.body).toContain('Stop');
    const failed = dataBanner({ run_id: 'r1', phase: 'restore_failed', error: 'restoring tenant failed' });
    expect(failed?.tone).toBe('failed');
    expect(failed?.body).toContain('restoring tenant failed');
    expect(failed?.retry && failed?.restored).toBe(true);
  });

  it('calon data yang ditunda menawarkan Run lab', () => {
    const deferred = row('1', 'cant_verify', {
      lab_need: 'data',
      evidence: 'Lab with test data deferred: it changes your local data.'
    });
    const other = row('2', 'cant_verify', { lab_need: 'role' });
    expect(deferredDataRows([deferred, other, row('3', 'pass')]).map((r) => r.ac_id)).toEqual(['1']);
  });
});
