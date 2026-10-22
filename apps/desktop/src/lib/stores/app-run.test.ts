import { describe, expect, it } from 'vitest';
import type { AppRunView } from '#lib/protocol/AppRunView.ts';
import type { Rpc } from '#lib/rpc.ts';
import { AppRunStore } from './app-run.svelte.ts';

function view(run_id: string, state: string): AppRunView {
  return { run_id, state, members: [], database_confirmed: false, login_confirmed: false, error: null };
}

class FakeRpc implements Rpc {
  owner: AppRunView | null = view('rA', 'running');
  status = view('rB', 'off');
  calls: string[] = [];
  handlers = new Map<string, (p: unknown) => void>();
  async request<T>(method: string): Promise<T> {
    this.calls.push(method);
    if (method === 'app.owner') return this.owner as T;
    if (method === 'app.status') return this.status as T;
    throw new Error(method);
  }
  on(method: string, h: (p: unknown) => void): () => void {
    this.handlers.set(method, h);
    return () => this.handlers.delete(method);
  }
}

const tick = () => new Promise((r) => setTimeout(r, 0));

describe('state Run app', () => {
  it('app.updated memperbarui pemilik dan state run yang dibuka', async () => {
    const rpc = new FakeRpc();
    const store = new AppRunStore();
    store.attach(rpc);
    store.watch('rB');
    await tick();
    expect(store.owner?.run_id).toBe('rA');
    expect(store.otherOwner('rB')?.run_id).toBe('rA');
    expect(store.otherOwner('rA')).toBeNull();
    rpc.owner = view('rB', 'running');
    rpc.status = view('rB', 'running');
    rpc.handlers.get('app.updated')?.({ run_id: 'rB' });
    await tick();
    expect(store.owner?.run_id).toBe('rB');
    expect(store.current?.state).toBe('running');
    rpc.calls = [];
    rpc.handlers.get('app.updated')?.({ run_id: 'rX' });
    await tick();
    expect(rpc.calls).toEqual(['app.owner']);
  });
});
