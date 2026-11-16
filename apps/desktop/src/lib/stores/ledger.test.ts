import { describe, expect, it } from 'vitest';
import { NO_VISUAL } from '#lib/ac-review.ts';
import type { AcLedgerView } from '#lib/protocol/AcLedgerView.ts';
import type { AcRowView } from '#lib/protocol/AcRowView.ts';
import type { FindingView } from '#lib/protocol/FindingView.ts';
import type { ReviewView } from '#lib/protocol/ReviewView.ts';
import type { Rpc } from '#lib/rpc.ts';
import { LedgerStore } from './ledger.svelte.ts';

function row(id: string, verdict: string): AcRowView {
  return {
    ...NO_VISUAL,
    ac_id: id,
    label: `AC-${id}`,
    text: id,
    design: 'not_checked',
    design_note: null,
    verdict,
    evidence: null,
    stale: false,
    status: 'active',
    reason: null,
    decided_by: null,
    in_gate: true,
    selectable: false,
    card: null
  };
}

class FakeRpc implements Rpc {
  calls: string[] = [];
  handlers = new Map<string, (p: unknown) => void>();
  ledger: AcLedgerView = {
    run_id: 'r1',
    rows: [row('1', 'not_reviewed'), row('2', 'not_reviewed')],
    synced_at_ms: 1,
    sync_error: null,
    design_sources: [],
    gate_mode: 'block'
  };
  history: ReviewView[] = [];
  findings: FindingView[] = [];
  params: unknown[] = [];
  async request<T>(method: string, params?: unknown): Promise<T> {
    this.calls.push(method);
    this.params.push(params);
    if (method === 'ledger.get') return this.ledger as T;
    if (method === 'review.history') return this.history as T;
    if (method === 'review.findings') return this.findings as T;
    if (method === 'ledger.look_decision') {
      const p = params as { ac_id: string; approve: boolean };
      return this.ledger.rows.map((r) =>
        r.ac_id === p.ac_id ? { ...r, visual: p.approve ? 'accepted' : 'differs', look_recommendation: null } : r
      ) as T;
    }
    throw new Error(method);
  }
  on(method: string, h: (p: unknown) => void): () => void {
    this.handlers.set(method, h);
    return () => this.handlers.delete(method);
  }
}

const tick = () => new Promise((r) => setTimeout(r, 0));

describe('buku AC', () => {
  it('menerapkan progres review per baris tanpa memuat ulang buku AC', async () => {
    const rpc = new FakeRpc();
    const store = new LedgerStore();
    store.attach(rpc, 'r1');
    await tick();
    expect(store.ledger?.rows.map((r) => r.verdict)).toEqual(['not_reviewed', 'not_reviewed']);
    const loads = rpc.calls.filter((c) => c === 'ledger.get').length;
    rpc.handlers.get('ledger.updated')?.({ run_id: 'r1', rows: [row('1', 'pass'), row('2', 'not_reviewed')] });
    expect(store.ledger?.rows[0].verdict).toBe('pass');
    expect(rpc.calls.filter((c) => c === 'ledger.get').length).toBe(loads);
    // Event run lain diabaikan.
    rpc.handlers.get('ledger.updated')?.({ run_id: 'r9', rows: [] });
    expect(store.ledger?.rows.length).toBe(2);
  });

  it('review.progress memuat riwayat; resync memuat ulang semuanya', async () => {
    const rpc = new FakeRpc();
    const store = new LedgerStore();
    const detach = store.attach(rpc, 'r1');
    await tick();
    rpc.calls = [];
    rpc.handlers.get('review.progress')?.({ run_id: 'r1' });
    await tick();
    expect(rpc.calls.sort()).toEqual(['review.findings', 'review.history']);
    rpc.calls = [];
    rpc.handlers.get('resync')?.(null);
    await tick();
    expect(rpc.calls.sort()).toEqual(['ledger.get', 'review.findings', 'review.history']);
    detach();
    expect(rpc.handlers.size).toBe(0);
  });
});

describe('review lab (plan 2026-11-10-001, U9)', () => {
  it('memuat temuan di luar AC bersama riwayat', async () => {
    const rpc = new FakeRpc();
    rpc.findings = [{ review_id: 'rv1', card: null, title: 'Toast overlaps the header', detail: 'x', steps: [] }];
    const store = new LedgerStore();
    store.attach(rpc, 'r1');
    await tick();
    expect(store.findings.map((f) => f.title)).toEqual(['Toast overlaps the header']);
  });

  it('Approve dan Reject rekomendasi menerapkan baris dari daemon', async () => {
    const rpc = new FakeRpc();
    rpc.ledger.rows[0] = {
      ...rpc.ledger.rows[0],
      visual: 'differs',
      look_recommendation: { advice: 'accept', reason: 'AC spelling' }
    };
    const store = new LedgerStore();
    store.attach(rpc, 'r1');
    await tick();
    await store.decideLook('1', true);
    expect(rpc.params.at(-1)).toEqual({ run_id: 'r1', ac_id: '1', approve: true });
    expect(store.ledger?.rows[0].visual).toBe('accepted');
    expect(store.ledger?.rows[0].look_recommendation).toBeNull();
    await store.decideLook('1', false);
    expect(store.ledger?.rows[0].visual).toBe('differs');
  });
});

describe('screenshot hasil tampilan (plan 2026-11-03-002, U7)', () => {
  it('requests a screenshot of the attached run and returns its data URL', async () => {
    const calls: [string, unknown][] = [];
    const rpc = {
      request: async (method: string, params: unknown) => {
        calls.push([method, params]);
        if (method === 'review.screenshot') return { data_url: 'data:image/png;base64,AA==' };
        return method === 'review.history' ? [] : { run_id: 'r1', rows: [], synced_at_ms: null, sync_error: null, design_sources: [], gate_mode: 'block' };
      },
      on: () => () => {}
    } as unknown as Rpc;
    const store = new LedgerStore();
    store.attach(rpc, 'r1');
    expect(await store.screenshot('ac1', 1)).toBe('data:image/png;base64,AA==');
    expect(calls.find(([m]) => m === 'review.screenshot')?.[1]).toEqual({ run_id: 'r1', ac_id: 'ac1', index: 1 });
    await store.screenshot('ac1', 0, true);
    expect(calls.filter(([m]) => m === 'review.screenshot').at(-1)?.[1]).toEqual({
      run_id: 'r1',
      ac_id: 'ac1',
      index: 0,
      evidence: true
    });
  });
});
