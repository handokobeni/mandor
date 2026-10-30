// Tab Overnight (U10 plan VM): antrean, setelan, laporan siklus, dan aksi atas
// kartu VM. Semua dibaca dari link VM; board tetap dari link laptop.

import { publishParams } from '#lib/ac-review.ts';
import type { AcLedgerView } from '#lib/protocol/AcLedgerView.ts';
import type { LoginCheck } from '#lib/protocol/LoginCheck.ts';
import type { OvernightQueueView } from '#lib/protocol/OvernightQueueView.ts';
import type { OvernightReportView } from '#lib/protocol/OvernightReportView.ts';
import type { OvernightSettingsView } from '#lib/protocol/OvernightSettingsView.ts';
import type { ReportCardView } from '#lib/protocol/ReportCardView.ts';
import type { RepoView } from '#lib/protocol/RepoView.ts';
import type { StartWorkspace } from '#lib/protocol/StartWorkspace.ts';
import type { StatusResult } from '#lib/protocol/StatusResult.ts';
import type { WorkspaceView } from '#lib/protocol/WorkspaceView.ts';
import type { Rpc } from '#lib/rpc.ts';

/** Selang muat ulang data VM. */
const POLL_MS = 30_000;

/** Target kartu saat diantre: satu repo, atau workspace. */
export type QueueTarget = { repo_id: string } | { workspace: StartWorkspace };

function message(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export class OvernightStore {
  queue = $state<OvernightQueueView | null>(null);
  settings = $state<OvernightSettingsView | null>(null);
  report = $state<OvernightReportView | null>(null);
  logins = $state<LoginCheck[]>([]);
  repos = $state<RepoView[]>([]);
  workspaces = $state<WorkspaceView[]>([]);
  error = $state<string | null>(null);
  /** Status daemon VM sudah terbaca sekali. */
  statusLoaded = $state(false);
  /** Daemon VM sudah login Lark; antrean, setelan, dan laporan butuh ini. */
  larkReady = $state(false);
  /** Daemon berperan VM (melaporkan status login VM); `false` = masih peran laptop. */
  vmRole = $state(false);
  private rpc: Rpc | null = null;

  /**
   * Kartu di antrean VM; `null` bila link VM tidak tersambung (antrean tidak
   * diketahui, badge "Queued for VM" tidak ditampilkan).
   */
  get queuedIds(): Set<string> | null {
    if (!this.rpc || !this.queue) return null;
    return new Set(this.queue.items.map((i) => i.record_id));
  }

  get connected(): boolean {
    return this.rpc !== null;
  }

  /** Menyambung ke client VM yang siap; mengembalikan fungsi pelepas. */
  attach(rpc: Rpc): () => void {
    this.rpc = rpc;
    void this.refresh();
    const timer = setInterval(() => void this.refresh(), POLL_MS);
    return () => {
      clearInterval(timer);
      if (this.rpc === rpc) this.detach();
    };
  }

  detach(): void {
    this.rpc = null;
    this.statusLoaded = false;
    this.queue = null;
    this.report = null;
  }

  async refresh(): Promise<void> {
    const rpc = this.rpc;
    if (!rpc) return;
    // Status dan repo selalu bisa dibaca; data lembur butuh sesi Lark di VM.
    try {
      const [status, repos, workspaces] = await Promise.all([
        rpc.request<StatusResult>('status'),
        rpc.request<RepoView[]>('repos.list'),
        rpc.request<WorkspaceView[]>('workspaces.list')
      ]);
      if (this.rpc !== rpc) return;
      this.statusLoaded = true;
      this.vmRole = status.logins !== undefined && status.logins !== null;
      this.logins = status.logins ?? [];
      this.larkReady = status.lark.logged_in && !status.lark.reauth_required;
      this.repos = repos;
      this.workspaces = workspaces;
    } catch (err) {
      this.error = message(err);
      return;
    }
    if (!this.larkReady) {
      this.queue = null;
      this.settings = null;
      this.report = null;
      this.error = null;
      return;
    }
    try {
      const [queue, settings, report] = await Promise.all([
        rpc.request<OvernightQueueView>('overnight.queue.list'),
        rpc.request<OvernightSettingsView | null>('overnight.settings.get'),
        rpc.request<OvernightReportView>('overnight.report')
      ]);
      if (this.rpc !== rpc) return;
      this.queue = queue;
      this.settings = settings;
      this.report = report;
      this.error = null;
    } catch (err) {
      this.error = message(err);
    }
  }

  private async call(method: string, params: unknown): Promise<void> {
    const rpc = this.rpc;
    if (!rpc) throw new Error('The VM is not connected');
    await rpc.request(method, params);
    await this.refresh();
  }

  /** Mengirim kartu To Do ke antrean VM (R6). */
  add(recordId: string, target: QueueTarget, note: string | null = null): Promise<void> {
    return this.call('overnight.queue.add', { record_id: recordId, repo_id: null, note, ...target });
  }

  remove(recordId: string): Promise<void> {
    return this.call('overnight.queue.remove', { record_id: recordId });
  }

  /** Memindah kartu `delta` posisi (−1 naik, +1 turun). */
  move(recordId: string, delta: number): Promise<void> {
    const ids = (this.queue?.items ?? []).map((i) => i.record_id);
    const from = ids.indexOf(recordId);
    const to = from + delta;
    if (from < 0 || to < 0 || to >= ids.length) return Promise.resolve();
    [ids[from], ids[to]] = [ids[to], ids[from]];
    return this.call('overnight.queue.reorder', { record_ids: ids });
  }

  /** "Start now" di luar jam lembur (R7). */
  startNow(recordId: string): Promise<void> {
    return this.call('overnight.queue.start_now', { record_id: recordId });
  }

  saveSettings(s: OvernightSettingsView): Promise<void> {
    return this.call('overnight.settings.set', s);
  }

  /** Keputusan AC untuk kartu `ac_gate`: VM langsung push dan membuat PR draft (R12a). */
  decide(runId: string, confirmed: string[], reason: string | null): Promise<void> {
    return this.call('overnight.gate_decide', publishParams(runId, confirmed, reason));
  }

  /** Jawaban untuk kartu tertahan; kartu kembali ke antrean di branch yang sama (R20). */
  answer(card: ReportCardView, note: string): Promise<void> {
    return this.call('overnight.queue.add', { record_id: card.record_id, repo_id: card.repo_id, note });
  }

  /** Buku AC kartu VM untuk keputusan gerbang. */
  async ledger(runId: string): Promise<AcLedgerView> {
    const rpc = this.rpc;
    if (!rpc) throw new Error('The VM is not connected');
    return rpc.request<AcLedgerView>('ledger.get', { run_id: runId });
  }

  /** "Send to review" untuk PR draft (R23). */
  sendToReview(runId: string): Promise<void> {
    return this.call('overnight.send_to_review', { run_id: runId });
  }

  /**
   * "Take over to laptop" (R22): VM push kerja malam dan memindah klaim ke
   * daemon laptop, lalu laptop memulihkan branch kartu. Mengembalikan id run
   * laptop.
   */
  async takeover(card: ReportCardView, laptop: Rpc, laptopDaemonId: string): Promise<string> {
    await this.call('runs.handoff', { record_id: card.record_id, to_daemon_id: laptopDaemonId });
    const run = await laptop.request<{ id: string }>('runs.takeover', {
      record_id: card.record_id,
      repo_id: card.repo_id
    });
    return run.id;
  }
}

export const overnight = new OvernightStore();
