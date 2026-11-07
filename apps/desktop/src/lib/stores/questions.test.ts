import { describe, expect, it } from 'vitest';
import type { QuestionView } from '#lib/protocol/QuestionView.ts';
import type { Rpc } from '#lib/rpc.ts';
import { QuestionsStore } from './questions.svelte.ts';

function question(id: string, status = 'pending'): QuestionView {
  return {
    id,
    run_id: 'r1',
    question: 'Keep?',
    options: [
      { label: 'Yes', description: 'keep', recommended: true },
      { label: 'No', description: 'drop', recommended: false }
    ],
    status,
    answer: null,
    created_at_ms: 1,
    closed_at_ms: null
  };
}

function fakeRpc(state: { list: QuestionView[]; reject?: string }) {
  const calls: [string, unknown][] = [];
  const handlers = new Map<string, (p: unknown) => void>();
  const rpc: Rpc = {
    async request<T>(method: string, params?: unknown): Promise<T> {
      calls.push([method, params]);
      if (method === 'questions.list') return state.list as T;
      if (method === 'questions.answer' && state.reject) throw new Error(state.reject);
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

describe('QuestionsStore', () => {
  it('memuat pertanyaan dan memuat ulang hanya untuk run ini', async () => {
    const { rpc, calls, handlers } = fakeRpc({ list: [question('q-1')] });
    const s = new QuestionsStore();
    s.attach(rpc, 'r1');
    await tick();
    expect(s.byId('q-1')?.question).toBe('Keep?');
    const before = calls.length;
    handlers.get('questions.updated')?.({ run_id: 'r2' });
    await tick();
    expect(calls.length).toBe(before);
    handlers.get('questions.updated')?.({ run_id: 'r1' });
    await tick();
    expect(calls.length).toBe(before + 1);
  });

  it('klik mengirim indeks pilihan dan menandai sedang dikirim', async () => {
    const { rpc, calls } = fakeRpc({ list: [question('q-1')] });
    const s = new QuestionsStore();
    s.attach(rpc, 'r1');
    await tick();
    const done = s.answer('q-1', 1);
    expect(s.submitting['q-1']).toBe(true);
    await done;
    expect(calls).toContainEqual(['questions.answer', { question_id: 'q-1', option: 1 }]);
    expect(s.submitting['q-1']).toBeUndefined();
    expect(s.notes['q-1']).toBeUndefined();
  });

  it('kiriman gagal memberi catatan dan kartu bisa dicoba lagi', async () => {
    const state: { list: QuestionView[]; reject?: string } = { list: [question('q-1')], reject: 'agent did not start' };
    const { rpc } = fakeRpc(state);
    const s = new QuestionsStore();
    s.attach(rpc, 'r1');
    await tick();
    await s.answer('q-1', 0);
    expect(s.notes['q-1']).toContain('agent did not start');
    expect(s.submitting['q-1']).toBeUndefined();
    state.reject = undefined;
    await s.answer('q-1', 0);
    expect(s.notes['q-1']).toBeUndefined();
  });
});
