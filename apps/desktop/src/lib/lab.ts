// Logika review lab (plan 2026-11-10-001): badge bukti di baris AC, label
// kebutuhan lab dan rekomendasi, form perlengkapan per repo, dan status
// penyiapannya. Murni agar teruji tanpa render komponen.

import type { AcRowView } from '#lib/protocol/AcRowView.ts';
import type { LabApproveParams } from '#lib/protocol/LabApproveParams.ts';
import type { LabDataDto } from '#lib/protocol/LabDataDto.ts';
import type { LabEquipmentDto } from '#lib/protocol/LabEquipmentDto.ts';
import type { LabSetParams } from '#lib/protocol/LabSetParams.ts';
import type { LabView } from '#lib/protocol/LabView.ts';
import type { Tone } from '#lib/board.ts';

/** Badge bukti verdict: "Lab" untuk verdict dari reviewer lab, lalu tanda
 * keadaan awal yang dibuat lewat simulasi request atau SQL (R17). */
export function evidenceBadges(r: AcRowView): string[] {
  const d = r.evidence_detail;
  if (!d) return [];
  const out: string[] = [];
  if (d.lab) out.push('Lab');
  if (d.simulated === 'request') out.push('Simulated request');
  if (d.simulated === 'sql') out.push('Prepared with SQL');
  return out;
}

const LAB_NEED: Record<string, string> = {
  data: 'Needs prepared data',
  role: 'Needs another test account',
  simulation: 'Needs a simulated failure',
  file: 'Needs a test file'
};

/** Apa yang dibutuhkan lab untuk baris Can't verify; `null` bila tidak ada. */
export function labNeedLabel(need: string | null): string | null {
  if (need === null) return null;
  return LAB_NEED[need] ?? need;
}

/** Rekomendasi reviewer untuk baris *looks different* (R18). */
export function adviceLabel(advice: string): string {
  return advice === 'accept'
    ? 'Difference accepted: the app follows the AC'
    : 'Change the app to follow the prototype';
}

export interface LabAccountForm {
  role: string;
  username: string;
  default: boolean;
  /** Password tersimpan di daemon; nilainya tidak pernah dikirim ke app. */
  hasPassword: boolean;
  /** Password baru; kosong mempertahankan yang tersimpan. */
  password: string;
}

export interface LabForm {
  accounts: LabAccountForm[];
  loginUrl: string;
  usernameField: string;
  passwordField: string;
  submit: string;
  submitPath: string;
  successKind: 'path' | 'text';
  success: string;
  authenticatedUrl: string;
  safePosts: string[];
  fixturesDir: string;
  /** Bagian lab data (fase 2), diteruskan apa adanya sampai diubah di form. */
  data: LabDataDto | null;
}

export function emptyLabForm(): LabForm {
  return {
    accounts: [],
    loginUrl: '',
    usernameField: '',
    passwordField: '',
    submit: '',
    submitPath: '',
    successKind: 'path',
    success: '',
    authenticatedUrl: '',
    safePosts: [],
    fixturesDir: '',
    data: null
  };
}

/** Form dari perlengkapan tersimpan atau usulan; password selalu kosong. */
export function labForm(eq: LabEquipmentDto | null): LabForm {
  if (!eq) return emptyLabForm();
  return {
    accounts: eq.accounts.map((a) => ({
      role: a.role,
      username: a.username,
      default: a.default,
      hasPassword: a.has_password,
      password: ''
    })),
    loginUrl: eq.login.url,
    usernameField: eq.login.username_field,
    passwordField: eq.login.password_field,
    submit: eq.login.submit,
    submitPath: eq.login.submit_path,
    successKind: eq.login.success_kind === 'text' ? 'text' : 'path',
    success: eq.login.success,
    authenticatedUrl: eq.login.authenticated_url,
    safePosts: [...eq.safe_posts],
    fixturesDir: eq.fixtures_dir ?? '',
    data: eq.data
  };
}

function equipment(f: LabForm): LabEquipmentDto {
  const fixtures = f.fixturesDir.trim();
  return {
    accounts: f.accounts.map((a) => ({
      role: a.role.trim(),
      username: a.username.trim(),
      default: a.default,
      has_password: a.hasPassword
    })),
    login: {
      url: f.loginUrl.trim(),
      username_field: f.usernameField.trim(),
      password_field: f.passwordField.trim(),
      submit: f.submit.trim(),
      submit_path: f.submitPath.trim(),
      success_kind: f.successKind,
      success: f.success.trim(),
      authenticated_url: f.authenticatedUrl.trim()
    },
    safe_posts: f.safePosts.map((p) => p.trim()).filter((p) => p.length > 0),
    fixtures_dir: fixtures.length > 0 ? fixtures : null,
    data: f.data
  };
}

/** `lab.set`: hanya password yang diisi ikut terkirim. */
export function labSetParams(repoId: string, f: LabForm): LabSetParams {
  return {
    repo_id: repoId,
    equipment: equipment(f),
    passwords: f.accounts
      .filter((a) => a.password.length > 0)
      .map((a) => ({ role: a.role.trim(), password: a.password })),
    db: []
  };
}

/** `lab.approve`: perlengkapan persis seperti yang dilihat developer. */
export function labApproveParams(repoId: string, f: LabForm): LabApproveParams {
  return { repo_id: repoId, equipment: equipment(f) };
}

const ROLE = /^[a-z0-9][a-z0-9-]{0,15}$/;

/** Pemeriksaan dasar sebelum dikirim; daemon tetap memvalidasi penuh. */
export function validateLabForm(f: LabForm): Record<string, string> {
  const errs: Record<string, string> = {};
  const roles = f.accounts.map((a) => a.role.trim());
  if (f.accounts.length === 0) errs.accounts = 'Add at least one test account.';
  else if (f.accounts.length > 4) errs.accounts = 'Use at most 4 test accounts.';
  else if (roles.some((r) => !ROLE.test(r)))
    errs.accounts = 'Roles are short lowercase names such as admin or manager.';
  else if (new Set(roles).size !== roles.length) {
    const dup = roles.find((r, i) => roles.indexOf(r) !== i) ?? '';
    errs.accounts = `Role ${dup} is listed twice.`;
  } else if (f.accounts.filter((a) => a.default).length !== 1)
    errs.accounts = 'Mark exactly one default account for ordinary review.';
  else if (f.accounts.some((a) => a.username.trim().length === 0)) errs.accounts = 'Enter a user name for every account.';
  if (!/^https?:\/\//.test(f.loginUrl.trim())) errs.loginUrl = 'Enter the login page URL.';
  if (!/^https?:\/\//.test(f.authenticatedUrl.trim())) errs.authenticatedUrl = 'Enter a page that needs a login.';
  for (const [key, value] of [
    ['usernameField', f.usernameField],
    ['passwordField', f.passwordField],
    ['submit', f.submit],
    ['success', f.success]
  ] as const) {
    if (value.trim().length === 0) errs[key] = 'Required.';
  }
  if (!f.submitPath.trim().startsWith('/')) errs.submitPath = 'A path starting with /.';
  return errs;
}

export interface LabStatus {
  text: string;
  tone: Tone;
  /** Tawarkan "Propose from repo". */
  setup: boolean;
}

/** Status perlengkapan repo untuk bagian "Review lab" di pengaturan. */
export function labStatus(v: LabView | null): LabStatus {
  if (v?.proposing) return { text: 'Proposing lab equipment from the repo…', tone: 'working', setup: false };
  if (v?.equipment && v.approved) return { text: 'Approved', tone: 'ok', setup: false };
  if (v?.proposal) return { text: 'A proposal is ready for you to review', tone: 'waiting', setup: false };
  if (v?.equipment) return { text: 'Saved; waiting for your approval', tone: 'waiting', setup: false };
  if (v?.proposal_error) return { text: `The proposal failed: ${v.proposal_error}`, tone: 'failed', setup: true };
  return { text: 'No test login is prepared for unattended review', tone: 'muted', setup: true };
}
