import { describe, expect, it } from 'vitest';
import {
  appControlAction,
  appControlLabel,
  appUrls,
  cantVerifyReason,
  confirmModel,
  confirmedPrototypes,
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
    members: members.map(([repo_id, s]) => ({ repo_id, url: 'http://localhost:5173', state: s, prototype: null, browser_tests: false })),
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
      {
        repo_id: 'web',
        snapshot: 't1',
        files: ['src/a.ts', 'package.json'],
        flagged: ['package.json'],
        prototype: null,
        confirmed: false
      }
    ];
    const m = confirmModel(changes);
    expect(m[0].files).toEqual(['package.json', 'src/a.ts']);
    expect(m[0].flagged.has('package.json')).toBe(true);
    expect(snapshots(changes)).toEqual({ web: 't1' });
  });
});

describe('prototype di Run app (plan 2026-11-03-002)', () => {
  const proto = (confirmed: boolean) => ({
    repo_id: 'prototype:d1',
    snapshot: 'git:b',
    files: confirmed ? [] : ['src/app/page.tsx'],
    flagged: [],
    prototype: 'kelola-web-prototype',
    confirmed
  });
  const web = { repo_id: 'web', snapshot: 't1', files: [], flagged: [], prototype: null, confirmed: false };

  it('hides a prototype whose revision is already confirmed but still sends its snapshot', () => {
    const changes = [web, proto(true)];
    expect(confirmModel(changes).map((r) => r.repo_id)).toEqual(['web']);
    expect(snapshots(changes)).toEqual({ web: 't1', 'prototype:d1': 'git:b' });
  });

  it('names prototypes that start again at their confirmed revision', () => {
    expect(confirmedPrototypes([web, proto(true)])).toEqual(['kelola-web-prototype']);
    expect(confirmedPrototypes([web, proto(false)])).toEqual([]);
  });

  it('labels a changed prototype with its design source name', () => {
    const m = confirmModel([web, proto(false)]);
    expect(m[1].label).toBe('kelola-web-prototype (prototype)');
    expect(m[0].label).toBe('web');
  });

  it('names the prototype in the control label and finds its URL apart from the app URL', () => {
    const a = app('waiting_for_port', [
      ['web', 'running'],
      ['prototype:d1', 'waiting_for_port']
    ]);
    a.members[1].prototype = 'kelola-web-prototype';
    a.members[1].url = 'http://127.0.0.1:3001';
    expect(appControlLabel(a)).toBe('Waiting for port kelola-web-prototype (2 of 2)');
    a.state = 'running';
    a.members[1].state = 'running';
    expect(appUrls(a)).toEqual({ app: 'http://localhost:5173', prototype: 'http://127.0.0.1:3001' });
  });

  it('links the browser-tested repo, not an API repo listed first', () => {
    const a = app('running', [
      ['kelola-api', 'running'],
      ['kelola-web', 'running']
    ]);
    a.members[0].url = 'http://localhost:8090';
    a.members[1].url = 'http://stagingintegro.localhost:3000';
    expect(appUrls(a).app).toBe('http://localhost:8090');
    a.members[1].browser_tests = true;
    expect(appUrls(a).app).toBe('http://stagingintegro.localhost:3000');
  });
});
