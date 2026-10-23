// Kesiapan kerja latar (U8 plan service OS): service daemon, linger, keep-alive
// WSL, dan keep-awake, beserta pemberitahuan pembaruan daemon.

import type { StatusResult } from '#lib/protocol/StatusResult.ts';

/** Hasil `mandord install` (dari provisioning app). */
export interface InstallReport {
  result: string;
  service: string;
  linger: boolean;
  apps_stopped: boolean;
  warnings: string[];
  message: string | null;
}

/** Keadaan provisioning daemon oleh app (command Tauri `provision_run`). */
export type ProvisionState =
  | { state: 'not_needed' }
  | { state: 'not_bundled' }
  | { state: 'up_to_date'; systemd: boolean; linger: boolean }
  | { state: 'installed'; report: InstallReport }
  | { state: 'deferred'; report: InstallReport }
  | { state: 'cannot_check'; message: string }
  | { state: 'failed'; message: string };

/** Status `.wslconfig` (command Tauri `wsl_keepalive_status`). */
export interface KeepAliveView {
  status: 'enabled' | 'disabled' | 'unsupported';
  windows_build: number | null;
}

export type Tone = 'ok' | 'warn' | 'error' | 'unknown';

export interface Row {
  id: 'service' | 'linger' | 'keep_alive' | 'keep_awake';
  tone: Tone;
  title: string;
  detail: string;
  action?: 'enable_keep_alive' | 'disable_keep_alive';
}

export interface Notice {
  id: 'update_waiting' | 'quit_unsafe' | 'apps_stopped' | 'provision_failed' | 'cannot_check';
  tone: Tone;
  text: string;
}

export interface BackgroundInput {
  /** App memakai daemon di WSL. */
  wsl: boolean;
  provision: ProvisionState | null;
  status: StatusResult | null;
  keepAlive: KeepAliveView | null;
}

export interface BackgroundView {
  rows: Row[];
  notices: Notice[];
  /** Agent aman ditinggal lembur setelah app di-Quit. */
  ready: boolean;
}

const SYSTEMD_STEPS =
  'Turn on systemd in WSL: add "[boot]" and "systemd=true" to /etc/wsl.conf, then run "wsl --shutdown" from Windows and reopen Mandor.';

/** Status service, linger, dan sistem dari provisioning. */
function provisioned(p: ProvisionState | null): { systemd: boolean | null; linger: boolean | null } {
  if (!p) return { systemd: null, linger: null };
  switch (p.state) {
    case 'up_to_date':
      return { systemd: p.systemd, linger: p.linger };
    case 'installed':
    case 'deferred':
      return { systemd: p.report.service === 'systemd', linger: p.report.linger };
    default:
      return { systemd: null, linger: null };
  }
}

function serviceRow(status: StatusResult | null, systemd: boolean | null): Row {
  const managed = status?.managed_by;
  if (managed === 'systemd') {
    return {
      id: 'service',
      tone: 'ok',
      title: 'Daemon runs as a service',
      detail: 'It keeps running when you close or quit Mandor, and restarts if it crashes.'
    };
  }
  if (managed === 'detached') {
    return {
      id: 'service',
      tone: 'warn',
      title: 'Daemon is not a service',
      detail:
        systemd === false
          ? `It stops when WSL stops. ${SYSTEMD_STEPS}`
          : 'It stops when WSL stops. Reopen Mandor to set up the service.'
    };
  }
  return { id: 'service', tone: 'unknown', title: 'Daemon service: unknown', detail: 'Update Mandor to see this status.' };
}

function lingerRow(linger: boolean | null): Row {
  if (linger === true) {
    return { id: 'linger', tone: 'ok', title: 'Linger is on', detail: 'The service survives the end of your WSL session.' };
  }
  if (linger === false) {
    return {
      id: 'linger',
      tone: 'warn',
      title: 'Linger is off',
      detail: 'The service stops when your WSL session ends. In WSL, run: sudo loginctl enable-linger "$USER"'
    };
  }
  return { id: 'linger', tone: 'unknown', title: 'Linger: unknown', detail: 'Shown after Mandor checks the daemon.' };
}

function keepAliveRow(k: KeepAliveView | null): Row {
  if (!k) {
    return { id: 'keep_alive', tone: 'unknown', title: 'WSL keep-alive: unknown', detail: 'Could not read .wslconfig.' };
  }
  switch (k.status) {
    case 'enabled':
      return {
        id: 'keep_alive',
        tone: 'ok',
        title: 'WSL stays on without Mandor',
        detail: 'instanceIdleTimeout and vmIdleTimeout are -1 in .wslconfig.',
        action: 'disable_keep_alive'
      };
    case 'unsupported':
      return {
        id: 'keep_alive',
        tone: 'warn',
        title: 'WSL keep-alive needs Windows 11 (this is Windows 10)',
        detail: 'Quit is not safe for overnight work on Windows 10; keep Mandor in the tray.'
      };
    default:
      return {
        id: 'keep_alive',
        tone: 'warn',
        title: 'WSL stops after Mandor quits',
        detail:
          'Keeping WSL on changes .wslconfig for every distro and keeps its memory in use. Mandor backs up the file first.',
        action: 'enable_keep_alive'
      };
  }
}

function keepAwakeRow(status: StatusResult | null): Row {
  switch (status?.keep_awake) {
    case 'held':
      return { id: 'keep_awake', tone: 'ok', title: 'Keeping the laptop awake', detail: 'Agents are working; sleep is held off.' };
    case 'off':
      return {
        id: 'keep_awake',
        tone: 'ok',
        title: 'Keep-awake ready',
        detail: 'Sleep is held off only while agents work, including after you quit Mandor.'
      };
    case 'failing':
      return {
        id: 'keep_awake',
        tone: 'error',
        title: 'Keep-awake is failing',
        detail: status.keep_awake_error ?? 'The keep-awake helper could not run.'
      };
    case 'not_configured':
      return {
        id: 'keep_awake',
        tone: 'warn',
        title: 'Keep-awake is not set up',
        detail: 'Reopen Mandor to set it up; until then the laptop may sleep while agents work.'
      };
    default:
      return { id: 'keep_awake', tone: 'unknown', title: 'Keep-awake: unknown', detail: 'Update the daemon to see this status.' };
  }
}

function notices(p: ProvisionState | null, status: StatusResult | null): Notice[] {
  const out: Notice[] = [];
  if (p?.state === 'deferred') {
    out.push({ id: 'update_waiting', tone: 'warn', text: 'Update waiting for running agents' });
    if (status?.keep_awake === undefined && status?.busy) {
      out.push({
        id: 'quit_unsafe',
        tone: 'warn',
        text: 'Keep Mandor open until the update installs: this daemon cannot keep the laptop awake after you quit.'
      });
    }
  }
  if (p?.state === 'installed' && p.report.apps_stopped) {
    out.push({ id: 'apps_stopped', tone: 'warn', text: 'Running dev apps stopped while the daemon updated.' });
  }
  if (p?.state === 'failed') {
    out.push({ id: 'provision_failed', tone: 'error', text: `Daemon setup failed: ${p.message}` });
  }
  if (p?.state === 'cannot_check') {
    out.push({ id: 'cannot_check', tone: 'warn', text: `Could not check the daemon: ${p.message}` });
  }
  return out;
}

/** Baris status dan pemberitahuan untuk Settings → Background work. */
export function backgroundView(input: BackgroundInput): BackgroundView {
  const { systemd, linger } = provisioned(input.provision);
  const rows: Row[] = [serviceRow(input.status, systemd)];
  if (systemd !== false) rows.push(lingerRow(linger));
  if (input.wsl) rows.push(keepAliveRow(input.keepAlive));
  rows.push(keepAwakeRow(input.status));
  return { rows, notices: notices(input.provision, input.status), ready: rows.every((r) => r.tone === 'ok') };
}

/** Pembaruan yang tertunda dicoba lagi saat daemon tidak lagi sibuk. */
export function shouldRetryProvision(p: ProvisionState | null, status: StatusResult | null): boolean {
  return p?.state === 'deferred' && status?.busy === false;
}

/** App memegang keep-awake sendiri bila daemon belum mendukungnya dan ada run bekerja. */
export function appShouldHoldKeepAwake(status: StatusResult | null): boolean {
  return !!status && status.keep_awake === undefined && status.busy === true;
}
