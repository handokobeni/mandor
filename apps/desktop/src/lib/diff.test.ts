import { describe, expect, it } from 'vitest';
import { parsePatch } from '#lib/diff.ts';

describe('parser diff', () => {
  it('memberi nomor baris lama dan baru per hunk, melewati header berkas', () => {
    const rows = parsePatch(
      [
        'diff --git a/src/a.ts b/src/a.ts',
        'index 1111..2222 100644',
        '--- a/src/a.ts',
        '+++ b/src/a.ts',
        '@@ -12,3 +12,4 @@ fn main',
        ' const a = 1;',
        '-const b = 2;',
        '+const b = 3;',
        '+const c = 4;',
        ' end',
        '\\ No newline at end of file'
      ].join('\n')
    );
    expect(rows).toEqual([
      { kind: 'hunk', old: null, new: null, text: '@@ -12,3 +12,4 @@ fn main' },
      { kind: 'ctx', old: 12, new: 12, text: 'const a = 1;' },
      { kind: 'del', old: 13, new: null, text: 'const b = 2;' },
      { kind: 'add', old: null, new: 13, text: 'const b = 3;' },
      { kind: 'add', old: null, new: 14, text: 'const c = 4;' },
      { kind: 'ctx', old: 14, new: 15, text: 'end' }
    ]);
  });

  it('patch kosong menghasilkan daftar kosong', () => {
    expect(parsePatch('')).toEqual([]);
  });
});
