import { describe, expect, it } from 'vitest';
import {
  appControlAction,
  appControlLabel,
  cantVerifyReason,
  confirmModel,
  missingCount,
  readinessItems,
  snapshots
} from '#lib/app-run.ts';
import type { AppRunView } from '#lib/protocol/AppRunView.ts';
import type { ReviewReadinessView } from '#lib/protocol/ReviewReadinessView.ts';

function app(state: string, members: [string, string][]): AppRunView {
  return {
    run_id: 'r1',
    state,
    members: members.map(([repo_id, s]) => ({ repo_id, url: 'http://localhost:5173', state: s })),
    database_confirmed: false,
    login_confirmed: false,
    error: null
  };
}

function ready(extra: Partial<ReviewReadinessView> = {}): ReviewReadinessView {
  return {
    needs_dialog: true,
    browser_tests: true,
    app: 'current',
    other_owner: null,
    confirmed: { database: false, login: false },
    app_urls: ['http://localhost:5173'],
    chrome_connected: true,
    cant_verify_reason: null,
    ...extra
  };
}

describe('kontrol Run app', () => {
  it('label menyebut repo yang sedang dinyalakan dan urutannya', () => {
    expect(appControlLabel(null)).toBe('App off');
    expect(appControlLabel(app('starting', [['kelola-api', 'running'], ['kelola-web', 'waiting_for_port']]))).toBe(
      'Waiting for port kelola-web (2 of 2)'
    );
    expect(appControlLabel(app('failed', [['kelola-web', 'failed']]))).toBe('Failed to start · kelola-web');
    expect(appControlAction(app('running', [['w', 'running']]))).toBe('stop');
    expect(appControlAction(app('failed', [['w', 'failed']]))).toBe('logs');
    expect(appControlAction(app('stopped_by_other_run', []))).toBe('run');
  });
});

describe('checklist kesiapan', () => {
  it('menghitung item yang kurang dan alasan Can\'t verify yang akan muncul', () => {
    const items = readinessItems(ready({ chrome_connected: false }), { database: true, login: true }, null);
    expect(missingCount(items)).toBe(1);
    expect(cantVerifyReason(items)).toBe('Claude in Chrome extension not connected');
    const login = readinessItems(ready(), { database: true, login: false }, null);
    expect(cantVerifyReason(login)).toBe('Login not confirmed');
    const all = readinessItems(ready(), { database: true, login: true }, null);
    expect(missingCount(all)).toBe(0);
    expect(cantVerifyReason(all)).toBeNull();
  });

  it('app kode lama menawarkan Restart app; app milik run lain disebut', () => {
    const older = readinessItems(ready({ app: 'older_code' }), { database: true, login: true }, null);
    const item = older.find((i) => i.key === 'app');
    expect(item?.title).toBe('App is running older code');
    expect(item?.action).toBe('restart_app');
    const other = readinessItems(ready({ app: 'not_running' }), { database: true, login: true }, 'PGS-1430');
    expect(other.find((i) => i.key === 'app')?.help).toContain('PGS-1430');
  });
});

describe('dialog konfirmasi Run app', () => {
  it('berkas berubah per repo dengan package.json di atas dan snapshot untuk app.run', () => {
    const changes = [
      { repo_id: 'web', snapshot: 't1', files: ['src/a.ts', 'package.json'], flagged: ['package.json'] }
    ];
    const m = confirmModel(changes);
    expect(m[0].files).toEqual(['package.json', 'src/a.ts']);
    expect(m[0].flagged.has('package.json')).toBe(true);
    expect(snapshots(changes)).toEqual({ web: 't1' });
  });
});
