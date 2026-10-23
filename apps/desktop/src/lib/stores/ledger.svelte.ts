// Buku AC dan riwayat review satu run (Rilis 2). `ledger.updated` membawa baris
// terbaru sehingga progres review diterapkan per baris tanpa memuat ulang buku
// AC dari tracker; `resync` memuat ulang semuanya.

import type { AcLedgerView } from '#lib/protocol/AcLedgerView.ts';
import type { LedgerRowsEvent } from '#lib/protocol/LedgerRowsEvent.ts';
import type { ReviewView } from '#lib/protocol/ReviewView.ts';
import type { RunEventView } from '#lib/protocol/RunEventView.ts';
import type { Rpc } from '#lib/rpc.ts';

export class LedgerStore {
  ledger = $state<AcLedgerView | null>(null);
  history = $state<ReviewView[]>([]);
  error = $state<string | null>(null);
  private rpc: Rpc | null = null;
  private runId: string | null = null;
  private off: (() => void)[] = [];

  attach(rpc: Rpc, runId: string): () => void {
    this.detach();
    this.rpc = rpc;
    this.runId = runId;
    this.ledger = null;
    this.history = [];
    this.off = [
      rpc.on('ledger.updated', (p) => this.applyRows(p as LedgerRowsEvent)),
      rpc.on('review.progress', (p) => {
        if ((p as RunEventView).run_id === this.runId) void this.loadHistory();
      }),
      rpc.on('resync', () => void this.reload())
    ];
    void this.reload();
    return () => this.detach();
  }

  private detach(): void {
    for (const f of this.off) f();
    this.off = [];
  }

  /** Baris terbaru dari event; event run lain diabaikan. */
  applyRows(e: LedgerRowsEvent): void {
    if (e.run_id !== this.runId || !this.ledger) return;
    this.ledger = { ...this.ledger, rows: e.rows };
  }

  async reload(): Promise<void> {
    await Promise.all([this.load(), this.loadHistory()]);
  }

  async load(): Promise<void> {
    if (!this.rpc || !this.runId) return;
    try {
      this.ledger = await this.rpc.request<AcLedgerView>('ledger.get', { run_id: this.runId });
      this.error = null;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    }
  }

  async loadHistory(): Promise<void> {
    if (!this.rpc || !this.runId) return;
    try {
      this.history = await this.rpc.request<ReviewView[]>('review.history', { run_id: this.runId });
    } catch {
      this.history = [];
    }
  }
}
