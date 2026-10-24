// Kerja latar (U8 plan service OS): provisioning daemon oleh app, status
// daemon, keep-alive WSL, dan keep-awake milik app untuk daemon lama.

import { invoke } from '@tauri-apps/api/core';
import {
  appShouldHoldKeepAwake,
  shouldRetryProvision,
  type InstallReport,
  type KeepAliveView,
  type ProvisionView,
  type Runner
} from '#lib/background.ts';
import type { StatusResult } from '#lib/protocol/StatusResult.ts';
import type { Rpc } from '#lib/rpc.ts';

/** Selang pemeriksaan status daemon. */
const POLL_MS = 30_000;

function message(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

class Background {
  provision = $state<ProvisionView | null>(null);
  status = $state<StatusResult | null>(null);
  keepAlive = $state<KeepAliveView | null>(null);
  keepAliveError = $state<string | null>(null);
  private running: Promise<ProvisionView> | null = null;
  private appHold = false;

  /** Tempat app memasang daemon; `null` bila tidak memasang (build dev). */
  get runner(): Runner | null {
    return this.provision?.runner ?? null;
  }

  /** App memakai daemon di WSL. */
  get wsl(): boolean {
    return this.runner === 'wsl';
  }

  /** Memasang atau memperbarui daemon sekali; dipanggil sebelum tersambung. */
  ensureProvisioned(): Promise<ProvisionView> {
    this.running ??= this.run();
    return this.running;
  }

  private async run(): Promise<ProvisionView> {
    try {
      const p = await invoke<ProvisionView>('provision_run');
      this.provision = p;
      return p;
    } catch (err) {
      const p: ProvisionView = { state: 'failed', message: message(err), runner: this.runner };
      this.provision = p;
      return p;
    }
  }

  /** Memantau status daemon selama koneksi siap; mengembalikan pelepas. */
  attach(rpc: Rpc): () => void {
    let stopped = false;
    const tick = async (): Promise<void> => {
      try {
        const status = await rpc.request<StatusResult>('status');
        if (stopped) return;
        this.status = status;
        if (shouldRetryProvision(this.provision, status)) {
          this.running = null;
          void this.ensureProvisioned();
        }
        await this.setAppHold(appShouldHoldKeepAwake(status));
      } catch {
        // Koneksi terputus: client tersambung ulang dan attach dipanggil lagi.
      }
    };
    void tick();
    const timer = setInterval(() => void tick(), POLL_MS);
    return () => {
      stopped = true;
      clearInterval(timer);
    };
  }

  private async setAppHold(hold: boolean): Promise<void> {
    if (hold === this.appHold) return;
    try {
      await invoke('power_app_hold', { hold });
      this.appHold = hold;
    } catch {
      // Di luar Tauri (mode mock) tidak ada pegangan app.
    }
  }

  /** Melepas service daemon di Linux sebelum paket dihapus. */
  async removeService(): Promise<InstallReport> {
    try {
      return await invoke<InstallReport>('daemon_uninstall');
    } catch (err) {
      return {
        result: 'error',
        service: 'none',
        linger: false,
        apps_stopped: false,
        warnings: [],
        message: message(err)
      };
    }
  }

  async loadKeepAlive(): Promise<void> {
    try {
      this.keepAlive = await invoke<KeepAliveView>('wsl_keepalive_status');
      this.keepAliveError = null;
    } catch (err) {
      this.keepAlive = null;
      this.keepAliveError = message(err);
    }
  }

  async setKeepAlive(enabled: boolean): Promise<void> {
    try {
      await invoke('wsl_keepalive_set', { enabled });
      this.keepAliveError = null;
    } catch (err) {
      this.keepAliveError = message(err);
    }
    await this.loadKeepAlive();
  }
}

export const background = new Background();
