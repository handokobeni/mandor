// Run kartu selesai yang bisa ditutup, untuk satu daemon (laptop atau VM)
// (plan 2026-11-03-001, R4, R5, R7, R9).
import type { CleanupSettingsView } from '#lib/protocol/CleanupSettingsView.ts';
import type { CloseFinishedResult } from '#lib/protocol/CloseFinishedResult.ts';
import type { FinishedRunView } from '#lib/protocol/FinishedRunView.ts';
import type { FinishedRunsView } from '#lib/protocol/FinishedRunsView.ts';
import type { RunStorageView } from '#lib/protocol/RunStorageView.ts';
import type { SkippedRunView } from '#lib/protocol/SkippedRunView.ts';
import type { Rpc } from '#lib/rpc.ts';

export class CleanupStore {
  closable = $state<FinishedRunView[]>([]);
  skipped = $state<SkippedRunView[]>([]);
  private rpc: Rpc | null = null;
  private off: (() => void)[] = [];

  /** Menyambung ke daemon; papan atau run yang berubah memuat ulang daftar. */
  attach(rpc: Rpc): () => void {
    this.detach();
    this.rpc = rpc;
    this.off = [rpc.on('board.updated', () => void this.load()), rpc.on('run.updated', () => void this.load())];
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

  /** Menutup semua run yang saat ini bisa ditutup; daemon memeriksa ulang tiap run. */
  async closeAll(): Promise<CloseFinishedResult> {
    const rpc = this.client();
    const res = await rpc.request<CloseFinishedResult>('runs.close_finished', {
      run_ids: this.closable.map((r) => r.run_id)
    });
    await this.load();
    return res;
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
