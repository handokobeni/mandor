// Logika murni run grup (plan run grup U9): progres kartu, aksi berikutnya,
// pemilihan dan urutan kartu, serta pengelompokan buku AC per kartu.
import type { AcRowView } from './protocol/AcRowView.ts';
import type { GroupCandidateView } from './protocol/GroupCandidateView.ts';
import type { GroupCardView } from './protocol/GroupCardView.ts';

export interface Progress {
  done: number;
  active: number;
  pending: number;
  removed: number;
  /** Kartu yang masih disertakan (tanpa yang dikeluarkan). */
  total: number;
}

export function isRemoved(c: GroupCardView): boolean {
  return c.state.startsWith('removed');
}

export function cardKey(c: { key: string | null; record_id: string }): string {
  return c.key ?? c.record_id;
}

export function progress(cards: GroupCardView[]): Progress {
  const count = (s: string) => cards.filter((c) => c.state === s).length;
  const removed = cards.filter(isRemoved).length;
  return {
    done: count('done'),
    active: count('active'),
    pending: count('pending'),
    removed,
    total: cards.length - removed
  };
}

export function progressText(p: Progress): string {
  const parts = [`${p.done} of ${p.total} done`];
  if (p.active) parts.push(`${p.active} active`);
  if (p.pending) parts.push(`${p.pending} waiting`);
  if (p.removed) parts.push(`${p.removed} removed`);
  return parts.join(' · ');
}

/**
 * Aksi grup setelah giliran kartu selesai: `next` (kartu berikutnya), `finish`
 * (kartu terakhir), `continue` (kartu dikeluarkan, lanjut ke kartu berikutnya),
 * atau `null` bila bukan saatnya atau semua kartu selesai (Push & PR).
 */
export function groupAction(run: {
  state: string;
  phase: string;
  cards: GroupCardView[];
}): 'next' | 'finish' | 'continue' | null {
  if (run.cards.length === 0 || run.state !== 'awaiting_dev' || run.phase !== 'implement') return null;
  const active = run.cards.some((c) => c.state === 'active');
  const pending = run.cards.some((c) => c.state === 'pending');
  if (active) return pending ? 'next' : 'finish';
  return pending ? 'continue' : null;
}

const STATE_LABELS: Record<string, string> = {
  pending: 'Waiting',
  active: 'Working',
  done: 'Done',
  removed_changed: 'Removed: changed in Lark',
  removed_by_dev: 'Removed',
  removed_reverted: 'Reverted',
  removed_follow_up: 'Moved to a follow-up PR'
};

export function stateLabel(state: string): string {
  return STATE_LABELS[state] ?? state;
}

/**
 * Kunci dan judul di header workspace: kartu grup yang sedang dikerjakan, atau
 * kartu utama run bila tidak ada kartu aktif (run tunggal, atau semua selesai).
 */
export function headerCard(run: {
  key: string | null;
  title: string;
  record_id: string;
  cards: GroupCardView[];
}): { key: string; title: string } {
  const active = run.cards.find((c) => c.state === 'active');
  if (active) return { key: cardKey(active), title: active.title };
  return { key: run.key ?? run.record_id, title: run.title };
}

/** Kartu selesai yang berubah di Lark dan belum diputuskan dev. */
export function changedCards(cards: GroupCardView[]): GroupCardView[] {
  return cards.filter((c) => c.state === 'done' && c.changed_in_lark && !c.kept_reason);
}

/** Urutan awal: semua kandidat yang bisa dipilih, urut kandidat. */
export function initialOrder(candidates: GroupCandidateView[]): string[] {
  return candidates.filter((c) => !c.blocked).map((c) => c.record_id);
}

/** Centang atau hapus centang; kartu yang dicentang ulang masuk di akhir urutan. */
export function toggleCard(order: string[], id: string): string[] {
  return order.includes(id) ? order.filter((x) => x !== id) : [...order, id];
}

/** Menggeser kartu satu langkah; di ujung tidak berubah. */
export function moveCard(order: string[], id: string, delta: -1 | 1): string[] {
  const i = order.indexOf(id);
  const j = i + delta;
  if (i < 0 || j < 0 || j >= order.length) return order;
  const out = [...order];
  [out[i], out[j]] = [out[j], out[i]];
  return out;
}

export interface CardRows {
  /** `null` untuk baris tanpa kartu (run tunggal). */
  card: GroupCardView | null;
  rows: AcRowView[];
}

/** Baris buku AC per kartu, urut posisi kartu; baris tanpa kartu di akhir. */
export function rowsByCard(rows: AcRowView[], cards: GroupCardView[]): CardRows[] {
  const out: CardRows[] = cards
    .map((card) => ({ card, rows: rows.filter((r) => r.card === card.record_id) }))
    .filter((g) => g.rows.length > 0);
  const rest = rows.filter((r) => !r.card || !cards.some((c) => c.record_id === r.card));
  if (rest.length > 0) out.push({ card: null, rows: rest });
  return out;
}
