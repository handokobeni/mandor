import { describe, expect, it } from 'vitest';
import {
  groupChanges,
  prLinks,
  reposUnder,
  startParams,
  validateWorkspace,
  workspaceName
} from '#lib/workspaces.ts';

describe('link PR', () => {
  it('satu PR memakai nomor saja', () => {
    expect(prLinks('https://github.com/o/web/pull/12')).toEqual([
      { url: 'https://github.com/o/web/pull/12', label: '#12' }
    ]);
  });

  it('banyak PR (satu per baris) diberi nama repo', () => {
    expect(prLinks('https://github.com/o/kelola-api/pull/1\nhttps://github.com/o/kelola-web/pull/2\n')).toEqual([
      { url: 'https://github.com/o/kelola-api/pull/1', label: 'kelola-api#1' },
      { url: 'https://github.com/o/kelola-web/pull/2', label: 'kelola-web#2' }
    ]);
  });

  it('kosong atau bukan URL PR', () => {
    expect(prLinks(null)).toEqual([]);
    expect(prLinks('https://x/y')).toEqual([{ url: 'https://x/y', label: 'PR' }]);
  });
});

describe('workspace', () => {
  const repos = [
    { id: 'kelola-api', path: '/w/kelola-v2/kelola-api' },
    { id: 'kelola-web', path: '/w/kelola-v2/kelola-web' },
    { id: 'lain', path: '/w/kelola-v2-old/lain' }
  ];

  it('repo di bawah folder induk dicentang lebih dulu', () => {
    expect(reposUnder('/w/kelola-v2/', repos)).toEqual(['kelola-api', 'kelola-web']);
  });

  it('nama bawaan dari folder induk', () => {
    expect(workspaceName('/w/kelola-v2/')).toBe('kelola-v2');
  });

  it('validasi form', () => {
    expect(validateWorkspace({ name: '', root: 'rel', repoIds: [] })).toEqual({
      name: 'Name the workspace.',
      root: 'Use an absolute path on the daemon machine.',
      repos: 'Choose at least one repo.'
    });
    expect(validateWorkspace({ name: 'k', root: '/w/k', repoIds: ['a'] })).toEqual({});
  });

  it('berkas berubah dikelompokkan per repo dari awalan path', () => {
    const files = [
      { path: 'kelola-web/src/a.ts', old_path: null, status: 'modified', additions: 1, deletions: 0 },
      { path: 'kelola-api/src/b.rs', old_path: null, status: 'added', additions: 2, deletions: 0 }
    ];
    expect(groupChanges(files, ['kelola-api', 'kelola-web']).map((g) => [g.repo, g.files.length])).toEqual([
      ['kelola-api', 1],
      ['kelola-web', 1]
    ]);
    expect(groupChanges(files, null)).toEqual([{ repo: null, files }]);
  });
});

const SKIP = { kind: 'skip', brainstorm_skill: null, plan_skill: null };
const PLAN = { kind: 'plan', brainstorm_skill: null, plan_skill: 'ce:plan' };

describe('parameter Start', () => {
  it('satu repo', () => {
    expect(
      startParams({ recordId: 'rec1', note: '  ', target: { kind: 'repo', repoId: 'web' }, planning: SKIP })
    ).toEqual({
      record_id: 'rec1',
      repo_id: 'web',
      note: null,
      planning: SKIP
    });
  });

  it('workspace: repo utama mengikuti urutan workspace', () => {
    const ws = { id: 'k', repo_ids: ['api', 'web', 'app'] };
    expect(
      startParams({
        recordId: 'rec1',
        note: 'x',
        target: { kind: 'workspace', workspace: ws, repoIds: ['web', 'api'] },
        planning: PLAN
      })
    ).toEqual({
      record_id: 'rec1',
      repo_id: 'api',
      note: 'x',
      workspace: { id: 'k', repo_ids: ['api', 'web'] },
      planning: PLAN
    });
  });

});
