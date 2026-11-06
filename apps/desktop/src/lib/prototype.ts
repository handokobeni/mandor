// Setelan app sumber desain yang bisa dijalankan (plan 2026-11-03-002, R6–R7).
import type { RepoAppView } from '#lib/protocol/RepoAppView.ts';
import type { RepoAppForm } from '#lib/settings.ts';

/** Form dari setelan tersimpan, atau kosong untuk sumber baru. */
export function prototypeForm(app: RepoAppView | null): RepoAppForm {
  return {
    testMode: 'web',
    setupCommand: app?.setup_command ?? '',
    startCommand: app?.start_command ?? '',
    port: app?.port == null ? '' : String(app.port),
    appUrl: app?.app_url ?? '',
    envKeys: [...(app?.env_keys ?? [])]
  };
}

/** Payload `app` untuk `design_sources.add/update`; daemon memvalidasi ulang. */
export function prototypePayload(f: RepoAppForm): RepoAppView {
  return {
    test_mode: 'web',
    setup_command: f.setupCommand.trim() || null,
    start_command: f.startCommand.trim() || null,
    port: f.port.trim() ? Number(f.port.trim()) : null,
    app_url: f.appUrl.trim() || null,
    env_keys: f.envKeys,
    browser_tests: true,
    auto_restart: false,
    unattended_login: false
  };
}
