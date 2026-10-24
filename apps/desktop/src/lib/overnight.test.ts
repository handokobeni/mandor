import { describe, expect, it } from 'vitest';
import type { CardView } from '#lib/protocol/CardView.ts';
import type { LoginCheck } from '#lib/protocol/LoginCheck.ts';
import type { OvernightReportView } from '#lib/protocol/OvernightReportView.ts';
import type { ReportCardView } from '#lib/protocol/ReportCardView.ts';
import { canActivate, canQueue, loginNotices, reportGroups, usageText, vmBadge } from './overnight';

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
    reviewed_without_fix: 1
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
