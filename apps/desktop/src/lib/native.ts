// Jembatan ke sisi native Tauri: callback OAuth dan notifikasi desktop.

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  isPermissionGranted,
  requestPermission,
  sendNotification
} from '@tauri-apps/plugin-notification';
import type { LoginBegin } from '#lib/protocol/LoginBegin.ts';
import type { NoticeView } from '#lib/protocol/NoticeView.ts';

/** Membuka browser dan menunggu kode OAuth di port callback loopback. */
export function catchOAuthCode(begin: LoginBegin): Promise<string> {
  return invoke<string>('oauth_login', { url: begin.url, port: begin.port, state: begin.state });
}

let permitted: boolean | null = null;

/** Notifikasi desktop (R2); diam bila izin ditolak. */
export async function notifyDesktop(n: NoticeView): Promise<void> {
  try {
    if (permitted === null) {
      permitted = (await isPermissionGranted()) || (await requestPermission()) === 'granted';
    }
    if (permitted) sendNotification({ title: n.title, body: n.body });
  } catch {
    permitted = false;
  }
}

/** Run yang menunggu dev untuk menu tray. */
export interface TrayRun {
  run_id: string;
  key: string;
  title: string;
}

/** Memperbarui menu tray; diam bila tidak berjalan di Tauri (mis. mode mock). */
export async function updateTray(waiting: TrayRun[]): Promise<void> {
  try {
    await invoke('tray_update', { waiting });
  } catch {
    // Tray hanya pelengkap.
  }
}

/** Run yang dipilih dari menu tray; mengembalikan fungsi pelepas. */
export function onTrayOpenRun(cb: (runId: string) => void): () => void {
  let off: (() => void) | null = null;
  let stopped = false;
  listen<string>('tray://open-run', (e) => cb(e.payload))
    .then((u) => {
      if (stopped) u();
      else off = u;
    })
    .catch(() => undefined);
  return () => {
    stopped = true;
    off?.();
  };
}
