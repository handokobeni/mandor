import { describe, expect, it } from 'vitest';
import type { Rpc } from '#lib/rpc.ts';
import { CleanupStore } from './cleanup.svelte.ts';

const run = (id: string, bytes: number | null) => ({ run_id: id, keys: [`PGS-${id}`], title: id, bytes });

class FakeDaemon implements Rpc {
  calls: [string, unknown][] = [];
  closable = [run('1', 1000), run('2', null)];
  auto = false;
  handlers: Record<string, (p: unknown) => void> = {};
  async request<T>(method: string, params?: unknown): Promise<T> {
    this.calls.push([method, params]);
    switch (method) {
      case 'runs.finished':
        return { closable: this.closable, skipped: [{ run: run('3', 5), reason: 'unpushed_changes' }] } as T;
      case 'runs.close_finished': {
        const ids = (params as { run_ids: string[] }).run_ids;
        this.closable = this.closable.filter((r) => !ids.includes(r.run_id));
        return { closed: ids, skipped: [], freed_bytes: 1000, summary: 'Closed 2 finished runs, freed 1.0 KB' } as T;
      }
      case 'runs.storage':
        return [{ run_id: '9', keys: ['PGS-9'], title: 'Besar', run_state: 'awaiting_dev', card_status: ['In Progress'], bytes: 9, updated_at_ms: 1 }] as T;
      case 'settings.cleanup.get':
        return { auto_close: this.auto } as T;
      case 'settings.cleanup.set':
        this.auto = (params as { auto_close: boolean }).auto_close;
        return {} as T;
      default:
        return {} as T;
    }
  }
  on(event: string, h: (p: unknown) => void): () => void {
    this.handlers[event] = h;
    return () => delete this.handlers[event];
  }
}

describe('store penutupan run selesai', () => {
  it('memuat run selesai saat tersambung dan setiap papan berubah', async () => {
    const d = new FakeDaemon();
    const store = new CleanupStore();
    store.attach(d);
    await store.load();
    expect(store.closable.map((r) => r.run_id)).toEqual(['1', '2']);
    expect(store.skipped[0].reason).toBe('unpushed_changes');
    d.closable = [];
    d.handlers['board.updated'](null);
    await store.load();
    expect(store.closable).toEqual([]);
  });

  it('menutup run yang bisa ditutup lalu memuat ulang', async () => {
    const d = new FakeDaemon();
    const store = new CleanupStore();
    store.attach(d);
    await store.load();
    const res = await store.closeAll();
    expect(d.calls.find(([m]) => m === 'runs.close_finished')?.[1]).toEqual({ run_ids: ['1', '2'] });
    expect(res.summary).toBe('Closed 2 finished runs, freed 1.0 KB');
    expect(store.closable).toEqual([]);
  });

  it('daftar storage dan setelan otomatis', async () => {
    const d = new FakeDaemon();
    const store = new CleanupStore();
    store.attach(d);
    expect((await store.storage())[0].keys).toEqual(['PGS-9']);
    expect(await store.autoClose()).toBe(false);
    await store.setAutoClose(true);
    expect(d.calls.at(-1)).toEqual(['settings.cleanup.set', { auto_close: true }]);
    expect(await store.autoClose()).toBe(true);
  });

  it('tanpa daemon tersambung tidak ada run', async () => {
    const store = new CleanupStore();
    await store.load();
    expect(store.closable).toEqual([]);
    await expect(store.closeAll()).rejects.toThrow('not connected');
  });
});
