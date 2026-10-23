// Transport ke mandord lewat jembatan Rust Tauri (relay stdio `mandord connect`).

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { background } from '#lib/stores/background.svelte.ts';
import type { Transport } from './client';

export class TauriTransport implements Transport {
  private unlisten: UnlistenFn[] = [];

  async token(): Promise<{ token: string; protocol: string }> {
    // Daemon dipasang atau diperbarui dulu (sekali per sesi app).
    await background.ensureProvisioned();
    return invoke('daemon_token');
  }

  async open(onLine: (line: string) => void, onClose: () => void): Promise<void> {
    for (const off of this.unlisten) off();
    this.unlisten = [
      await listen<string>('daemon://line', (e) => onLine(e.payload)),
      await listen('daemon://closed', () => onClose())
    ];
    await invoke('daemon_open');
  }

  send(line: string): Promise<void> {
    return invoke('daemon_send', { message: line });
  }
}
