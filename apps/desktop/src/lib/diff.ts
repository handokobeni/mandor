// Unified diff dari `git diff` menjadi baris tabel dengan nomor baris lama dan baru.

export interface DiffRow {
  kind: 'hunk' | 'ctx' | 'add' | 'del';
  old: number | null;
  new: number | null;
  text: string;
}

const HUNK = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/;

export function parsePatch(patch: string): DiffRow[] {
  const rows: DiffRow[] = [];
  let oldNo = 0;
  let newNo = 0;
  let inHunk = false;
  for (const line of patch.split('\n')) {
    const m = HUNK.exec(line);
    if (m) {
      oldNo = Number(m[1]);
      newNo = Number(m[2]);
      inHunk = true;
      rows.push({ kind: 'hunk', old: null, new: null, text: line });
      continue;
    }
    // Header berkas (diff --git, index, ---, +++) muncul sebelum hunk pertama.
    if (!inHunk || line.startsWith('\\')) continue;
    if (line.startsWith('diff --git')) {
      inHunk = false;
      continue;
    }
    if (line.startsWith('+')) rows.push({ kind: 'add', old: null, new: newNo++, text: line.slice(1) });
    else if (line.startsWith('-')) rows.push({ kind: 'del', old: oldNo++, new: null, text: line.slice(1) });
    else if (line.startsWith(' ')) rows.push({ kind: 'ctx', old: oldNo++, new: newNo++, text: line.slice(1) });
  }
  return rows;
}
