import { describe, expect, it } from 'vitest';
import type { CardView } from '#lib/protocol/CardView.ts';
import type { LoginCheck } from '#lib/protocol/LoginCheck.ts';
import type { OvernightReportView } from '#lib/protocol/OvernightReportView.ts';
import type { ReportCardView } from '#lib/protocol/ReportCardView.ts';
import {
  backInQueue,
  canActivate,
  canQueue,
  designPickerLabel,
  queueForVmBlocker,
  canTakeOverReport,
  defaultVmTarget,
  vmTargetRepos,
  queueRowMode,
  queueGroupText,
  groupCardLabel,
  holdLabel,
  leadRow,
  heldAction,
  queueAddLabel,
  queueStepLabel,
  showQueueForVm,
  formatTokens,
  loginNotices,
  offsetOptions,
  reportGroups,
  timeOptions,
  usageText,
  vmBadge
} from './overnight';

function card(over: Partial<CardView> = {}): CardView {
  return {
    record_id: 'rec1',
    key: 'PGS-1',
    title: 'Form',
    status: 'To Do',
    column: 'todo',
    readiness: 'Ready To Dev',
    start_blocked: null,
    sprint_ids: [],
    assignees: ['ou_me'],
    team: false,
    owner: null,
    owner_active: false,
    mine: true,
    can_takeover: false,
    flag: null,
    run: null,
    holder: null,
    pr_url: null,
    branch: null,
    repo_id: null,
    last_modified_ms: 0,
    ...over
  } as CardView;
}

function rc(over: Partial<ReportCardView>): ReportCardView {
  return {
    record_id: 'r',
    key: 'PGS-9',
    title: 't',
    run_id: 'run',
    repo_id: 'repo1',
    outcome: 'working',
    hold_reason: null,
    hold_note: null,
    pr_url: null,
    ac_pass: 0,
    ac_fail: 0,
    ac_cant_verify: 0,
    entered_waiting_fix: false,
    design_notes: [],
    group_card: null,
    changed_in_lark: false,
    ...over
  };
}

describe('badge kartu VM di board laptop', () => {
  it('On VM untuk kartu yang dipegang daemon VM milik saya', () => {
    expect(vmBadge(card({ holder: 'own_other_daemon' }), new Set())).toBe('on_vm');
  });

  it('covers AE15: Queued for VM selama di antrean, hilang setelah dikeluarkan atau VM offline', () => {
    const c = card();
    expect(vmBadge(c, new Set(['rec1']))).toBe('queued');
    expect(vmBadge(c, new Set())).toBeNull();
    expect(vmBadge(c, null), 'link VM terputus').toBeNull();
  });
});

describe('antrean', () => {
  it('hanya kartu To Do di papan saya yang bisa di-Start', () => {
    expect(canQueue(card(), new Set())).toBe(true);
    expect(canQueue(card(), new Set(['rec1'])), 'sudah diantre').toBe(false);
    expect(canQueue(card({ column: 'in_progress', status: 'In Progress' }), new Set())).toBe(false);
    expect(canQueue(card({ mine: false, team: true }), new Set())).toBe(false);
    expect(canQueue(card({ start_blocked: 'task is not ready: x' }), new Set())).toBe(false);
    expect(canQueue(card({ start_blocked: 'claimed by ou_x' }), new Set())).toBe(false);
    expect(canQueue(card({ holder: 'own_other_daemon' }), new Set())).toBe(false);
  });

  it('VM tidak bisa diaktifkan sebelum batas siklus diisi', () => {
    expect(canActivate(null)).toBe(false);
    expect(canActivate({ start: '19:00', end: '06:00', offset: '+07:00', token_limit: null })).toBe(false);
    expect(canActivate({ start: '19:00', end: '06:00', offset: '+07:00', token_limit: 0 })).toBe(false);
    expect(canActivate({ start: '19:00', end: '06:00', offset: '+07:00', token_limit: 500000 })).toBe(true);
  });
});

describe('login VM', () => {
  it('login yang expired atau hilang memberi peringatan dengan langkah perbaikan', () => {
    const logins: LoginCheck[] = [
      { service: 'lark', state: 'ok', fix: null },
      { service: 'gh', state: 'expired', fix: 'Run gh auth login' },
      { service: 'claude', state: 'missing', fix: 'Log in as beni-agent' }
    ];
    const n = loginNotices(logins);
    expect(n.map((x) => [x.label, x.tone])).toEqual([
      ['Lark', 'ok'],
      ['GitHub CLI', 'warning'],
      ['Claude Code (agent user)', 'failed']
    ]);
    expect(n[1].fix).toBe('Run gh auth login');
  });
});

describe('laporan pagi', () => {
  const report: OvernightReportView = {
    cycle_start_ms: 1,
    used: 250_000,
    limit: 1_000_000,
    cache_read_tokens: 5,
    cards: [
      rc({ record_id: 'a', outcome: 'draft' }),
      rc({ record_id: 'b', outcome: 'held', hold_reason: 'ac_gate' }),
      rc({ record_id: 'c', outcome: 'held', hold_reason: 'stuck', hold_note: 'Which endpoint?' }),
      rc({ record_id: 'd', outcome: 'sent_to_review' }),
      rc({ record_id: 'e', outcome: 'failed' })
    ],
    dequeued: [{ record_id: 'f', key: 'PGS-6', title: 'x', reason: 'the card moved to In Progress in Lark', at_ms: 2 }],
    reviewed_without_fix: 1,
    planned_held: []
  };

  it('kartu dikelompokkan per hasil, ac_gate terpisah dari parkir lain', () => {
    const g = reportGroups(report);
    expect(g.map((x) => [x.id, x.cards.map((c) => c.record_id)])).toEqual([
      ['draft', ['a']],
      ['ac_gate', ['b']],
      ['held', ['c']],
      ['sent_to_review', ['d']],
      ['failed', ['e']]
    ]);
  });

  it('pemakaian terhadap batas', () => {
    expect(usageText(report)).toBe('250,000 of 1,000,000 tokens (25%)');
    expect(usageText({ ...report, limit: null })).toBe('250,000 tokens · no limit set');
  });
});

describe('pilihan jam dan offset lembur', () => {
  it('jam tiap 30 menit dan nilai tersimpan di luar langkah tetap ada', () => {
    const opts = timeOptions('19:00');
    expect(opts[0]).toBe('00:00');
    expect(opts[1]).toBe('00:30');
    expect(opts).toHaveLength(48);
    expect(timeOptions('19:15')).toContain('19:15');
    expect(timeOptions('19:15')).toHaveLength(49);
  });

  it('offset UTC −12:00 sampai +14:00 dengan label zona umum Indonesia', () => {
    const opts = offsetOptions('+07:00');
    expect(opts[0].value).toBe('-12:00');
    expect(opts.at(-1)?.value).toBe('+14:00');
    expect(opts.find((o) => o.value === '+07:00')?.label).toBe('UTC+07:00 · WIB');
    expect(opts.find((o) => o.value === '+08:00')?.label).toBe('UTC+08:00 · WITA');
    expect(opts.find((o) => o.value === '+09:00')?.label).toBe('UTC+09:00 · WIT');
    expect(offsetOptions('+05:45').map((o) => o.value)).toContain('+05:45');
  });

  it('batas token dibaca dengan pemisah ribuan', () => {
    expect(formatTokens(10_000_000)).toBe('10,000,000');
  });
});

describe('kartu laporan yang sudah diantre ulang', () => {
  const card = { record_id: 'r1', outcome: 'held' } as ReportCardView;
  it('kartu tertahan atau gagal yang ada di antrean menunggu VM', () => {
    expect(backInQueue(card, new Set(['r1']))).toBe(true);
    expect(backInQueue({ ...card, outcome: 'failed' } as ReportCardView, new Set(['r1']))).toBe(true);
    expect(backInQueue(card, new Set())).toBe(false);
    expect(backInQueue(card, null), 'link VM terputus: tidak diketahui').toBe(false);
    expect(backInQueue({ ...card, outcome: 'draft' } as ReportCardView, new Set(['r1']))).toBe(false);
  });
});

describe('antre kartu Planned ke VM', () => {
  const src = (id: string, name: string) => ({ id, name, kind: 'folder', path: `/d/${name}`, ready: true, status: 'Ready' });

  it('label pemilih desain memakai nama, atau id bila sumber sudah dihapus', () => {
    const sources = [src('d1', 'kelola-mock'), src('d2', 'pegasus-mock')];
    expect(designPickerLabel(sources, [])).toBe('No design source');
    expect(designPickerLabel(sources, ['d1', 'x'])).toBe('Design: kelola-mock, x');
  });

  it('Follow design wajib punya sumber desain VM; target juga wajib', () => {
    const one = { laptopRepos: ['api'], targetRepos: ['api'] };
    expect(queueForVmBlocker({ target: null, follow: [], designSources: [], ...one })).toBe('Choose a repo on the VM');
    expect(queueForVmBlocker({ target: 'repo:api', follow: ['ac2'], designSources: [], ...one })).toBe(
      'Some AC follow the design. Pick a design source on the VM.'
    );
    expect(queueForVmBlocker({ target: 'repo:api', follow: ['ac2'], designSources: ['d1'], ...one })).toBeNull();
    expect(queueForVmBlocker({ target: 'repo:api', follow: [], designSources: [], ...one })).toBeNull();
  });

  it('plan workspace tidak boleh diantre ke satu repo atau repo lain', () => {
    const q = { target: 'repo:web', follow: [], designSources: [], laptopRepos: ['web', 'api'] };
    expect(queueForVmBlocker({ ...q, targetRepos: ['web'] })).toBe(
      'This plan covers web, api. Pick a VM target with the same repos.'
    );
    expect(queueForVmBlocker({ ...q, target: 'ws:x', targetRepos: ['api', 'web'] })).toBeNull();
    expect(queueForVmBlocker({ ...q, laptopRepos: ['web'], target: 'ws:x', targetRepos: ['web', 'api'] })).toBe(
      'This plan covers web. Pick a VM target with the same repos.'
    );
  });

  it('Take over dari laporan: draft, tertahan, gagal, atau Waiting Fix', () => {
    const c = (outcome: string, entered_waiting_fix = false) => ({ outcome, entered_waiting_fix });
    expect(canTakeOverReport(c('draft'))).toBe(true);
    expect(canTakeOverReport(c('held'))).toBe(true);
    expect(canTakeOverReport(c('failed'))).toBe(true);
    expect(canTakeOverReport(c('waiting_review', true))).toBe(true);
    expect(canTakeOverReport(c('waiting_review'))).toBe(false);
  });

  it('target VM bawaan mengikuti repo run laptop', () => {
    const vm = {
      repos: [{ id: 'web' }, { id: 'api' }],
      workspaces: [
        { id: 'lain', repo_ids: ['web'] },
        { id: 'smoke', repo_ids: ['api', 'web', 'docs'] }
      ]
    };
    expect(defaultVmTarget(['web'], null, vm)).toBe('repo:web');
    expect(defaultVmTarget(['tidak-ada'], null, vm)).toBe('');
    expect(defaultVmTarget(['web', 'api'], 'work-smoke', vm)).toBe('ws:smoke');
    expect(defaultVmTarget(['web', 'api'], 'lain', vm)).toBe('ws:smoke');
    expect(defaultVmTarget(['web', 'gone'], 'x', vm)).toBe('');
    expect(vmTargetRepos('repo:web', ['web', 'api'], vm.workspaces)).toEqual(['web']);
    expect(vmTargetRepos('ws:smoke', ['web', 'api'], vm.workspaces)).toEqual(['api', 'web']);
    expect(vmTargetRepos('ws:smoke', ['web'], vm.workspaces)).toEqual(['web']);
    expect(vmTargetRepos('ws:gone', ['web'], vm.workspaces)).toEqual([]);
  });

  it('teks langkah serah-terima', () => {
    expect(queueStepLabel('pushing')).toBe('Pushing the branch…');
    expect(queueStepLabel('queuing')).toBe('Adding to the VM queue…');
    expect(queueStepLabel('handing_over')).toBe('Handing over the card…');
  });
});

describe('laporan grup', () => {
  const row = (record_id: string, run_id: string) => ({ record_id, run_id });

  it('aksi run hanya di baris pertama tiap run; kartu tunggal selalu baris pertama', () => {
    const cards = [row('a', 'r1'), row('b', 'r1'), row('c', 'r2')];
    expect(cards.map((c) => leadRow(c, cards))).toEqual([true, false, true]);
  });

  it('aksi run tertahan: jawab/antre lagi, konfirmasi push, atau tunggu Keep/Revert kartu grup', () => {
    const r = (hold_reason: string, changed_in_lark = false, record_id = 'a') => ({
      record_id,
      run_id: 'r1',
      outcome: 'held',
      hold_reason,
      changed_in_lark
    });
    expect(heldAction(r('stuck'), [r('stuck')])).toBe('answer');
    expect(heldAction(r('usage_limit'), [r('usage_limit')])).toBe('answer');
    expect(heldAction(r('card_removed'), [r('card_removed')]), 'kartu berikutnya dikeluarkan: antre lagi').toBe('answer');
    expect(heldAction(r('needs_confirmation'), [r('needs_confirmation')])).toBe('confirm');
    const rows = [r('needs_confirmation'), r('needs_confirmation', true, 'b')];
    expect(heldAction(rows[0], rows), 'kartu ter-commit berubah di Lark: putuskan di barisnya').toBeNull();
    expect(heldAction({ ...r('stuck'), outcome: 'draft' }, [])).toBeNull();
  });

  it('label keadaan kartu grup', () => {
    expect(groupCardLabel('committed')).toBe('Committed');
    expect(groupCardLabel('active')).toBe('In progress');
    expect(groupCardLabel('pending')).toBe('Not started');
    expect(groupCardLabel('removed')).toBe('Removed');
  });

  it('alasan grup tertahan atau berjeda', () => {
    expect(holdLabel('card_removed')).toBe('Card changed in Lark');
    expect(holdLabel('window_ended')).toBe('Paused at the end of the window');
    expect(holdLabel('yield_to_fix')).toBe('Paused for a Waiting Fix');
  });
});

describe('antrean grup', () => {
  it('baris grup: jumlah kartu dan kartu sesudahnya; kartu tunggal tanpa teks grup', () => {
    expect(queueGroupText({ record_id: 'a', group: ['a', 'b', 'c'], group_keys: ['PGS-1', 'PGS-2', 'PGS-3'] })).toEqual({
      count: 3,
      then: 'then PGS-2, PGS-3'
    });
    expect(queueGroupText({ record_id: 'a', group: [], group_keys: [] })).toBeNull();
  });

  it('tombol tambah: Add to queue untuk satu kartu, Queue N cards together untuk grup', () => {
    expect(queueAddLabel(0)).toBe('Add to queue');
    expect(queueAddLabel(1)).toBe('Add to queue');
    expect(queueAddLabel(3)).toBe('Queue 3 cards together');
  });
});

describe('tampilan kartu Planned', () => {
  it('baris antrean: serah-terima tertunda, Planned, atau No plan', () => {
    expect(queueRowMode({ planned_state: 'handover_pending' })).toBe('pending');
    expect(queueRowMode({ planned_state: 'waiting' })).toBe('planned');
    expect(queueRowMode({ planned_state: 'held' })).toBe('planned');
    expect(queueRowMode({ planned_state: null })).toBe('no_plan');
  });

  it('tombol antre ke VM: kartu tunggal, atau grup selama kartu pertama masih di-plan (R24); tersembunyi saat VM belum siap', () => {
    const ready = { statusLoaded: true, vmRole: true, larkReady: true };
    const firstPlanning = [{ state: 'active' }, { state: 'pending' }];
    expect(showQueueForVm(ready, [])).toBe(true);
    expect(showQueueForVm(ready, firstPlanning)).toBe(true);
    expect(showQueueForVm(ready, [{ state: 'done' }, { state: 'active' }]), 'kartu pertama sudah selesai').toBe(false);
    expect(showQueueForVm(ready, [{ state: 'removed_by_dev' }, { state: 'active' }])).toBe(false);
    expect(showQueueForVm({ ...ready, statusLoaded: false }, [])).toBe(false);
    expect(showQueueForVm({ ...ready, vmRole: false }, firstPlanning)).toBe(false);
    expect(showQueueForVm({ ...ready, larkReady: false }, [])).toBe(false);
  });
});
