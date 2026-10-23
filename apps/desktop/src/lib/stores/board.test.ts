import { describe, expect, it } from 'vitest';
import { flagMeta, groupByColumn, isReadOnly, ownerLabel, runMeta, sortRuns } from '#lib/board.ts';
import type { CardView } from '#lib/protocol/CardView.ts';
import type { RunView } from '#lib/protocol/RunView.ts';
import type { Rpc } from '#lib/rpc.ts';
import { BoardStore } from './board.svelte.ts';

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
    pr_url: null,
    branch: null,
    repo_id: null,
    last_modified_ms: 0,
    ...extra
  };
}

function run(id: string, state: string): RunView {
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
    cards: []
  };
}

class FakeRpc implements Rpc {
  handlers = new Map<string, ((p: unknown) => void)[]>();
  board: CardView[] = [];
  syncedAt: number | null = 1000;
  syncError: string | null = null;
  calls = 0;
  async request<T>(method: string): Promise<T> {
    if (method !== 'board.list') throw new Error(method);
    this.calls++;
    return {
      me: 'ou_me',
      cards: this.board,
      sprints: [{ id: 's1', name: 'Sprint 42', start_ms: null, end_ms: null, status: null }],
      synced_at_ms: this.syncedAt,
      sync_error: this.syncError
    } as T;
  }
  on(method: string, h: (p: unknown) => void): () => void {
    this.handlers.set(method, [...(this.handlers.get(method) ?? []), h]);
    return () => undefined;
  }
  emit(method: string): void {
    for (const h of this.handlers.get(method) ?? []) h({});
  }
  emitWith(method: string, params: unknown): void {
    for (const h of this.handlers.get(method) ?? []) h(params);
  }
}

const tick = () => new Promise((r) => setTimeout(r, 0));

describe('papan kanban', () => {
  it('menempatkan kartu sesuai kolom dan menambah kolom Tidak dikenal bila perlu', () => {
    const cols = groupByColumn([card('1', 'todo'), card('2', 'review'), card('3', 'unknown')]);
    expect(cols.map((c) => c.id)).toEqual(['todo', 'in_progress', 'review', 'done', 'unknown']);
    expect(cols.find((c) => c.id === 'unknown')?.cards.map((c) => c.record_id)).toEqual(['3']);
    expect(groupByColumn([card('1', 'todo')]).map((c) => c.id)).not.toContain('unknown');
  });

  it('store memuat ulang papan saat event board.updated', async () => {
    const rpc = new FakeRpc();
    rpc.board = [card('1', 'todo')];
    const store = new BoardStore();
    store.attach(rpc);
    await tick();
    expect(store.columns[0].cards).toHaveLength(1);
    rpc.board = [card('1', 'in_progress', { status: 'In Progress' })];
    rpc.emit('board.updated');
    await tick();
    expect(store.columns[0].cards).toHaveLength(0);
    expect(store.columns[1].cards[0].status).toBe('In Progress');
    expect(store.me).toBe('ou_me');
  });

  it('status sinkron Lark dari daemon tersedia di store', async () => {
    const rpc = new FakeRpc();
    rpc.syncError = 'Lark returned 503';
    const store = new BoardStore();
    store.attach(rpc);
    await tick();
    expect(store.syncedAt).toBe(1000);
    expect(store.syncError).toBe('Lark returned 503');
  });

  it('status sinkron ikut event sync.status tanpa memuat ulang papan', async () => {
    const rpc = new FakeRpc();
    const store = new BoardStore();
    store.attach(rpc);
    await tick();
    const calls = rpc.calls;
    rpc.emitWith('sync.status', { synced_at_ms: 5000, sync_error: 'InvalidFilter' });
    expect(store.syncedAt).toBe(5000);
    expect(store.syncError).toBe('InvalidFilter');
    expect(rpc.calls).toBe(calls);
  });

  it('refresh menandai sinkron berjalan sampai hasil poll tiba', async () => {
    const rpc = new FakeRpc();
    const store = new BoardStore();
    store.attach(rpc);
    await tick();
    store.beginSync();
    expect(store.syncing).toBe(true);
    rpc.emitWith('sync.status', { synced_at_ms: 9000, sync_error: null });
    expect(store.syncing).toBe(false);
    expect(store.syncedAt).toBe(9000);
  });

  it('nama sprint dari Lark; sprint tak dikenal memakai ID', async () => {
    const rpc = new FakeRpc();
    rpc.board = [card('1', 'todo', { sprint_ids: ['s1'] }), card('2', 'todo', { sprint_ids: ['sX'] })];
    const store = new BoardStore();
    store.attach(rpc);
    await tick();
    expect(store.sprintName('s1')).toBe('Sprint 42');
    expect(store.sprintName('sX')).toBe('sX');
    expect(store.sprintOpts.map((o) => o.label)).toEqual(['Sprint 42', 'sX']);
  });

  it('kartu milik dev lain read-only; ambil alih hanya saat klaim tidak aktif', () => {
    const active = card('1', 'in_progress', { owner: 'ou_b', owner_active: true });
    const stale = card('2', 'in_progress', { owner: 'ou_b', owner_active: false, can_takeover: true });
    expect(isReadOnly(active)).toBe(true);
    expect(ownerLabel(active)).toBe('ou_b · active');
    expect(isReadOnly(stale)).toBe(false);
    expect(ownerLabel(stale)).toBe('ou_b · inactive');
    expect(stale.can_takeover).toBe(true);
  });

  it('filter sprint di store hanya menampilkan kartu sprint terpilih', async () => {
    const rpc = new FakeRpc();
    rpc.board = [card('1', 'todo', { sprint_ids: ['s1'] }), card('2', 'todo', { sprint_ids: ['s2'] })];
    const store = new BoardStore();
    store.attach(rpc);
    await tick();
    store.sprint = 's2';
    expect(store.columns[0].cards.map((c) => c.record_id)).toEqual(['2']);
  });

  it('kartu yang nilainya ditimpa Lark menampilkan flag nilai ditimpa', () => {
    const c = card('1', 'todo', { flag: { kind: 'overwritten', detail: 'status berubah' } });
    expect(c.flag && flagMeta(c.flag.kind).label).toBe('Overwritten by Lark');
    const wf = card('2', 'in_progress', { run: run('2', 'wf_awaiting_comment') });
    expect(wf.run && runMeta(wf.run.state).label).toBe('Waiting for reviewer comments');
  });

  it('gerbang readiness: alasan Start nonaktif berasal dari daemon', () => {
    const c = card('1', 'todo', { start_blocked: 'task is not ready: Development Readiness "Review AC By PM"' });
    expect(c.start_blocked).toContain('Review AC By PM');
  });

  it('panel agent mengurutkan run yang menunggu jawaban di atas', () => {
    const sorted = sortRuns([run('a', 'running'), run('b', 'in_review_loop'), run('c', 'awaiting_dev'), run('d', 'failed')]);
    expect(sorted.map((r) => r.id)).toEqual(['c', 'd', 'a', 'b']);
  });
});
