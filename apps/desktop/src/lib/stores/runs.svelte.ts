// Run aktif lintas repo untuk panel agent (R1).

import { sortRuns } from '#lib/board.ts';
import type { RunView } from '#lib/protocol/RunView.ts';
import type { Rpc } from '#lib/rpc.ts';

export class RunsStore {
  runs = $state<RunView[]>([]);
  sorted = $derived(sortRuns(this.runs));
  private rpc: Rpc | null = null;
  private off: (() => void)[] = [];

  attach(rpc: Rpc): () => void {
    for (const f of this.off) f();
    this.rpc = rpc;
    this.off = [rpc.on('run.updated', () => void this.load()), rpc.on('resync', () => void this.load())];
    void this.load();
    return () => {
      for (const f of this.off) f();
      this.off = [];
    };
  }

  async load(): Promise<void> {
    if (!this.rpc) return;
    try {
      this.runs = await this.rpc.request<RunView[]>('runs.list');
    } catch {
      this.runs = [];
    }
  }

  byId(id: string): RunView | undefined {
    return this.runs.find((r) => r.id === id);
  }
}

export const runs = new RunsStore();
