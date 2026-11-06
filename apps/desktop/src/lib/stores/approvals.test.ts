import { describe, expect, it } from 'vitest';
import type { ApprovalView } from '#lib/protocol/ApprovalView.ts';
import type { Rpc } from '#lib/rpc.ts';
import { ApprovalsStore } from './approvals.svelte.ts';

function card(id: string, at: number, status = 'pending'): ApprovalView {
  return {
    id,
    run_id: 'r1',
    turn: 'implement',
    tool: 'Bash',
    input: { command: `cmd ${id}` },
    description: null,
    cwd: '/w',
    status,
    answer: null,
    created_at_ms: at,
    deadline_ms: at + 600_000,
    closed_at_ms: null
  };
}

function fakeRpc(state: { cards: ApprovalView[]; reject?: boolean }) {
  const calls: [string, unknown][] = [];
  const handlers = new Map<string, (p: unknown) => void>();
  const rpc: Rpc = {
    async request<T>(method: string, params?: unknown): Promise<T> {
      calls.push([method, params]);
      if (method === 'approvals.list') return state.cards as T;
      if (method === 'approvals.answer' && state.reject) throw new Error('no longer pending');
      return {} as T;
    },
    on(method, handler) {
      handlers.set(method, handler);
      return () => handlers.delete(method);
    }
  };
  return { rpc, calls, handlers };
}

const tick = () => new Promise((r) => setTimeout(r, 0));

describe('ApprovalsStore', () => {
  it('memuat kartu urut terlama dan memuat ulang hanya untuk run ini', async () => {
    const state = { cards: [card('b', 2), card('a', 1)] };
    const { rpc, calls, handlers } = fakeRpc(state);
    const s = new ApprovalsStore();
    s.attach(rpc, 'r1');
    await tick();
    expect(s.cards.map((c) => c.id)).toEqual(['a', 'b']);
    expect(s.pending).toBe(2);
    const before = calls.length;
    handlers.get('approvals.updated')?.({ run_id: 'r2' });
    await tick();
    expect(calls.length).toBe(before);
    handlers.get('approvals.updated')?.({ run_id: 'r1' });
    await tick();
    expect(calls.length).toBe(before + 1);
  });

  it('jawaban terlambat menampilkan bahwa kartu tidak berlaku', async () => {
    const state = { cards: [card('a', 1)], reject: true };
    const { rpc, calls } = fakeRpc(state);
    const s = new ApprovalsStore();
    s.attach(rpc, 'r1');
    await tick();
    await s.answer('a', 'allow_once');
    expect(calls).toContainEqual(['approvals.answer', { run_id: 'r1', approval_id: 'a', answer: 'allow_once' }]);
    expect(s.notes.a).toBe('This request is no longer pending.');
    expect(s.submitting.a).toBeUndefined();
  });

  it('seen mengirim approvals.seen untuk run ini', async () => {
    const { rpc, calls } = fakeRpc({ cards: [] });
    const s = new ApprovalsStore();
    s.attach(rpc, 'r1');
    await s.seen();
    expect(calls).toContainEqual(['approvals.seen', { run_id: 'r1' }]);
  });
});

describe('notifikasi kartu', () => {
  it('membuka workspace run dan menandai tab Chat di kartu terlama', async () => {
    const { nav } = await import('#lib/stores/nav.svelte.ts');
    const { workspace } = await import('#lib/stores/workspace.svelte.ts');
    nav.runApprovals('r9');
    expect(nav.approvalsFor).toBe('r9');
    expect(nav.page).toBe('board');
    expect(workspace.selected).toBe('r9');
  });
});
