// Pesan chat per run (dimuat bertahap) dan draft yang bertahan saat pindah workspace.

import { SvelteMap } from 'svelte/reactivity';
import type { MessageView } from '#lib/protocol/MessageView.ts';
import type { Rpc } from '#lib/rpc.ts';

const PAGE = 200;

export class WorkspaceStore {
  selected = $state<string | null>(null);
  messages = new SvelteMap<string, MessageView[]>();
  drafts = new SvelteMap<string, string>();
  /** Pesan dev per run yang sedang dikirim; hilang saat daemon mencatatnya. */
  sending = new SvelteMap<string, string>();
  private rpc: Rpc | null = null;
  private off: (() => void) | null = null;
  private loading = new Set<string>();

  attach(rpc: Rpc): () => void {
    this.off?.();
    this.rpc = rpc;
    this.off = rpc.on('run.message', (p) => {
      const runId = (p as { run_id?: string } | null)?.run_id;
      if (runId && this.messages.has(runId)) void this.fetchNew(runId);
    });
    return () => this.off?.();
  }

  open(runId: string): void {
    this.selected = runId;
    if (!this.messages.has(runId)) void this.fetchNew(runId);
  }

  close(): void {
    this.selected = null;
  }

  draft(runId: string): string {
    return this.drafts.get(runId) ?? '';
  }

  setDraft(runId: string, text: string): void {
    if (text) this.drafts.set(runId, text);
    else this.drafts.delete(runId);
  }

  /** Pesan dikirim: tampil di chat sebelum daemon mencatatnya. */
  beginSend(runId: string, text: string): void {
    this.sending.set(runId, text);
  }

  /** Kiriman selesai tanpa pesan tercatat (gagal), atau sudah tercatat. */
  endSend(runId: string): void {
    this.sending.delete(runId);
  }

  /** Mengambil pesan setelah pesan terakhir yang sudah dimuat. */
  async fetchNew(runId: string): Promise<void> {
    if (!this.rpc || this.loading.has(runId)) return;
    this.loading.add(runId);
    try {
      for (;;) {
        const have = this.messages.get(runId) ?? [];
        const after = have.at(-1)?.id ?? 0;
        const page = await this.rpc.request<MessageView[]>('runs.messages', {
          run_id: runId,
          after_id: after,
          limit: PAGE
        });
        this.messages.set(runId, [...have, ...page]);
        // Pesan dev yang tercatat menggantikan pesan sementara.
        if (page.some((m) => m.role === 'dev')) this.sending.delete(runId);
        if (page.length < PAGE) break;
      }
    } finally {
      this.loading.delete(runId);
    }
  }
}

export const workspace = new WorkspaceStore();
