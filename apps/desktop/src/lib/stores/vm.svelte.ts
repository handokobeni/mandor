// Link VM di app (U9 plan VM): host VM tersimpan di sisi Rust, host key dipin
// setelah developer mengonfirmasi fingerprint, lalu koneksi kedua dibuka.

import { invoke } from '@tauri-apps/api/core';
import { TauriTransport } from '#lib/api/tauri-transport.ts';
import { vmConnection } from '#lib/stores/connection.svelte.ts';

/** Host key VM yang menunggu konfirmasi. */
export interface HostScan {
  host: string;
  fingerprints: string[];
}

function message(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

class VmLink {
  host = $state<string | null>(null);
  scan = $state<HostScan | null>(null);
  error = $state<string | null>(null);
  busy = $state(false);
  private loaded = false;

  /** Memuat host tersimpan sekali, lalu menyambung bila ada. */
  async load(): Promise<void> {
    if (this.loaded) return;
    this.loaded = true;
    try {
      this.host = await invoke<string | null>('vm_host');
      if (this.host) this.connect();
    } catch (err) {
      this.error = message(err);
    }
  }

  private connect(): void {
    vmConnection.start(new TauriTransport('vm'));
  }

  /** Membaca host key VM untuk dikonfirmasi. */
  async check(host: string): Promise<void> {
    await this.run(async () => {
      this.scan = await invoke<HostScan>('vm_scan', { host });
    });
  }

  /** Mempin host key yang dikonfirmasi lalu menyambung. */
  async trust(): Promise<void> {
    const scan = this.scan;
    if (!scan) return;
    await this.run(async () => {
      await invoke('vm_trust', { host: scan.host });
      this.host = scan.host;
      this.scan = null;
      this.connect();
    });
  }

  cancel(): void {
    this.scan = null;
  }

  /** Melepas VM dari app; daemon VM tetap berjalan. */
  async remove(): Promise<void> {
    await this.run(async () => {
      vmConnection.stop();
      await invoke('vm_remove');
      this.host = null;
    });
  }

  private async run(f: () => Promise<void>): Promise<void> {
    this.busy = true;
    this.error = null;
    try {
      await f();
    } catch (err) {
      this.error = message(err);
    } finally {
      this.busy = false;
    }
  }
}

export const vm = new VmLink();
