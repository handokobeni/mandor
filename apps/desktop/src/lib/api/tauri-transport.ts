// Transport ke mandord lewat jembatan Rust Tauri (relay stdio `mandord connect`,
// atau `ssh <vm> mandord connect` untuk link VM). Event dinamai per link.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { background } from '#lib/stores/background.svelte.ts';
import type { Transport } from './client';

/** Kunci link di sisi Rust. */
export type LinkKey = 'laptop' | 'vm';

export class TauriTransport implements Transport {
  private unlisten: UnlistenFn[] = [];

  constructor(private readonly key: LinkKey = 'laptop') {}

  async token(): Promise<{ token: string; protocol: string }> {
    // Daemon laptop dipasang atau diperbarui dulu (sekali per sesi app);
    // daemon VM dipasang lewat runbook.
    if (this.key === 'laptop') await background.ensureProvisioned();
    return invoke('daemon_token', { key: this.key });
  }

  async open(onLine: (line: string) => void, onClose: () => void): Promise<void> {
    for (const off of this.unlisten) off();
    this.unlisten = [
      await listen<string>(`daemon://${this.key}/line`, (e) => onLine(e.payload)),
      await listen(`daemon://${this.key}/closed`, () => onClose())
    ];
    await invoke('daemon_open', { key: this.key });
  }

  send(line: string): Promise<void> {
    return invoke('daemon_send', { key: this.key, message: line });
  }
}
