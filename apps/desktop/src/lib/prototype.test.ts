import { describe, expect, it } from 'vitest';
import { prototypeForm, prototypePayload } from './prototype.ts';

describe('runnable prototype settings', () => {
  it('builds the app payload with web mode and trimmed fields', () => {
    expect(
      prototypePayload({
        testMode: 'web',
        setupCommand: ' pnpm install ',
        startCommand: ' pnpm dev ',
        port: ' 3001 ',
        appUrl: '',
        envKeys: ['KELOLA_API_MODE']
      })
    ).toEqual({
      test_mode: 'web',
      setup_command: 'pnpm install',
      start_command: 'pnpm dev',
      port: 3001,
      app_url: null,
      env_keys: ['KELOLA_API_MODE'],
      browser_tests: true,
      auto_restart: false,
      unattended_login: false
    });
  });

  it('fills the form from a saved source and defaults for a new one', () => {
    expect(prototypeForm(null)).toEqual({
      testMode: 'web',
      setupCommand: '',
      startCommand: '',
      port: '',
      appUrl: '',
      envKeys: []
    });
    const form = prototypeForm({
      test_mode: 'web',
      setup_command: null,
      start_command: 'pnpm dev',
      port: 3001,
      app_url: 'http://127.0.0.1:3001',
      env_keys: [],
      browser_tests: true,
      auto_restart: false,
      unattended_login: false
    });
    expect(form.port).toBe('3001');
    expect(form.appUrl).toBe('http://127.0.0.1:3001');
  });
});
