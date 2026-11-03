import { describe, expect, it } from 'vitest';
import type { Rpc } from '#lib/rpc.ts';
import { CleanupStore } from './cleanup.svelte.ts';

const run = (id: string, bytes: number | null) => ({ run_id: id, keys: [`PGS-${id}`], title: id, bytes });

class FakeDaemon implements Rpc {
  calls: [string, unknown][] = [];
  closable = [run('1', 1000), run('2', null)];
  auto = false;
  closingIds: string[] = [];
  /** Daemon selesai menutup di latar belakang: run hilang dan notifikasi dikirim. */
  finishClosing(summary: string): void {
    this.closable = this.closable.filter((r) => !this.closingIds.includes(r.run_id));
    this.handlers['notice']({ id: 1, read: false, kind: 'runs_closed', record_id: '', run_id: null, title: 'Finished runs closed', body: summary, created_at_ms: 0 });
  }
  handlers: Record<string, (p: unknown) => void> = {};
  async request<T>(method: string, params?: unknown): Promise<T> {
    this.calls.push([method, params]);
    switch (method) {
      case 'runs.finished':
        return { closable: this.closable, skipped: [{ run: run('3', 5), reason: 'unpushed_changes' }] } as T;
      case 'runs.close_finished': {
        const ids = (params as { run_ids: string[] }).run_ids;
        this.closingIds = ids;
        return { count: ids.length } as T;
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

  it('menutup di latar belakang: status menutup sampai notifikasi runs_closed datang', async () => {
    const d = new FakeDaemon();
    const store = new CleanupStore();
    store.attach(d);
    await store.load();
    expect(await store.closeAll()).toBe(2);
    expect(d.calls.find(([m]) => m === 'runs.close_finished')?.[1]).toEqual({ run_ids: ['1', '2'] });
    expect(store.closing).toBe(true);
    d.finishClosing('Closed 2 finished runs, freed 1.0 KB');
    await store.load();
    expect(store.closing).toBe(false);
    expect(store.summary).toBe('Closed 2 finished runs, freed 1.0 KB');
    expect(store.closable).toEqual([]);
  });

  it('notifikasi jenis lain tidak mengubah status menutup', async () => {
    const d = new FakeDaemon();
    const store = new CleanupStore();
    store.attach(d);
    await store.load();
    await store.closeAll();
    d.handlers['notice']({ kind: 'pr_created', body: 'x' });
    expect(store.closing).toBe(true);
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
