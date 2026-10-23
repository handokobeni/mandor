import { describe, expect, it } from 'vitest';
import {
  appShouldHoldKeepAwake,
  backgroundView,
  shouldRetryProvision,
  type BackgroundInput,
  type ProvisionState
} from './background';
import type { StatusResult } from './protocol/StatusResult';

const status = (over: Partial<StatusResult> = {}): StatusResult => ({
  version: '0.1.0',
  uptime_ms: 1,
  repos: 1,
  lark: { configured: true, logged_in: true, open_id: 'ou', name: 'B', reauth_required: false },
  busy: false,
  managed_by: 'systemd',
  draining: false,
  keep_awake: 'off',
  ...over
});

const report = (over: Record<string, unknown> = {}) => ({
  result: 'updated',
  service: 'systemd',
  linger: true,
  apps_stopped: false,
  warnings: [],
  message: null,
  ...over
});

const input = (over: Partial<BackgroundInput> = {}): BackgroundInput => ({
  wsl: true,
  provision: { state: 'up_to_date', systemd: true, linger: true },
  status: status(),
  keepAlive: { status: 'enabled', windows_build: 26200 },
  ...over
});

const row = (v: ReturnType<typeof backgroundView>, id: string) => v.rows.find((r) => r.id === id);

describe('backgroundView', () => {
  it('siap lembur bila service, linger, keep-alive, dan keep-awake semuanya baik', () => {
    const v = backgroundView(input());
    expect(v.ready).toBe(true);
    expect(v.rows.map((r) => [r.id, r.tone])).toEqual([
      ['service', 'ok'],
      ['linger', 'ok'],
      ['keep_alive', 'ok'],
      ['keep_awake', 'ok']
    ]);
    expect(v.notices).toEqual([]);
  });

  it('daemon tanpa systemd memberi panduan mengaktifkan systemd', () => {
    const v = backgroundView(
      input({ provision: { state: 'up_to_date', systemd: false, linger: false }, status: status({ managed_by: 'detached' }) })
    );
    const s = row(v, 'service');
    expect(s?.tone).toBe('warn');
    expect(s?.detail).toContain('systemd=true');
    expect(s?.detail).toContain('wsl --shutdown');
    expect(row(v, 'linger')).toBeUndefined();
    expect(v.ready).toBe(false);
  });

  it('linger mati tidak siap dengan langkah perbaikan', () => {
    const v = backgroundView(input({ provision: { state: 'up_to_date', systemd: true, linger: false } }));
    expect(row(v, 'linger')?.tone).toBe('warn');
    expect(row(v, 'linger')?.detail).toContain('loginctl enable-linger');
    expect(v.ready).toBe(false);
  });

  it('keep-alive belum aktif menawarkan persetujuan', () => {
    const v = backgroundView(input({ keepAlive: { status: 'disabled', windows_build: 26200 } }));
    expect(row(v, 'keep_alive')?.tone).toBe('warn');
    expect(row(v, 'keep_alive')?.action).toBe('enable_keep_alive');
    expect(v.ready).toBe(false);
  });

  it('Windows 10 tidak mendukung keep-alive: Quit tidak aman', () => {
    const v = backgroundView(input({ keepAlive: { status: 'unsupported', windows_build: 19045 } }));
    expect(row(v, 'keep_alive')?.title).toContain('Windows 10');
    expect(row(v, 'keep_alive')?.detail).toContain('keep Mandor in the tray');
    expect(row(v, 'keep_alive')?.action).toBeUndefined();
  });

  it('keep-awake gagal menampilkan pesannya', () => {
    const v = backgroundView(input({ status: status({ keep_awake: 'failing', keep_awake_error: 'the keep-awake helper exited' }) }));
    expect(row(v, 'keep_awake')?.tone).toBe('error');
    expect(row(v, 'keep_awake')?.detail).toContain('helper exited');
  });

  it('keep-awake memegang saat agent bekerja', () => {
    const v = backgroundView(input({ status: status({ busy: true, keep_awake: 'held' }) }));
    expect(row(v, 'keep_awake')?.title).toContain('awake');
    expect(row(v, 'keep_awake')?.tone).toBe('ok');
  });

  it('daemon lama tanpa field baru tampil unknown, bukan error', () => {
    const old = status();
    delete old.busy;
    delete old.managed_by;
    delete old.keep_awake;
    const v = backgroundView(input({ status: old, provision: null }));
    expect(row(v, 'service')?.tone).toBe('unknown');
    expect(row(v, 'keep_awake')?.tone).toBe('unknown');
    expect(row(v, 'linger')?.tone).toBe('unknown');
  });

  it('mode lokal tidak menampilkan keep-alive WSL', () => {
    const v = backgroundView(input({ wsl: false, keepAlive: null }));
    expect(row(v, 'keep_alive')).toBeUndefined();
  });

  it('pembaruan tertunda dan peringatan Quit bila daemon lama belum mendukung keep-awake', () => {
    const old = status({ busy: true });
    delete old.keep_awake;
    const deferred: ProvisionState = { state: 'deferred', report: report({ result: 'deferred' }) };
    const v = backgroundView(input({ provision: deferred, status: old }));
    expect(v.notices.map((n) => n.id)).toEqual(['update_waiting', 'quit_unsafe']);
    expect(v.notices[0]?.text).toBe('Update waiting for running agents');
  });

  it('app dev berhenti karena pembaruan diberitahukan', () => {
    const v = backgroundView(input({ provision: { state: 'installed', report: report({ apps_stopped: true }) } }));
    expect(v.notices.map((n) => n.id)).toContain('apps_stopped');
  });

  it('kegagalan provisioning ditampilkan', () => {
    const v = backgroundView(input({ provision: { state: 'failed', message: 'the bundled daemon does not match this app' } }));
    expect(v.notices[0]).toMatchObject({ id: 'provision_failed', tone: 'error' });
    expect(v.notices[0]?.text).toContain('does not match');
  });
});

describe('keputusan', () => {
  it('pembaruan dicoba lagi hanya saat tertunda dan daemon tidak sibuk', () => {
    const deferred: ProvisionState = { state: 'deferred', report: report({ result: 'deferred' }) };
    expect(shouldRetryProvision(deferred, status({ busy: false }))).toBe(true);
    expect(shouldRetryProvision(deferred, status({ busy: true }))).toBe(false);
    expect(shouldRetryProvision({ state: 'up_to_date', systemd: true, linger: true }, status())).toBe(false);
    expect(shouldRetryProvision(deferred, null)).toBe(false);
  });

  it('app memegang keep-awake hanya bila daemon belum mendukung dan ada run bekerja', () => {
    const old = status({ busy: true });
    delete old.keep_awake;
    expect(appShouldHoldKeepAwake(old)).toBe(true);
    expect(appShouldHoldKeepAwake(status({ busy: true, keep_awake: 'held' }))).toBe(false);
    expect(appShouldHoldKeepAwake({ ...old, busy: false })).toBe(false);
    expect(appShouldHoldKeepAwake(null)).toBe(false);
  });
});
