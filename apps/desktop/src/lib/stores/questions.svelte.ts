// Kartu pertanyaan agent satu run (plan 2026-11-07-001, U6): dimuat dari
// daemon, dimuat ulang saat `questions.updated` untuk run ini.

import type { QuestionView } from '#lib/protocol/QuestionView.ts';
import type { RunEventView } from '#lib/protocol/RunEventView.ts';
import type { Rpc } from '#lib/rpc.ts';

export class QuestionsStore {
  questions = $state<QuestionView[]>([]);
  /** Kartu yang jawabannya sedang dikirim. */
  submitting = $state<Record<string, boolean>>({});
  /** Catatan error per kartu, mis. kiriman gagal (R9). */
  notes = $state<Record<string, string>>({});
  private rpc: Rpc | null = null;
  private runId: string | null = null;
  private off: (() => void)[] = [];

  attach(rpc: Rpc, runId: string): () => void {
    this.detach();
    this.rpc = rpc;
    this.runId = runId;
    this.questions = [];
    this.notes = {};
    this.off = [
      rpc.on('questions.updated', (p) => {
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

  byId(id: string): QuestionView | undefined {
    return this.questions.find((q) => q.id === id);
  }

  async load(): Promise<void> {
    if (!this.rpc || !this.runId) return;
    try {
      this.questions = await this.rpc.request<QuestionView[]>('questions.list', { run_id: this.runId });
    } catch {
      // Kartu muncul lagi lewat event berikutnya.
    }
  }

  /** Klik satu pilihan (R8); gagal → kartu aktif lagi dengan catatan (R9). */
  async answer(id: string, option: number): Promise<void> {
    if (!this.rpc || this.submitting[id]) return;
    this.submitting = { ...this.submitting, [id]: true };
    const { [id]: _old, ...notes } = this.notes;
    this.notes = notes;
    try {
      await this.rpc.request('questions.answer', { question_id: id, option });
    } catch (e) {
      const why = e instanceof Error && e.message ? e.message : 'try again';
      this.notes = { ...this.notes, [id]: `Could not send the answer: ${why}` };
    } finally {
      const { [id]: _done, ...rest } = this.submitting;
      this.submitting = rest;
      await this.load();
    }
  }
}
