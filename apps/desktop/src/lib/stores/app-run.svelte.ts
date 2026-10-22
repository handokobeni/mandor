// State Run app (Rilis 2): app yang sedang menyala (pemilik, untuk sidebar) dan
// state app run yang dibuka. `app.updated` memuat ulang keduanya.

import type { AppRunView } from '#lib/protocol/AppRunView.ts';
import type { RunEventView } from '#lib/protocol/RunEventView.ts';
import type { Rpc } from '#lib/rpc.ts';

export class AppRunStore {
  owner = $state<AppRunView | null>(null);
  current = $state<AppRunView | null>(null);
  private rpc: Rpc | null = null;
  private runId: string | null = null;
  private off: (() => void)[] = [];

  attach(rpc: Rpc): () => void {
    for (const f of this.off) f();
    this.rpc = rpc;
    this.off = [
      rpc.on('app.updated', (p) => {
        void this.loadOwner();
        if ((p as RunEventView).run_id === this.runId) void this.loadCurrent();
      }),
      rpc.on('resync', () => void this.reload())
    ];
    void this.loadOwner();
    return () => {
      for (const f of this.off) f();
      this.off = [];
    };
  }

  /** Run yang dibuka di workspace. */
  watch(runId: string | null): void {
    this.runId = runId;
    this.current = null;
    void this.loadCurrent();
  }

  async reload(): Promise<void> {
    await Promise.all([this.loadOwner(), this.loadCurrent()]);
  }

  async loadOwner(): Promise<void> {
    if (!this.rpc) return;
    try {
      this.owner = await this.rpc.request<AppRunView | null>('app.owner');
    } catch {
      this.owner = null;
    }
  }

  async loadCurrent(): Promise<void> {
    if (!this.rpc || !this.runId) return;
    try {
      this.current = await this.rpc.request<AppRunView>('app.status', { run_id: this.runId });
    } catch {
      this.current = null;
    }
  }

  /** App run lain yang akan dihentikan bila run ini menyalakan app-nya. */
  otherOwner(runId: string): AppRunView | null {
    const o = this.owner;
    return o && o.run_id !== runId && ['running', 'starting', 'setting_up', 'waiting_for_port'].includes(o.state)
      ? o
      : null;
  }
}

export const appRun = new AppRunStore();
