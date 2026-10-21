// Workspace multi-repo: link PR per repo, form Settings › Workspaces, dan Changes per repo.

import type { FileChangeView } from '#lib/protocol/FileChangeView.ts';
import type { StartParams } from '#lib/protocol/StartParams.ts';
import type { StartPlanning } from '#lib/protocol/StartPlanning.ts';

export interface PrLink {
  url: string;
  label: string;
}

/** Link PR dari field `mandor_pr` (satu per baris). Banyak PR diberi nama repo. */
export function prLinks(prUrl: string | null | undefined): PrLink[] {
  const urls = (prUrl ?? '')
    .split('\n')
    .map((u) => u.trim())
    .filter(Boolean);
  return urls.map((url) => {
    const m = url.match(/\/([^/]+)\/pull\/(\d+)/);
    if (!m) return { url, label: 'PR' };
    return { url, label: urls.length > 1 ? `${m[1]}#${m[2]}` : `#${m[2]}` };
  });
}

function trimSlash(path: string): string {
  return path.trim().replace(/\/+$/, '');
}

/** Repo terdaftar yang berada di bawah folder induk. */
export function reposUnder(root: string, repos: { id: string; path: string }[]): string[] {
  const r = trimSlash(root);
  if (!r) return [];
  return repos.filter((repo) => repo.path.startsWith(`${r}/`)).map((repo) => repo.id);
}

/** Nama bawaan workspace: nama folder induk. */
export function workspaceName(root: string): string {
  const r = trimSlash(root);
  return r.slice(r.lastIndexOf('/') + 1);
}

export interface WorkspaceForm {
  name: string;
  root: string;
  repoIds: string[];
}

export type WorkspaceErrors = Partial<Record<'name' | 'root' | 'repos', string>>;

/** Pesan error per field; objek kosong berarti valid. Daemon tetap memvalidasi ulang. */
export function validateWorkspace(form: WorkspaceForm): WorkspaceErrors {
  const errors: WorkspaceErrors = {};
  if (!form.name.trim()) errors.name = 'Name the workspace.';
  if (!form.root.trim().startsWith('/')) errors.root = 'Use an absolute path on the daemon machine.';
  if (form.repoIds.length === 0) errors.repos = 'Choose at least one repo.';
  return errors;
}

export interface ChangeGroup {
  /** `null` untuk run satu repo. */
  repo: string | null;
  files: FileChangeView[];
}

/** Berkas berubah per repo (run workspace memakai awalan `<repo>/`), urut seperti `repoIds`. */
export function groupChanges(files: FileChangeView[], repoIds: string[] | null): ChangeGroup[] {
  if (!repoIds) return [{ repo: null, files }];
  return repoIds
    .map((repo) => ({ repo, files: files.filter((f) => f.path.startsWith(`${repo}/`)) }))
    .filter((g) => g.files.length > 0);
}

export type StartTarget =
  | { kind: 'repo'; repoId: string }
  | { kind: 'workspace'; workspace: { id: string; repo_ids: string[] }; repoIds: string[] };

/** Parameter `runs.start`. Repo workspace dikirim dalam urutan workspace (repo utama pertama). */
export function startParams(input: {
  recordId: string;
  note: string;
  target: StartTarget;
  planning: StartPlanning;
}): StartParams {
  const note = input.note.trim() || null;
  const planning = input.planning;
  const t = input.target;
  if (t.kind === 'repo') return { record_id: input.recordId, repo_id: t.repoId, note, planning };
  const repoIds = t.workspace.repo_ids.filter((id) => t.repoIds.includes(id));
  return {
    record_id: input.recordId,
    repo_id: repoIds[0] ?? '',
    note,
    workspace: { id: t.workspace.id, repo_ids: repoIds },
    planning
  };
}
