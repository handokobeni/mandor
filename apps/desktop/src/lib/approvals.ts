// Kartu persetujuan izin agent (plan 2026-11-06-001, U7): logika tampilan murni.

import type { ApprovalView } from '#lib/protocol/ApprovalView.ts';

/** Karakter kontrol, format (termasuk pengatur arah teks), pemisah baris
 * Unicode, area pribadi, belum terdefinisi, dan pengisi Hangul yang tampak
 * kosong; baris baru dan tab tetap. */
const HIDDEN = /(?![\n\t])[\p{Cc}\p{Cf}\p{Zl}\p{Zp}\p{Co}\p{Cn}\u115F\u1160\u3164\uFFA0]/gu;

/** Teks dengan karakter tersembunyi ditulis sebagai `\u{..}` agar terlihat (R5). */
export function visible(text: string): string {
  return text.replace(HIDDEN, (c) => `\\u{${(c.codePointAt(0) ?? 0).toString(16)}}`);
}

/** Isi utama kartu: perintah Bash apa adanya, selain itu pasangan kunci-nilai. */
export function inputText(card: ApprovalView): string {
  const input = card.input as Record<string, unknown>;
  const { description: _ignored, ...rest } = input;
  if (card.tool === 'Bash' && typeof input.command === 'string') {
    const { command, ...other } = rest;
    // Kunci lain (mis. `run_in_background`) ikut tampil agar dev melihat semuanya.
    const extra = Object.keys(other).length > 0 ? `\n\n${JSON.stringify(other, null, 2)}` : '';
    return visible(`${String(command)}${extra}`);
  }
  return visible(JSON.stringify(rest, null, 2));
}

/** Sisa waktu `m:ss` sampai tenggat, minimal 0 (R8). */
export function remaining(card: ApprovalView, nowMs: number): string {
  const secs = Math.max(0, Math.ceil((card.deadline_ms - nowMs) / 1000));
  return `${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, '0')}`;
}

/** Kartu urut dari yang terlama (R24). */
export function sortCards(cards: ApprovalView[]): ApprovalView[] {
  return [...cards].sort((a, b) => a.created_at_ms - b.created_at_ms);
}

/** Kartu menunggu yang terlama, tujuan notifikasi. */
export function oldestPending(cards: ApprovalView[]): ApprovalView | undefined {
  return sortCards(cards).find((c) => c.status === 'pending');
}

/** Tombol aktif hanya untuk kartu menunggu yang jawabannya belum dikirim (R20). */
export function canAnswer(card: ApprovalView, submitting: boolean): boolean {
  return card.status === 'pending' && !submitting;
}

/** Keputusan kartu untuk Chat (R10). */
export function answerLabel(card: ApprovalView): string {
  switch (card.status) {
    case 'pending':
      return 'Waiting for you';
    case 'expired':
      return 'Expired after 10 minutes, denied';
    case 'cancelled':
      return 'Cancelled, the run stopped';
    default:
      break;
  }
  switch (card.answer) {
    case 'allow_once':
      return 'Allowed once';
    case 'allow_run':
      return 'Allowed for this run';
    default:
      return 'Denied';
  }
}
