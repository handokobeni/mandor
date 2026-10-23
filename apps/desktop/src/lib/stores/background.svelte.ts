// Kerja latar (U8 plan service OS): provisioning daemon oleh app, status
// daemon, keep-alive WSL, dan keep-awake milik app untuk daemon lama.

import { invoke } from '@tauri-apps/api/core';
import {
  appShouldHoldKeepAwake,
  shouldRetryProvision,
  type KeepAliveView,
  type ProvisionState
} from '#lib/background.ts';
import type { StatusResult } from '#lib/protocol/StatusResult.ts';
import type { Rpc } from '#lib/rpc.ts';

/** Selang pemeriksaan status daemon. */
const POLL_MS = 30_000;

function message(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

class Background {
  provision = $state<ProvisionState | null>(null);
  status = $state<StatusResult | null>(null);
  keepAlive = $state<KeepAliveView | null>(null);
  keepAliveError = $state<string | null>(null);
  private running: Promise<ProvisionState> | null = null;
  private appHold = false;

  /** App memakai daemon di WSL (provisioning berjalan). */
  get wsl(): boolean {
    const s = this.provision?.state;
    return s !== undefined && s !== 'not_needed';
  }

  /** Memasang atau memperbarui daemon sekali; dipanggil sebelum tersambung. */
  ensureProvisioned(): Promise<ProvisionState> {
    this.running ??= this.run();
    return this.running;
  }

  private async run(): Promise<ProvisionState> {
    try {
      const p = await invoke<ProvisionState>('provision_run');
      this.provision = p;
      return p;
    } catch (err) {
      const p: ProvisionState = { state: 'failed', message: message(err) };
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
