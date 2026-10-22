// Logika Run app dan checklist kesiapan (Rilis 2, brief §4.4): label state,
// kontrol action bar, item kesiapan, dan model dialog konfirmasi. Murni agar teruji.

import type { Tone } from '#lib/board.ts';
import type { AppChangesView } from '#lib/protocol/AppChangesView.ts';
import type { AppRunView } from '#lib/protocol/AppRunView.ts';
import type { ReviewReadinessView } from '#lib/protocol/ReviewReadinessView.ts';

export interface AppStateMeta {
  label: string;
  tone: Tone;
  spin: boolean;
}

const STATES: Record<string, AppStateMeta> = {
  off: { label: 'App off', tone: 'muted', spin: false },
  setting_up: { label: 'Setting up', tone: 'working', spin: true },
  starting: { label: 'Starting', tone: 'working', spin: true },
  waiting_for_port: { label: 'Waiting for port', tone: 'working', spin: true },
  running: { label: 'Running', tone: 'ok', spin: false },
  failed: { label: 'Failed to start', tone: 'failed', spin: false },
  stopped_by_other_run: { label: 'Stopped: another run started its app', tone: 'muted', spin: false }
};

export function appStateMeta(state: string): AppStateMeta {
  return STATES[state] ?? STATES.off;
}

/** Proses app sedang dinyalakan. */
export function appStarting(state: string): boolean {
  return ['setting_up', 'starting', 'waiting_for_port'].includes(state);
}

/** Label kontrol action bar, mis. "Starting kelola-web (2 of 2)". */
export function appControlLabel(app: AppRunView | null): string {
  if (!app) return STATES.off.label;
  const meta = appStateMeta(app.state);
  if (!appStarting(app.state)) {
    if (app.state === 'failed') {
      const failed = app.members.find((m) => m.state === 'failed');
      return failed ? `${meta.label} · ${failed.repo_id}` : meta.label;
    }
    return meta.label;
  }
  const i = app.members.findIndex((m) => appStarting(m.state));
  if (i < 0) return meta.label;
  const m = app.members[i];
  const count = app.members.length > 1 ? ` (${i + 1} of ${app.members.length})` : '';
  return `${appStateMeta(m.state).label} ${m.repo_id}${count}`;
}

/** Tombol kontrol: jalankan, hentikan, atau lihat log. */
export function appControlAction(app: AppRunView | null): 'run' | 'stop' | 'logs' {
  if (!app) return 'run';
  if (app.state === 'failed') return 'logs';
  if (app.state === 'running' || appStarting(app.state)) return 'stop';
  return 'run';
}

export type ItemState = 'ok' | 'missing' | 'warn' | 'confirm';

export interface ReadinessItem {
  key: 'database' | 'app' | 'login' | 'extension' | 'browser';
  state: ItemState;
  title: string;
  help: string;
  action?: 'run_app' | 'restart_app' | 'recheck';
}

/** Item checklist kesiapan dari fakta daemon dan konfirmasi di dialog (KTD-39). */
export function readinessItems(
  r: ReviewReadinessView,
  confirmed: { database: boolean; login: boolean },
  owner: string | null
): ReadinessItem[] {
  const url = r.app_urls[0] ?? 'the app URL';
  const app: ReadinessItem =
    r.app === 'current'
      ? { key: 'app', state: 'ok', title: 'App is running from the latest code', help: url }
      : r.app === 'older_code'
        ? {
            key: 'app',
            state: 'warn',
            title: 'App is running older code',
            help: 'The code changed after the app started. Restart so the reviewer sees the latest changes.',
            action: 'restart_app'
          }
        : {
            key: 'app',
            state: 'missing',
            title: 'App is not running',
            help: owner
              ? `The app running now belongs to ${owner}. Start this run's app with Run app.`
              : 'Start it with Run app. The reviewer uses it in the browser.',
            action: 'run_app'
          };
  return [
    {
      key: 'database',
      state: confirmed.database ? 'ok' : 'confirm',
      title: 'Database is running',
      help: 'Mandor does not start databases. Start it yourself, then confirm.'
    },
    app,
    {
      key: 'login',
      state: confirmed.login ? 'ok' : 'confirm',
      title: `Logged in at ${url}`,
      help: 'Log in with your own account in Chrome. Mandor cannot see the login and never receives your password.'
    },
    r.chrome_connected
      ? {
          key: 'extension',
          state: 'ok',
          title: 'Claude in Chrome extension connected',
          help: 'The reviewer uses its own tab, only at the app URL.'
        }
      : {
          key: 'extension',
          state: 'missing',
          title: 'Claude in Chrome extension not connected',
          help: 'Open Chrome and enable the extension, then check again.',
          action: 'recheck'
        },
    {
      key: 'browser',
      state: r.browser_tests ? 'ok' : 'missing',
      title: r.browser_tests ? 'Browser testing allowed' : 'Browser testing is off',
      help: 'Page content is read by the AI model. Change this in Settings.'
    }
  ];
}

/** Item yang belum beres (bukan ok). */
export function missingCount(items: ReadinessItem[]): number {
  return items.filter((i) => i.state !== 'ok').length;
}

/**
 * Alasan Can't verify yang muncul bila review dijalankan sekarang, dengan teks
 * dan urutan yang sama seperti daemon. Database tidak memengaruhi alasan.
 */
export function cantVerifyReason(items: ReadinessItem[]): string | null {
  const bad = (key: ReadinessItem['key']) => items.some((i) => i.key === key && i.state !== 'ok');
  const app = items.find((i) => i.key === 'app');
  if (bad('browser')) return 'Browser tests are off';
  if (app && app.state === 'missing') return 'The app is not running';
  if (app && app.state === 'warn') return 'app running older code';
  if (bad('login')) return 'Login not confirmed';
  if (bad('extension')) return 'Claude in Chrome extension not connected';
  return null;
}

export interface ConfirmRepo {
  repo_id: string;
  snapshot: string;
  files: string[];
  flagged: Set<string>;
}

/** Model dialog konfirmasi Run app: berkas berubah per repo, yang ditandai di atas. */
export function confirmModel(changes: AppChangesView[]): ConfirmRepo[] {
  return changes.map((c) => {
    const flagged = new Set(c.flagged);
    const files = [...c.files].sort((a, b) => Number(flagged.has(b)) - Number(flagged.has(a)) || a.localeCompare(b));
    return { repo_id: c.repo_id, snapshot: c.snapshot, files, flagged };
  });
}

/** Snapshot yang dikirim ke `app.run`. */
export function snapshots(changes: AppChangesView[]): Record<string, string> {
  return Object.fromEntries(changes.map((c) => [c.repo_id, c.snapshot]));
}
