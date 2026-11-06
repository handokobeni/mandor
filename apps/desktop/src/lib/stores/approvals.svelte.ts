// Kartu persetujuan satu run (plan 2026-11-06-001, U7): dimuat dari daemon,
// dimuat ulang saat `approvals.updated` untuk run ini.

import { sortCards } from '#lib/approvals.ts';
import type { ApprovalView } from '#lib/protocol/ApprovalView.ts';
import type { RunEventView } from '#lib/protocol/RunEventView.ts';
import type { Rpc } from '#lib/rpc.ts';

export type Answer = 'deny' | 'allow_once' | 'allow_run';

export class ApprovalsStore {
  cards = $state<ApprovalView[]>([]);
  /** Kartu yang jawabannya sedang dikirim (status "sedang dikirim" hanya di UI). */
  submitting = $state<Record<string, boolean>>({});
  /** Pesan per kartu, mis. jawaban terlambat. */
  notes = $state<Record<string, string>>({});
  pending = $derived(this.cards.filter((c) => c.status === 'pending').length);
  private rpc: Rpc | null = null;
  private runId: string | null = null;
  private off: (() => void)[] = [];

  attach(rpc: Rpc, runId: string): () => void {
    this.detach();
    this.rpc = rpc;
    this.runId = runId;
    this.cards = [];
    this.notes = {};
    this.off = [
      rpc.on('approvals.updated', (p) => {
        if ((p as RunEventView).run_id === this.runId) void this.load();
      }),
      rpc.on('resync', () => void this.load())
    ];
    void this.load();
    return () => this.detach();
  }

  private detach(): void {
    for (const f of this.off) f();
    this.off = [];
  }

  async load(): Promise<void> {
    if (!this.rpc || !this.runId) return;
    try {
      const cards = await this.rpc.request<ApprovalView[]>('approvals.list', { run_id: this.runId });
      this.cards = sortCards(cards);
    } catch {
      // Kartu muncul lagi lewat event berikutnya.
    }
  }

  async answer(id: string, answer: Answer): Promise<void> {
    if (!this.rpc || !this.runId || this.submitting[id]) return;
    this.submitting = { ...this.submitting, [id]: true };
    try {
      await this.rpc.request('approvals.answer', { run_id: this.runId, approval_id: id, answer });
    } catch {
      this.notes = { ...this.notes, [id]: 'This request is no longer pending.' };
    } finally {
      const { [id]: _done, ...rest } = this.submitting;
      this.submitting = rest;
      await this.load();
    }
  }

  /** Tab Chat run terlihat: mode tolak langsung berakhir (R19). */
  async seen(): Promise<void> {
    if (!this.rpc || !this.runId) return;
    try {
      await this.rpc.request('approvals.seen', { run_id: this.runId });
    } catch {
      // Dicoba lagi saat tab Chat terlihat berikutnya.
    }
  }
}
