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

export type RepoList = 'companions' | 'reviewers' | 'allowed';

/** Error untuk satu item baru di daftar pengaturan repo; `null` bila boleh ditambahkan. */
export function validateListItem(list: RepoList, raw: string, existing: string[]): string | null {
  const v = raw.trim();
  if (!v) return 'Type a value first.';
  if (list === 'companions' && !v.startsWith('/')) return 'Use an absolute path on the daemon machine.';
  if (list === 'reviewers' && !/^ou_[a-z0-9]{6,}$/.test(v)) return 'Lark open_ids start with ou_, e.g. ou_51aa6c.';
  if (list === 'allowed' && !/^[A-Z][A-Za-z]+(\(.+\))?$/.test(v)) return 'Use a tool pattern such as Bash(pnpm test *).';
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
  { kind: 'needs_attention', label: 'Needs attention: claim lost, task deleted or reassigned' }
] as const;

export type NotifyPrefs = Record<string, boolean>;

export const DEFAULT_NOTIFY: NotifyPrefs = {
  awaiting_answer: true,
  failed: true,
  waiting_fix: true,
  pr_created: false,
  needs_attention: true
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
