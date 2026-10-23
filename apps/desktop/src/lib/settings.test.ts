import { describe, expect, it } from 'vitest';
import { validateLark, validateListItem, validateRepoApp } from '#lib/settings.ts';

const ok = {
  appId: 'cli_a6f2c81d93b8d00e',
  appSecret: '',
  appToken: 'NkQ2bXr7YcPq1sZt9vLmHg4eAbc',
  tableId: 'tbl8kX2pQm4vR7aW',
  acTableId: 'tblD3nF6hJ9kL2pS',
  port: '53682'
};

describe('validasi pengaturan Lark', () => {
  it('nilai yang benar lolos; secret boleh kosong bila sudah tersimpan', () => {
    expect(validateLark(ok, true)).toEqual({});
  });

  it('setiap field memberi pesan yang menjelaskan cara memperbaikinya', () => {
    const errs = validateLark(
      { appId: 'abc', appSecret: '', appToken: 'short', tableId: 'x', acTableId: '', port: '80' },
      false
    );
    expect(errs).toEqual({
      appId: 'App ID should look like cli_a6f2c81d93b8d00e.',
      appSecret: 'Paste the App Secret.',
      appToken: 'This looks too short for an app_token.',
      tableId: 'Table IDs start with tbl.',
      acTableId: 'Acceptance Criteria table ID is required.',
      port: 'Use a port between 1024 and 65535.'
    });
  });
});

describe('validasi daftar repo', () => {
  it('companion path absolut, reviewer open_id, allowlist pola tool, tanpa duplikat', () => {
    expect(validateListItem('companions', 'relative/path', [])).toBe('Use an absolute path on the daemon machine.');
    expect(validateListItem('companions', '/home/a/shared', [])).toBeNull();
    expect(validateListItem('reviewers', 'budi', [])).toBe('Lark open_ids start with ou_, e.g. ou_51aa6c.');
    expect(validateListItem('reviewers', 'ou_51aa6c', ['ou_51aa6c'])).toBe('Already in the list.');
    expect(validateListItem('allowed', 'just test', [])).toBe('Use a tool pattern such as Bash(pnpm test *).');
    expect(validateListItem('allowed', 'Bash(just *)', [])).toBeNull();
    expect(validateListItem('allowed', '  ', [])).toBe('Type a value first.');
  });
});

describe('onboarding', () => {
  const st = (configured: boolean, logged_in: boolean) => ({ configured, logged_in, open_id: null, reauth_required: false });

  it('muncul sampai Lark diatur, login, dan ada repo; bisa dilewati', async () => {
    const { onboardingStart } = await import('#lib/settings.ts');
    expect(onboardingStart(st(false, false), 0, false)).toBe('welcome');
    expect(onboardingStart(st(true, false), 0, false)).toBe('login');
    expect(onboardingStart(st(true, true), 0, false)).toBe('repo');
    expect(onboardingStart(st(true, true), 1, false)).toBeNull();
    expect(onboardingStart(st(false, false), 0, true)).toBeNull();
    expect(onboardingStart(null, 0, false)).toBeNull();
  });
});

describe('preferensi notifikasi desktop', () => {
  it('bawaan semua aktif kecuali PR dibuat; jenis tak dikenal tetap tampil', async () => {
    const { DEFAULT_NOTIFY, shouldNotify } = await import('#lib/settings.ts');
    expect(shouldNotify('awaiting_answer', DEFAULT_NOTIFY)).toBe(true);
    expect(shouldNotify('pr_created', DEFAULT_NOTIFY)).toBe(false);
    expect(shouldNotify('failed', { ...DEFAULT_NOTIFY, failed: false })).toBe(false);
    expect(shouldNotify('jenis_baru', DEFAULT_NOTIFY)).toBe(true);
  });
});

describe('form Lark berubah', () => {
  it('berubah bila ada field yang beda dari tersimpan atau secret sedang diganti', async () => {
    const { larkFormDirty } = await import('#lib/settings.ts');
    expect(larkFormDirty(ok, ok, false)).toBe(false);
    expect(larkFormDirty({ ...ok, appToken: 'Xq7NbPJh5aD7SesHvtGlmgaZZz' }, ok, false)).toBe(true);
    expect(larkFormDirty({ ...ok, appSecret: 'baru' }, ok, false)).toBe(true);
    expect(larkFormDirty(ok, ok, true)).toBe(true);
    // Spasi di awal/akhir tidak dihitung sebagai perubahan.
    expect(larkFormDirty({ ...ok, tableId: ` ${ok.tableId} ` }, ok, false)).toBe(false);
  });
});

describe('breadcrumb penjelajah folder', () => {
  it('memecah path absolut menjadi langkah yang bisa diklik', async () => {
    const { pathCrumbs } = await import('#lib/settings.ts');
    expect(pathCrumbs('/home/beni/Work')).toEqual([
      { name: '/', path: '/' },
      { name: 'home', path: '/home' },
      { name: 'beni', path: '/home/beni' },
      { name: 'Work', path: '/home/beni/Work' }
    ]);
    expect(pathCrumbs('/')).toEqual([{ name: '/', path: '/' }]);
  });
});

describe('jeda tenang Waiting Fix', () => {
  it('menerima 30–900 detik bilangan bulat dan menjelaskan nilai yang salah', async () => {
    const { validateQuiet } = await import('#lib/settings.ts');
    expect(validateQuiet('120')).toBeNull();
    expect(validateQuiet(' 30 ')).toBeNull();
    expect(validateQuiet('900')).toBeNull();
    expect(validateQuiet('29')).toBe('Use 30 to 900 seconds.');
    expect(validateQuiet('901')).toBe('Use 30 to 900 seconds.');
    expect(validateQuiet('12.5')).toBe('Use whole seconds.');
    expect(validateQuiet('')).toBe('Type a number of seconds.');
    expect(validateQuiet('abc')).toBe('Use whole seconds.');
  });
});

describe('validasi app repo', () => {
  const base = {
    testMode: 'web' as const,
    setupCommand: '',
    startCommand: 'bun run dev',
    port: '5173',
    appUrl: '',
    envKeys: ['API_URL']
  };
  it('Web tanpa perintah start, port 80, host lain, dan kunci .env tidak valid ditolak', () => {
    expect(validateRepoApp(base)).toEqual({});
    expect(validateRepoApp({ ...base, startCommand: ' ' }).startCommand).toBeDefined();
    expect(validateRepoApp({ ...base, port: '80' }).port).toBeDefined();
    expect(validateRepoApp({ ...base, appUrl: 'http://example.com:5173' }).appUrl).toBeDefined();
    expect(validateRepoApp({ ...base, appUrl: 'http://localhost:3000' }).appUrl).toBe('The app URL must use the same port.');
    expect(validateRepoApp({ ...base, appUrl: 'http://127.0.0.1:5173' })).toEqual({});
    expect(validateRepoApp({ ...base, envKeys: ['MY-KEY'] }).envKeys).toContain('MY-KEY');
    expect(validateRepoApp({ ...base, testMode: 'no_app', startCommand: '' })).toEqual({});
  });
});
