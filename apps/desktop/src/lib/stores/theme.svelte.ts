// Tema tampilan: system / dark / light (Settings › General). Default dark.
// Preferensi disimpan per perangkat di localStorage; gagal baca/tulis diabaikan.

export type ThemePreference = 'system' | 'dark' | 'light';

const KEY = 'mandor:theme';

function readPreference(): ThemePreference {
  try {
    const v = localStorage.getItem(KEY);
    return v === 'system' || v === 'light' || v === 'dark' ? v : 'dark';
  } catch {
    return 'dark';
  }
}

class ThemeStore {
  preference = $state<ThemePreference>('dark');
  systemDark = $state(true);
  resolved = $derived<'dark' | 'light'>(
    this.preference === 'system' ? (this.systemDark ? 'dark' : 'light') : this.preference
  );

  /** Membaca preferensi dan mengikuti perubahan tema OS; mengembalikan fungsi pelepas. */
  start(): () => void {
    this.preference = readPreference();
    const media = window.matchMedia('(prefers-color-scheme: dark)');
    this.systemDark = media.matches;
    const onChange = (e: MediaQueryListEvent) => (this.systemDark = e.matches);
    media.addEventListener('change', onChange);
    return () => media.removeEventListener('change', onChange);
  }

  set(pref: ThemePreference): void {
    this.preference = pref;
    try {
      localStorage.setItem(KEY, pref);
    } catch {
      // Penyimpanan tidak tersedia: preferensi hanya berlaku sampai app ditutup.
    }
  }
}

export const theme = new ThemeStore();
