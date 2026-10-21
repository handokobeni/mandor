// Papan kanban: kartu dari daemon, dimuat ulang saat event `board.updated`.

import { filterCards, groupByColumn, sprintOptions, sprintsOf } from '#lib/board.ts';
import type { SprintView } from '#lib/protocol/SprintView.ts';
import type { SyncStatusView } from '#lib/protocol/SyncStatusView.ts';
import type { BoardView } from '#lib/protocol/BoardView.ts';
import type { CardView } from '#lib/protocol/CardView.ts';
import type { Rpc } from '#lib/rpc.ts';

/** Batas menunggu hasil poll setelah Refresh. */
const SYNC_TIMEOUT_MS = 30_000;

export class BoardStore {
  cards = $state<CardView[]>([]);
  me = $state<string | null>(null);
  sprint = $state<string | null>(null);
  query = $state('');
  // Bawaan: papan sendiri; Team berisi kartu seluruh tim.
  mineOnly = $state(true);
  error = $state<string | null>(null);
  loading = $state(false);
  /** Belum pernah berhasil memuat: tampilkan skeleton. */
  loaded = $state(false);
  /** Poll Lark terakhir yang berhasil di daemon (ms epoch). */
  syncedAt = $state<number | null>(null);
  /** Error poll Lark terakhir di daemon. */
  syncError = $state<string | null>(null);
  /** Refresh diminta dan hasil poll belum tiba. */
  syncing = $state(false);
  private syncTimer: ReturnType<typeof setTimeout> | null = null;

  visible = $derived(filterCards(this.cards, { sprint: this.sprint, query: this.query, mineOnly: this.mineOnly }));
  columns = $derived(groupByColumn(this.visible));
  /** Data sprint dari Lark (nama, tanggal) untuk sprint yang dirujuk kartu. */
  sprintInfo = $state<SprintView[]>([]);
  sprints = $derived(sprintsOf(this.cards));
  sprintOpts = $derived(sprintOptions(this.sprints, this.sprintInfo));

  private rpc: Rpc | null = null;
  private off: (() => void)[] = [];
  private reloadQueued = false;

  /** Menyambung ke client dan memuat papan; event berikutnya memicu muat ulang. */
  attach(rpc: Rpc): () => void {
    this.detach();
    this.rpc = rpc;
    this.off = [
      rpc.on('board.updated', () => void this.load()),
      rpc.on('run.updated', () => void this.load()),
      rpc.on('resync', () => void this.load()),
      // Hasil poll Lark: perbarui indikator tanpa memuat ulang papan.
      rpc.on('sync.status', (p) => {
        const v = p as SyncStatusView;
        this.syncedAt = v.synced_at_ms;
        this.syncError = v.sync_error;
        this.endSync();
      })
    ];
    void this.load();
    return () => this.detach();
  }

  detach(): void {
    for (const f of this.off) f();
    this.off = [];
  }

  async load(): Promise<void> {
    if (!this.rpc) return;
    if (this.loading) {
      // Event beruntun digabung menjadi satu muat ulang tambahan.
      this.reloadQueued = true;
      return;
    }
    this.loading = true;
    try {
      const view = await this.rpc.request<BoardView>('board.list');
      this.cards = view.cards;
      this.me = view.me;
      this.sprintInfo = view.sprints;
      this.syncedAt = view.synced_at_ms;
      this.syncError = view.sync_error;
      this.error = null;
      this.loaded = true;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loading = false;
    }
    if (this.reloadQueued) {
      this.reloadQueued = false;
      await this.load();
    }
  }

  /** Menandai sinkron berjalan; selesai saat event `sync.status` tiba, paling lama 30 detik. */
  beginSync(): void {
    this.syncing = true;
    if (this.syncTimer) clearTimeout(this.syncTimer);
    this.syncTimer = setTimeout(() => this.endSync(), SYNC_TIMEOUT_MS);
  }

  private endSync(): void {
    this.syncing = false;
    if (this.syncTimer) clearTimeout(this.syncTimer);
    this.syncTimer = null;
  }

  clearFilters(): void {
    this.sprint = null;
    this.query = '';
    this.mineOnly = false;
  }

  sprintName(id: string): string {
    return this.sprintInfo.find((s) => s.id === id)?.name ?? id;
  }

  card(recordId: string): CardView | undefined {
    return this.cards.find((c) => c.record_id === recordId);
  }
}

export const board = new BoardStore();
