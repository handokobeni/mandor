// Preferensi notifikasi desktop per jenis (Settings › General), disimpan per perangkat.

import { DEFAULT_NOTIFY, type NotifyPrefs } from '#lib/settings.ts';

const KEY = 'mandor:notify';

function read(): NotifyPrefs {
  try {
    const raw = localStorage.getItem(KEY);
    return raw ? { ...DEFAULT_NOTIFY, ...(JSON.parse(raw) as NotifyPrefs) } : { ...DEFAULT_NOTIFY };
  } catch {
    return { ...DEFAULT_NOTIFY };
  }
}

class NotifyPrefsStore {
  prefs = $state<NotifyPrefs>(read());

  set(kind: string, on: boolean): void {
    this.prefs = { ...this.prefs, [kind]: on };
    try {
      localStorage.setItem(KEY, JSON.stringify(this.prefs));
    } catch {
      // Tanpa penyimpanan, preferensi berlaku sampai app ditutup.
    }
  }
}

export const notifyPrefs = new NotifyPrefsStore();
