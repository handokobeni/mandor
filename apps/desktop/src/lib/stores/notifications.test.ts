import { describe, expect, it } from 'vitest';
import type { NoticeView } from '#lib/protocol/NoticeView.ts';
import type { Rpc } from '#lib/rpc.ts';
import { NotificationStore } from './notifications.svelte.ts';

function notice(id: number, read = false): NoticeView {
  return { id, read, kind: 'failed', record_id: 'rec1', run_id: 'r1', title: `N${id}`, body: '', created_at_ms: id };
}

class FakeRpc implements Rpc {
  list: NoticeView[] = [notice(2), notice(1, true)];
  readCalls: unknown[] = [];
  handlers = new Map<string, (p: unknown) => void>();
  async request<T>(method: string, params?: unknown): Promise<T> {
    if (method === 'notifications.list') return this.list as T;
    if (method === 'notifications.read') {
      this.readCalls.push(params);
      return {} as T;
    }
    throw new Error(method);
  }
  on(method: string, h: (p: unknown) => void): () => void {
    this.handlers.set(method, h);
    return () => this.handlers.delete(method);
  }
}

const tick = () => new Promise((r) => setTimeout(r, 0));

describe('notification center', () => {
  it('memuat riwayat, menambah notifikasi baru di atas, dan menghitung yang belum dibaca', async () => {
    const rpc = new FakeRpc();
    const store = new NotificationStore();
    store.attach(rpc);
    await tick();
    expect(store.items.map((n) => n.id)).toEqual([2, 1]);
    expect(store.unread).toBe(1);
    rpc.handlers.get('notice')?.(notice(3));
    expect(store.items.map((n) => n.id)).toEqual([3, 2, 1]);
    expect(store.unread).toBe(2);
  });

  it('tandai satu dan tandai semua dibaca', async () => {
    const rpc = new FakeRpc();
    const store = new NotificationStore();
    store.attach(rpc);
    await tick();
    await store.markRead([2]);
    expect(rpc.readCalls).toEqual([{ ids: [2] }]);
    expect(store.unread).toBe(0);
    rpc.handlers.get('notice')?.(notice(4));
    await store.markAllRead();
    expect(rpc.readCalls.at(-1)).toEqual({ ids: null });
    expect(store.items.every((n) => n.read)).toBe(true);
  });
});
