// Status dan pengaturan Lark, repo, serta notifikasi desktop.

import type { LarkSettingsInput } from '#lib/protocol/LarkSettingsInput.ts';
import type { LarkSettingsView } from '#lib/protocol/LarkSettingsView.ts';
import type { LarkStatus } from '#lib/protocol/LarkStatus.ts';
import type { LoginBegin } from '#lib/protocol/LoginBegin.ts';
import type { NoticeView } from '#lib/protocol/NoticeView.ts';
import type { RepoView } from '#lib/protocol/RepoView.ts';
import type { StatusResult } from '#lib/protocol/StatusResult.ts';
import type { WorkspaceView } from '#lib/protocol/WorkspaceView.ts';
import type { Rpc } from '#lib/rpc.ts';
import { people } from '#lib/stores/people.svelte.ts';

/** Menangkap kode OAuth di sisi native (Tauri) atau fake di test. */
export type OAuthCatcher = (begin: LoginBegin) => Promise<string>;
/** Menampilkan notifikasi desktop. */
export type Notify = (n: NoticeView) => void;

export class LarkStore {
  status = $state<LarkStatus | null>(null);
  settings = $state<LarkSettingsView | null>(null);
  repos = $state<RepoView[]>([]);
  /** Workspace multi-repo (Settings › Workspaces). */
  workspaces = $state<WorkspaceView[]>([]);
  error = $state<string | null>(null);
  /** Status, pengaturan, dan repo sudah dimuat setidaknya sekali. */
  loaded = $state(false);
  private rpc: Rpc | null = null;
  private off: (() => void)[] = [];

  attach(rpc: Rpc, notify: Notify): () => void {
    for (const f of this.off) f();
    this.rpc = rpc;
    this.off = [
      rpc.on('notice', (p) => notify(p as NoticeView)),
      // Sesi Lark baru siap (mis. setelah ganti Base): status dan pengaturan berubah.
      rpc.on('resync', () => void this.refresh())
    ];
    void this.refresh();
    // Notifikasi yang terjadi saat UI tertutup.
    void rpc
      .request<NoticeView[]>('notifications.take')
      .then((list) => list.forEach(notify))
      .catch(() => undefined);
    return () => {
      for (const f of this.off) f();
      this.off = [];
    };
  }

  async refresh(): Promise<void> {
    if (!this.rpc) return;
    try {
      const s = await this.rpc.request<StatusResult>('status');
      this.status = s.lark;
      if (s.lark.open_id && s.lark.name) people.know(s.lark.open_id, s.lark.name);
      this.settings = await this.rpc.request<LarkSettingsView | null>('lark.settings.get');
      this.repos = await this.rpc.request<RepoView[]>('repos.list');
      this.workspaces = await this.rpc.request<WorkspaceView[]>('workspaces.list');
      this.error = null;
      this.loaded = true;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    }
  }

  async save(input: LarkSettingsInput): Promise<void> {
    if (!this.rpc) return;
    this.status = await this.rpc.request<LarkStatus>('lark.settings.set', input);
    await this.refresh();
  }

  /** Jeda tenang Waiting Fix; daemon menolak nilai di luar 30–900 detik. */
  async setQuiet(secs: number): Promise<void> {
    if (!this.rpc) return;
    this.settings = await this.rpc.request<LarkSettingsView>('waiting_fix.quiet.set', { secs });
  }

  /** Login: URL dari daemon, kode ditangkap native, ditukar oleh daemon. */
  async login(catcher: OAuthCatcher): Promise<void> {
    if (!this.rpc) return;
    const begin = await this.rpc.request<LoginBegin>('lark.login.begin');
    const code = await catcher(begin);
    this.status = await this.rpc.request<LarkStatus>('lark.login.complete', { code, state: begin.state });
    if (this.status.open_id && this.status.name) people.know(this.status.open_id, this.status.name);
  }

  async logout(): Promise<void> {
    if (!this.rpc) return;
    this.status = await this.rpc.request<LarkStatus>('lark.logout');
  }
}

export const lark = new LarkStore();
