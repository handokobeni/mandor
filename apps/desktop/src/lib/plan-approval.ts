// Persetujuan plan dengan selisih desain (R8a, brief §4.2): baris Missing,
// Partly, dan Conflicts bisa ditandai Follow design sebelum approve.

import type { AcRowView } from '#lib/protocol/AcRowView.ts';

const GAPS = ['missing', 'partly', 'conflicts'];

/** Baris yang bisa ditandai Follow design. */
export function followCandidates(rows: AcRowView[]): AcRowView[] {
  return rows.filter((r) => r.status !== 'removed' && GAPS.includes(r.design));
}

/** Menandai atau membatalkan (Undo) satu baris; daftar baru berlaku saat approve. */
export function toggleFollow(follow: string[], acId: string): string[] {
  return follow.includes(acId) ? follow.filter((x) => x !== acId) : [...follow, acId];
}
