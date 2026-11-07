// Aturan tampilan kartu pertanyaan agent (plan 2026-11-07-001, R7–R16).

import { isWorking } from '#lib/board.ts';
import type { QuestionView } from '#lib/protocol/QuestionView.ts';
import type { RunView } from '#lib/protocol/RunView.ts';

/** Label status kartu yang sudah selesai (R11); `null` selama Pending. */
export function statusLabel(q: QuestionView): string | null {
  switch (q.status) {
    case 'answered':
      return 'Answered';
    case 'answered_in_chat':
      return 'Answered in chat';
    case 'skipped':
      return 'Skipped';
    default:
      return null;
  }
}

/** Pilihan bisa diklik: kartu Pending, run tidak bekerja, dan tidak sedang dikirim (R9). */
export function canPick(q: QuestionView, run: RunView, submitting: boolean): boolean {
  return q.status === 'pending' && !isWorking(run.state) && !submitting;
}

/** Pilihan yang dipakai menjawab kartu yang Answered. */
export function chosenIndex(q: QuestionView): number {
  return q.status === 'answered' ? q.options.findIndex((o) => o.label === q.answer) : -1;
}

/** Tombol Approve dan Approve and queue for VM disembunyikan selama ada pertanyaan (R15). */
export function approveHidden(run: RunView): boolean {
  return run.pending_question !== null;
}

/** Pesan antrean ditahan karena pertanyaan: Send dan Discard tersedia (R13). */
export function queueHeld(run: RunView): boolean {
  return run.pending_question !== null && run.state === 'awaiting_dev' && run.queued.length > 0;
}
