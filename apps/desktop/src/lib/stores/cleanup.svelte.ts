// Run kartu selesai yang bisa ditutup, untuk satu daemon (laptop atau VM)
// (plan 2026-11-03-001, R4, R5, R7, R9).
import type { CleanupSettingsView } from '#lib/protocol/CleanupSettingsView.ts';
import type { CloseFinishedStarted } from '#lib/protocol/CloseFinishedStarted.ts';
import type { NoticeView } from '#lib/protocol/NoticeView.ts';
import type { FinishedRunView } from '#lib/protocol/FinishedRunView.ts';
import type { FinishedRunsView } from '#lib/protocol/FinishedRunsView.ts';
import type { RunStorageView } from '#lib/protocol/RunStorageView.ts';
import type { SkippedRunView } from '#lib/protocol/SkippedRunView.ts';
import type { Rpc } from '#lib/rpc.ts';

export class CleanupStore {
  closable = $state<FinishedRunView[]>([]);
  skipped = $state<SkippedRunView[]>([]);
  /** Daemon sedang menutup run di latar belakang. */
  closing = $state(false);
  /** Ringkasan penutupan terakhir ("Closed N finished runs, freed X"). */
  summary = $state<string | null>(null);
  private rpc: Rpc | null = null;
  private off: (() => void)[] = [];

  /** Menyambung ke daemon; papan atau run yang berubah memuat ulang daftar. */
  attach(rpc: Rpc): () => void {
    this.detach();
    this.rpc = rpc;
    this.off = [
      rpc.on('board.updated', () => void this.load()),
      rpc.on('run.updated', () => void this.load()),
      rpc.on('notice', (p) => {
        const n = p as NoticeView;
        if (n.kind !== 'runs_closed') return;
        this.closing = false;
        this.summary = n.body;
        void this.load();
      })
    ];
    void this.load();
    return () => this.detach();
  }

  detach(): void {
    for (const f of this.off) f();
    this.off = [];
    this.rpc = null;
    this.closable = [];
    this.skipped = [];
  }

  async load(): Promise<void> {
    if (!this.rpc) return;
    try {
      const view = await this.rpc.request<FinishedRunsView>('runs.finished');
      this.closable = view.closable;
      this.skipped = view.skipped;
    } catch {
      // Daemon versi lama atau sementara tidak terjangkau: tombol disembunyikan.
      this.closable = [];
      this.skipped = [];
    }
  }

  private client(): Rpc {
    if (!this.rpc) throw new Error('The daemon is not connected');
    return this.rpc;
  }

  /**
   * Meminta daemon menutup semua run yang saat ini bisa ditutup di latar
   * belakang; daemon memeriksa ulang tiap run. Mengembalikan jumlah run.
   * Selesai saat notifikasi `runs_closed` datang.
   */
  async closeAll(): Promise<number> {
    const rpc = this.client();
    const res = await rpc.request<CloseFinishedStarted>('runs.close_finished', {
      run_ids: this.closable.map((r) => r.run_id)
    });
    this.closing = res.count > 0;
    this.summary = null;
    return res.count;
  }

  /** Semua run terbuka, worktree terbesar dulu. */
  storage(): Promise<RunStorageView[]> {
    return this.client().request<RunStorageView[]>('runs.storage');
  }

  async autoClose(): Promise<boolean> {
    return (await this.client().request<CleanupSettingsView>('settings.cleanup.get')).auto_close;
  }

  async setAutoClose(on: boolean): Promise<void> {
    await this.client().request('settings.cleanup.set', { auto_close: on });
  }
}

/** Store untuk daemon laptop. */
export const cleanup = new CleanupStore();
/** Store untuk daemon VM (tab Overnight dan Settings › Storage target VM). */
export const vmCleanup = new CleanupStore();
