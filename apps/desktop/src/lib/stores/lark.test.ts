import { describe, expect, it } from 'vitest';
import type { Rpc } from '#lib/rpc.ts';
import { LarkStore } from './lark.svelte.ts';

class FakeRpc implements Rpc {
  loggedIn = false;
  handlers = new Map<string, ((p: unknown) => void)[]>();
  async request<T>(method: string): Promise<T> {
    if (method === 'status') {
      return { version: 'x', uptime_ms: 0, repos: 0, lark: { configured: true, logged_in: this.loggedIn, open_id: null, name: null, reauth_required: false } } as T;
    }
    if (method === 'repos.list' || method === 'notifications.take') return [] as T;
    return null as T;
  }
  on(method: string, h: (p: unknown) => void): () => void {
    this.handlers.set(method, [...(this.handlers.get(method) ?? []), h]);
    return () => undefined;
  }
}

const tick = () => new Promise((r) => setTimeout(r, 0));

describe('status Lark', () => {
  it('dimuat ulang saat daemon mengirim resync (sesi baru siap)', async () => {
    const rpc = new FakeRpc();
    const store = new LarkStore();
    store.attach(rpc, () => undefined);
    await tick();
    expect(store.status?.logged_in).toBe(false);
    rpc.loggedIn = true;
    for (const h of rpc.handlers.get('resync') ?? []) h(null);
    await tick();
    expect(store.status?.logged_in).toBe(true);
  });
});

describe('jeda tenang', () => {
  it('disimpan lewat waiting_fix.quiet.set dan nilai baru tampil', async () => {
    const calls: [string, unknown][] = [];
    const rpc: Rpc = {
      async request<T>(method: string, params?: unknown): Promise<T> {
        calls.push([method, params]);
        if (method === 'waiting_fix.quiet.set') return { quiet_secs: 300 } as T;
        return null as T;
      },
      on: () => () => undefined
    };
    const store = new LarkStore();
    store.attach(rpc, () => undefined);
    await store.setQuiet(300);
    expect(calls.some(([m, p]) => m === 'waiting_fix.quiet.set' && (p as { secs: number }).secs === 300)).toBe(true);
    expect(store.settings?.quiet_secs).toBe(300);
  });
});
