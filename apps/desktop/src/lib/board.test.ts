import { describe, expect, it } from 'vitest';
import {
  acChip,
  agentGroups,
  assigneeLabel,
  canChat,
  cardAction,
  cardReason,
  composerBlocked,
  filterCards,
  isReady,
  isReadOnly,
  flagMeta,
  orderCards,
  ownerLabel,
  quietLeft,
  relTime,
  runLabel,
  runMeta
} from '#lib/board.ts';
import type { CardView } from '#lib/protocol/CardView.ts';
import type { RunView } from '#lib/protocol/RunView.ts';

function card(id: string, column: string, extra: Partial<CardView> = {}): CardView {
  return {
    record_id: id,
    key: `PGS-${id}`,
    title: `Task ${id}`,
    status: 'To Do',
    column,
    readiness: 'Ready To Dev',
    start_blocked: null,
    sprint_ids: ['s1'],
    assignees: ['ou_me'],
    team: false,
    owner: null,
    owner_active: false,
    mine: false,
    can_takeover: false,
    flag: null,
    run: null,
    holder: null,
    pr_url: null,
    branch: null,
    repo_id: null,
    last_modified_ms: 0,
    ...extra
  };
}

function run(id: string, state: string, extra: Partial<RunView> = {}): RunView {
  return {
    id,
    record_id: id,
    repo_id: 'r',
    key: null,
    title: id,
    state,
    branch: null,
    pr_url: null,
    workspace_id: null,
    repos: [],
    phase: 'implement',
    plan: null,
    last_error: null,
    note: null,
    queued: [],
    automatic: false,
    quiet_until_ms: null,
    started_at_ms: 0,
    updated_at_ms: 0,
    ac: null,
    cards: [],
    ...extra
  };
}

describe('label fase plan', () => {
  it('fase plan diberi label sendiri, fase implementasi memakai label bawaan', () => {
    expect(runLabel(run('a', 'running', { phase: 'plan' }))).toBe('Planning');
    expect(runLabel(run('a', 'awaiting_dev', { phase: 'plan', plan: '## Plan' }))).toBe('Plan ready');
    expect(runLabel(run('a', 'awaiting_dev'))).toBeUndefined();
    expect(runLabel(run('a', 'running', { phase: 'brainstorm' }))).toBe('Brainstorming');
    expect(runLabel(run('a', 'awaiting_dev', { phase: 'brainstorm' }))).toBe('Question for you');
  });
});

describe('status run dan flag', () => {
  it('setiap state run punya label, tone, dan ikon berputar hanya saat bekerja', () => {
    expect(runMeta('awaiting_dev')).toEqual({ label: 'Waiting for you', tone: 'attention', spin: false });
    expect(runMeta('running')).toEqual({ label: 'Working', tone: 'working', spin: true });
    expect(runMeta('orphaned').label).toBe('Parked: claim lost');
    expect(runMeta('stopped')).toEqual({ label: 'Stopped', tone: 'muted', spin: false });
    expect(runMeta('something_new')).toEqual({ label: 'something_new', tone: 'muted', spin: false });
  });

  it('flag kartu punya label, tone, dan aksi pemulihan', () => {
    expect(flagMeta('sync_failed')).toEqual({ label: 'Sync failed', tone: 'failed', action: 'retry' });
    expect(flagMeta('deleted').action).toBe('close');
    expect(flagMeta('unassigned')).toEqual({ label: 'No longer assigned', tone: 'warning', action: 'close' });
    expect(flagMeta('overwritten').action).toBeNull();
  });
});

describe('filter dan urutan papan', () => {
  const cards = [
    card('1', 'todo', { title: 'Invoice filter', sprint_ids: ['s1'] }),
    card('2', 'todo', { sprint_ids: ['s2'], owner: 'ou_b', owner_active: true }),
    card('3', 'in_progress', { key: 'PGS-1402', sprint_ids: ['s1'], owner: 'ou_b', run: run('3', 'running') })
  ];

  it('menyaring sprint, kata kunci pada key atau judul, dan kartu milik orang lain', () => {
    const ids = (f: Parameters<typeof filterCards>[1]) => filterCards(cards, f).map((c) => c.record_id);
    expect(ids({ sprint: 's1', query: '', mineOnly: false })).toEqual(['1', '3']);
    expect(ids({ sprint: null, query: 'invoice', mineOnly: false })).toEqual(['1']);
    expect(ids({ sprint: null, query: 'pgs-1402', mineOnly: false })).toEqual(['3']);
    // Kartu milik dev lain disembunyikan, kecuali ada run di mesin ini.
    expect(ids({ sprint: null, query: '', mineOnly: true })).toEqual(['1', '3']);
  });

  it('kartu rekan di papan Team: hanya di Team, hanya baca, tanpa Start, dengan nama assignee', () => {
    const t = card('t', 'todo', { team: true, assignees: ['ou_b', 'ou_c'], start_blocked: 'Assigned to another developer' });
    expect(filterCards([t], { sprint: null, query: '', mineOnly: true })).toEqual([]);
    expect(filterCards([t], { sprint: null, query: '', mineOnly: false })).toEqual([t]);
    expect(isReadOnly(t)).toBe(true);
    expect(cardAction(t)).toBeNull();
    expect(cardReason(t)).toBeNull();
    expect(assigneeLabel(t, (id) => ({ ou_b: 'Fikri', ou_c: 'Arsya' })[id] ?? id)).toBe('Fikri, Arsya');
    // Kartu sendiri juga menampilkan PIC, termasuk nama saya.
    expect(assigneeLabel(card('m', 'todo'), (id) => (id === 'ou_me' ? 'Beni Handoko' : id))).toBe('Beni Handoko');
    expect(assigneeLabel(card('n', 'todo', { assignees: [] }))).toBeNull();
  });

  it('Mine: hanya assignee saya, atau yang sedang saya kerjakan', () => {
    const shared = [
      card('a', 'todo'),
      card('b', 'todo', { assignees: ['ou_me', 'ou_b'] }),
      card('c', 'in_progress', { assignees: ['ou_me', 'ou_b'], mine: true, owner: 'ou_me' }),
      card('d', 'in_progress', { assignees: ['ou_me', 'ou_b'], run: run('d', 'running') })
    ];
    const ids = (mineOnly: boolean) => filterCards(shared, { sprint: null, query: '', mineOnly }).map((c) => c.record_id);
    expect(ids(true)).toEqual(['a', 'c', 'd']);
    expect(ids(false)).toEqual(['a', 'b', 'c', 'd']);
  });

  it('run yang menunggu dev di atas; di Todo kartu yang bisa dimulai di atas', () => {
    const order = orderCards([
      card('a', 'in_progress', { run: run('a', 'in_review_loop') }),
      card('b', 'in_progress', { run: run('b', 'awaiting_dev') }),
      card('c', 'in_progress', { flag: { kind: 'sync_failed', detail: '' }, run: run('c', 'running') }),
      card('d', 'in_progress', { run: run('d', 'failed') })
    ]);
    expect(order.map((c) => c.record_id)).toEqual(['b', 'd', 'c', 'a']);
    const todo = orderCards([card('x', 'todo', { start_blocked: 'not ready' }), card('y', 'todo')]);
    expect(todo.map((c) => c.record_id)).toEqual(['y', 'x']);
  });
});

describe('aksi dan alasan kartu', () => {
  it('Open bila ada run, Take over bila klaim tidak aktif, Start di Todo', () => {
    expect(cardAction(card('1', 'in_progress', { run: run('1', 'running') }))).toBe('open');
    expect(cardAction(card('2', 'in_progress', { owner: 'ou_b', can_takeover: true }))).toBe('takeover');
    expect(cardAction(card('3', 'todo'))).toBe('start');
    expect(cardAction(card('4', 'todo', { owner: 'ou_b', owner_active: true }))).toBeNull();
    expect(cardAction(card('5', 'review'))).toBeNull();
  });

  it('alasan Start nonaktif, status tanpa kolom, dan error run ditampilkan', () => {
    const blocked = card('1', 'todo', {
      start_blocked: 'task is not ready: Development Readiness "Review AC By PM"'
    });
    expect(cardReason(blocked)).toBe('Can\'t start: not ready, Development Readiness "Review AC By PM"');
    expect(cardReason(card('2', 'unknown', { status: 'Blocked' }))).toBe(
      'Lark status "Blocked" has no column. Change it in Lark to move this card.'
    );
    expect(cardReason(card('3', 'in_progress', { run: run('3', 'failed', { last_error: 'claude exited' }) }))).toBe(
      'claude exited'
    );
    expect(cardReason(card('4', 'todo'))).toBeNull();
  });

  it('ikon readiness hanya mengikuti gerbang readiness, bukan klaim dev lain', () => {
    expect(isReady(card('1', 'todo'))).toBe(true);
    expect(isReady(card('2', 'todo', { start_blocked: 'claimed by ou_b' }))).toBe(true);
    expect(isReady(card('3', 'todo', { start_blocked: 'task is not ready: Development Readiness is empty' }))).toBe(false);
  });

  it('pemilik lain ditandai aktif atau tidak aktif', () => {
    expect(ownerLabel(card('1', 'todo'))).toBeNull();
    expect(ownerLabel(card('2', 'todo', { owner: 'ou_me', mine: true }))).toBeNull();
    expect(ownerLabel(card('3', 'todo', { owner: 'ou_b', owner_active: true }))).toBe('ou_b · active');
    expect(ownerLabel(card('4', 'todo', { owner: 'ou_b' }))).toBe('ou_b · inactive');
  });
});

describe('panel agent', () => {
  it('mengelompokkan run: menunggu dev, perlu perhatian, bekerja, menunggu', () => {
    const groups = agentGroups(
      [
        run('a', 'in_review_loop'),
        run('b', 'running'),
        run('c', 'awaiting_dev'),
        run('d', 'failed'),
        run('e', 'wf_quiet'),
        run('f', 'running')
      ],
      new Map([['f', { kind: 'sync_failed', detail: '' }]])
    );
    expect(groups.map((g) => [g.label, g.runs.map((r) => r.id)])).toEqual([
      ['Waiting for you', ['c']],
      ['Needs attention', ['d', 'f']],
      ['Working', ['b']],
      ['Waiting', ['a', 'e']]
    ]);
    expect(agentGroups([run('x', 'running')], new Map()).map((g) => g.id)).toEqual(['working']);
  });
});

describe('composer', () => {
  it('chat hanya terbuka saat agent bekerja atau menunggu dev, dengan alasan bila tertutup', () => {
    expect(canChat(run('a', 'awaiting_dev'))).toBe(true);
    expect(canChat(run('b', 'running'))).toBe(true);
    expect(canChat(run('c', 'in_review_loop'))).toBe(false);
    expect(composerBlocked(run('c', 'in_review_loop'))).toBe(
      'The agent is idle while the PR waits for review. Feedback goes through Start fix.'
    );
    expect(composerBlocked(run('d', 'failed'))).toBe('Resume the run to continue the session.');
    expect(composerBlocked(run('e', 'awaiting_dev'))).toBeNull();
  });
});

describe('waktu relatif', () => {
  it('detik, menit, lalu jam', () => {
    expect(relTime(1_000, 13_000)).toBe('12s ago');
    expect(relTime(0, 125_000)).toBe('2m ago');
    expect(relTime(0, 2 * 3_600_000 + 5)).toBe('2h ago');
    expect(relTime(5_000, 1_000)).toBe('just now');
  });
});

describe('hitung mundur jeda tenang', () => {
  it('menit:detik sampai habis, lalu nol', () => {
    expect(quietLeft(95_000, 0)).toBe('1:35');
    expect(quietLeft(10_000, 9_500)).toBe('0:01');
    expect(quietLeft(10_000, 20_000)).toBe('0:00');
  });
});

describe('nama sprint', () => {
  const s = (id: string, name: string, start: number | null, end: number | null) => ({
    id,
    name,
    start_ms: start,
    end_ms: end,
    status: null
  });

  it('opsi filter: sprint terbaru dulu, sprint tanpa data memakai ID', async () => {
    const { sprintOptions } = await import('#lib/board.ts');
    const opts = sprintOptions(
      ['spr1', 'spr2', 'sprX'],
      [s('spr1', 'Sprint 41', Date.UTC(2026, 8, 29), Date.UTC(2026, 9, 10)), s('spr2', 'Sprint 42', Date.UTC(2026, 9, 13), Date.UTC(2026, 9, 24))]
    );
    expect(opts.map((o) => o.id)).toEqual(['spr2', 'spr1', 'sprX']);
    expect(opts[0].label).toBe('Sprint 42 · Oct 13 to Oct 24');
    expect(opts[2].label).toBe('sprX');
  });
});

describe('label review AC dan chip kartu', () => {
  const ac = { total: 7, ok: 5, failed: 2, unverified: 0, not_reviewed: 0, stale: false, design_gaps: 1 };
  it('runLabel untuk Reviewing, gagal, perlu cek, dan Push held', () => {
    expect(runMeta('reviewing').label).toBe('Reviewing AC');
    expect(runLabel({ state: 'awaiting_dev', phase: 'implement', ac, note: null })).toBe('Review: 2 failed');
    expect(runLabel({ state: 'awaiting_dev', phase: 'implement', ac: { ...ac, failed: 0, unverified: 1 } })).toBe(
      'Needs your check'
    );
    expect(runLabel({ state: 'awaiting_dev', phase: 'implement', note: 'Push held: AC-3 failed the review' })).toBe(
      'Push held'
    );
    expect(runLabel({ state: 'awaiting_dev', phase: 'implement' })).toBeUndefined();
  });

  it('chip AC 5/7, 2 failed, dan Review stale', () => {
    expect(acChip({ ...ac, failed: 0 })).toEqual({ text: 'AC 5/7', tone: 'muted' });
    expect(acChip(ac)).toEqual({ text: '2 failed', tone: 'failed' });
    expect(acChip({ ...ac, stale: true })?.text).toBe('Review stale');
    expect(acChip(null)).toBeNull();
  });
});
