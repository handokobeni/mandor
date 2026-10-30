// Validasi form pengaturan sebelum dikirim ke daemon. Daemon tetap memvalidasi ulang.

export interface LarkForm {
  appId: string;
  appSecret: string;
  appToken: string;
  tableId: string;
  acTableId: string;
  port: string;
}

export type LarkErrors = Partial<Record<keyof LarkForm, string>>;

function tableError(v: string, name: string): string | undefined {
  if (!v) return `${name} is required.`;
  if (!v.startsWith('tbl')) return 'Table IDs start with tbl.';
  return undefined;
}

/** Pesan error per field; objek kosong berarti valid. */
export function validateLark(form: LarkForm, secretSaved: boolean): LarkErrors {
  const f = Object.fromEntries(Object.entries(form).map(([k, v]) => [k, v.trim()])) as unknown as LarkForm;
  const port = Number(f.port);
  const errors: LarkErrors = {
    appId: !f.appId
      ? 'App ID is required.'
      : /^cli_[a-z0-9]{8,}$/.test(f.appId)
        ? undefined
        : 'App ID should look like cli_a6f2c81d93b8d00e.',
    appSecret: !secretSaved && !f.appSecret ? 'Paste the App Secret.' : undefined,
    appToken: !f.appToken
      ? 'Base app_token is required.'
      : f.appToken.length < 20
        ? 'This looks too short for an app_token.'
        : undefined,
    tableId: tableError(f.tableId, 'Sprint Tasks table ID'),
    acTableId: tableError(f.acTableId, 'Acceptance Criteria table ID'),
    port: /^\d{4,5}$/.test(f.port) && port >= 1024 && port <= 65535 ? undefined : 'Use a port between 1024 and 65535.'
  };
  return Object.fromEntries(Object.entries(errors).filter(([, v]) => v !== undefined)) as LarkErrors;
}

export type RepoList = 'companions' | 'reviewers' | 'allowed' | 'env';

/** Error untuk satu item baru di daftar pengaturan repo; `null` bila boleh ditambahkan. */
export function validateListItem(list: RepoList, raw: string, existing: string[]): string | null {
  const v = raw.trim();
  if (!v) return 'Type a value first.';
  if (list === 'companions' && !v.startsWith('/')) return 'Use an absolute path on the daemon machine.';
  if (list === 'reviewers' && !/^ou_[a-z0-9]{6,}$/.test(v)) return 'Lark open_ids start with ou_, e.g. ou_51aa6c.';
  if (list === 'allowed' && !/^[A-Z][A-Za-z]+(\(.+\))?$/.test(v)) return 'Use a tool pattern such as Bash(pnpm test *).';
  if (list === 'env' && !ENV_KEY.test(v)) return 'Use a variable name such as VITE_API_URL (no value).';
  if (existing.includes(v)) return 'Already in the list.';
  return null;
}

export type OnboardingStep = 'welcome' | 'prereq' | 'lark' | 'login' | 'repo' | 'done';

/**
 * Langkah awal onboarding, atau `null` bila tidak perlu: setelah Lark diatur,
 * login, dan ada repo, atau bila dev sudah melewatinya. Status belum dimuat = `null`.
 */
export function onboardingStart(
  status: { configured: boolean; logged_in: boolean } | null,
  repoCount: number,
  dismissed: boolean
): OnboardingStep | null {
  if (!status || dismissed) return null;
  if (!status.configured) return 'welcome';
  if (!status.logged_in) return 'login';
  if (repoCount === 0) return 'repo';
  return null;
}

/** Jenis notifikasi desktop yang bisa dimatikan di Settings › General. */
export const NOTIFY_KINDS = [
  { kind: 'awaiting_answer', label: 'Agent finished and is waiting for you' },
  { kind: 'failed', label: 'Run failed' },
  { kind: 'waiting_fix', label: 'Reviewer requested fixes (Waiting Fix)' },
  { kind: 'pr_created', label: 'PR created' },
  { kind: 'needs_attention', label: 'Needs attention: claim lost, task deleted or reassigned, push held' },
  { kind: 'review_finished', label: 'AC review finished' },
  { kind: 'app_failed', label: 'App failed to start' }
] as const;

export type NotifyPrefs = Record<string, boolean>;

export const DEFAULT_NOTIFY: NotifyPrefs = {
  awaiting_answer: true,
  failed: true,
  waiting_fix: true,
  pr_created: false,
  needs_attention: true,
  review_finished: true,
  app_failed: true
};

/** Notifikasi desktop untuk jenis ini; jenis yang belum dikenal tetap ditampilkan. */
export function shouldNotify(kind: string, prefs: NotifyPrefs): boolean {
  return prefs[kind] ?? true;
}

/** Form Lark berbeda dari nilai tersimpan (spasi diabaikan), atau secret sedang diganti. */
export function larkFormDirty(form: LarkForm, saved: LarkForm, replacingSecret: boolean): boolean {
  if (replacingSecret) return true;
  return (Object.keys(form) as (keyof LarkForm)[]).some((k) => form[k].trim() !== saved[k].trim());
}

/** Langkah breadcrumb untuk path absolut di mesin daemon. */
export function pathCrumbs(path: string): { name: string; path: string }[] {
  const parts = path.split('/').filter(Boolean);
  return [
    { name: '/', path: '/' },
    ...parts.map((name, i) => ({ name, path: `/${parts.slice(0, i + 1).join('/')}` }))
  ];
}

/** Batas jeda tenang Waiting Fix yang diterima daemon (detik). */
export const QUIET_MIN = 30;
export const QUIET_MAX = 900;

/** Pesan salah untuk input jeda tenang, atau `null` bila valid. */
export function validateQuiet(raw: string): string | null {
  const v = raw.trim();
  if (!v) return 'Type a number of seconds.';
  if (!/^\d+$/.test(v)) return 'Use whole seconds.';
  const n = Number(v);
  if (n < QUIET_MIN || n > QUIET_MAX) return `Use ${QUIET_MIN} to ${QUIET_MAX} seconds.`;
  return null;
}

/** Form Run app per repo (KTD-37). */
export interface RepoAppForm {
  testMode: 'web' | 'no_app';
  setupCommand: string;
  startCommand: string;
  port: string;
  appUrl: string;
  envKeys: string[];
}

export type RepoAppErrors = Partial<Record<keyof RepoAppForm, string>>;

const ENV_KEY = /^[A-Za-z_][A-Za-z0-9_]*$/;

/** Validasi form app repo; objek kosong berarti valid. Daemon tetap memvalidasi ulang. */
export function validateRepoApp(f: RepoAppForm): RepoAppErrors {
  const errors: RepoAppErrors = {};
  if (f.testMode === 'no_app') return errors;
  if (!f.startCommand.trim()) errors.startCommand = 'Web repos need a start command, e.g. bun run dev.';
  const port = Number(f.port.trim());
  const portOk = /^\d{4,5}$/.test(f.port.trim()) && port >= 1024 && port <= 65535;
  if (!portOk) errors.port = 'Use a port between 1024 and 65535.';
  const url = f.appUrl.trim();
  if (url) {
    const m = /^https?:\/\/(localhost|127\.0\.0\.1)(?::(\d+))?\/?$/.exec(url);
    if (!m) errors.appUrl = 'Use http://localhost:<port> or http://127.0.0.1:<port>.';
    else if (portOk && m[2] && Number(m[2]) !== port) errors.appUrl = 'The app URL must use the same port.';
  }
  const bad = f.envKeys.find((k) => !ENV_KEY.test(k));
  if (bad) errors.envKeys = `${bad} is not a valid variable name.`;
  return errors;
}

/** Daemon yang diatur di Settings (laptop, atau daemon VM lewat link VM). */
export type SettingsTarget = 'laptop' | 'vm';

const VM_SECTIONS = ['lark', 'repos', 'workspaces'] as const;
const LAPTOP_SECTIONS = ['lark', 'repos', 'workspaces', 'design-sources', 'general', 'background', 'vm', 'about'] as const;

/** Bagian Settings yang tersedia untuk target; daemon VM hanya butuh Lark App, repo, dan workspace. */
export function sectionsFor(target: SettingsTarget): string[] {
  return target === 'vm' ? [...VM_SECTIONS] : [...LAPTOP_SECTIONS];
}

/** Bagian yang ditampilkan: bila tidak tersedia untuk target, kembali ke Lark. */
export function sectionFor(target: SettingsTarget, section: string): string {
  return sectionsFor(target).includes(section) ? section : 'lark';
}

/** Orang yang dikenal daemon (`people.known`): `open_id` dan nama. */
export interface KnownPerson {
  open_id: string;
  name: string;
}

/** Calon reviewer: belum dipilih, cocok dengan nama atau `open_id` (tanpa beda huruf besar), urut nama. */
export function reviewerCandidates(known: KnownPerson[], selected: string[], query: string): KnownPerson[] {
  const q = query.trim().toLowerCase();
  return known
    .filter((p) => !selected.includes(p.open_id))
    .filter((p) => !q || p.name.toLowerCase().includes(q) || p.open_id.toLowerCase().includes(q))
    .sort((a, b) => a.name.localeCompare(b.name));
}

/**
 * Nama reviewer untuk ditampilkan; `open_id` bila namanya belum dikenal. Bila
 * dua orang bernama sama, empat karakter terakhir `open_id` ditambahkan agar
 * orang yang dipercaya tidak tertukar.
 */
export function reviewerLabel(known: KnownPerson[], openId: string): string {
  const name = known.find((p) => p.open_id === openId)?.name;
  if (name === undefined) return openId;
  const twins = known.filter((p) => p.name === name).length > 1;
  return twins ? `${name} (…${openId.slice(-4)})` : name;
}
