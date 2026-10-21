// Riwayat notifikasi untuk notification center: dimuat dari daemon, ditambah event `notice`.

import type { NoticeView } from '#lib/protocol/NoticeView.ts';
import type { Rpc } from '#lib/rpc.ts';

export class NotificationStore {
  items = $state<NoticeView[]>([]);
  unread = $derived(this.items.filter((n) => !n.read).length);
  private rpc: Rpc | null = null;
  private off: (() => void) | null = null;

  attach(rpc: Rpc): () => void {
    this.off?.();
    this.rpc = rpc;
    this.off = rpc.on('notice', (p) => {
      const n = p as NoticeView;
      if (!this.items.some((x) => x.id === n.id)) this.items = [n, ...this.items];
    });
    void this.load();
    return () => this.off?.();
  }

  async load(): Promise<void> {
    if (!this.rpc) return;
    try {
      this.items = await this.rpc.request<NoticeView[]>('notifications.list');
    } catch {
      // Riwayat hanya pelengkap; notifikasi baru tetap datang lewat event.
    }
  }

  async markRead(ids: number[]): Promise<void> {
    if (!this.rpc || ids.length === 0) return;
    this.items = this.items.map((n) => (ids.includes(n.id) ? { ...n, read: true } : n));
    await this.rpc.request('notifications.read', { ids });
  }

  async markAllRead(): Promise<void> {
    if (!this.rpc) return;
    this.items = this.items.map((n) => ({ ...n, read: true }));
    await this.rpc.request('notifications.read', { ids: null });
  }
}

export const notifications = new NotificationStore();
