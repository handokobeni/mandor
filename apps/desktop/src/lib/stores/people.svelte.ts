// Nama orang dari open_id (Contact API lewat daemon). Tanpa nama, open_id ditampilkan.

import { SvelteMap } from 'svelte/reactivity';
import type { PersonView } from '#lib/protocol/PersonView.ts';
import type { Rpc } from '#lib/rpc.ts';

export class PeopleStore {
  private names = new SvelteMap<string, string>();
  // Bukan state: ditulis saat render, jadi tidak boleh memicu reaktivitas.
  private requested = new Set<string>();
  private pending: string[] = [];
  private rpc: Rpc | null = null;

  attach(rpc: Rpc): () => void {
    this.rpc = rpc;
    // Sambungan baru: nama yang belum dikenal boleh diminta lagi.
    this.requested = new Set(this.names.keys());
    return () => {
      if (this.rpc === rpc) this.rpc = null;
    };
  }

  /** Nama yang sudah diketahui tanpa Contact API, mis. akun yang login. */
  know(openId: string, name: string): void {
    this.names.set(openId, name);
  }

  /** Nama tampilan; permintaan nama yang belum dikenal digabung ke satu RPC. */
  name(openId: string): string {
    const known = this.names.get(openId);
    if (known) return known;
    if (this.rpc && !this.requested.has(openId)) {
      this.requested.add(openId);
      this.pending.push(openId);
      if (this.pending.length === 1) queueMicrotask(() => void this.flush());
    }
    return openId;
  }

  private async flush(): Promise<void> {
    const ids = this.pending;
    this.pending = [];
    if (!this.rpc || ids.length === 0) return;
    try {
      const list = await this.rpc.request<PersonView[]>('people.names', { open_ids: ids });
      for (const p of list) this.names.set(p.open_id, p.name);
    } catch {
      // Scope Contact belum disetujui atau Lark gagal: open_id tetap dipakai.
    }
  }
}

export const people = new PeopleStore();
