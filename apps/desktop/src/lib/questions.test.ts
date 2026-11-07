import { describe, expect, it } from 'vitest';
import type { QuestionView } from '#lib/protocol/QuestionView.ts';
import type { RunView } from '#lib/protocol/RunView.ts';
import { approveHidden, canPick, chosenIndex, queueHeld, statusLabel } from './questions.ts';

function q(status: string, answer: string | null = null): QuestionView {
  return {
    id: 'q-1',
    run_id: 'r1',
    question: 'Keep?',
    options: [
      { label: 'Keep both', description: 'both', recommended: true },
      { label: 'Replace', description: 'new', recommended: false }
    ],
    status,
    answer,
    created_at_ms: 1,
    closed_at_ms: null
  };
}

function run(state: string, pending: string | null, queued: string[] = []): RunView {
  return { state, pending_question: pending, queued } as unknown as RunView;
}

describe('kartu pertanyaan', () => {
  it('label status akhir (R11)', () => {
    expect(statusLabel(q('pending'))).toBeNull();
    expect(statusLabel(q('answered'))).toBe('Answered');
    expect(statusLabel(q('answered_in_chat'))).toBe('Answered in chat');
    expect(statusLabel(q('skipped'))).toBe('Skipped');
  });

  it('pilihan hanya aktif saat Pending, run tidak bekerja, dan tidak dikirim', () => {
    expect(canPick(q('pending'), run('awaiting_dev', 'q-1'), false)).toBe(true);
    expect(canPick(q('pending'), run('failed', 'q-1'), false)).toBe(true);
    expect(canPick(q('pending'), run('running', 'q-1'), false)).toBe(false);
    expect(canPick(q('pending'), run('awaiting_dev', 'q-1'), true)).toBe(false);
    expect(canPick(q('answered', 'Replace'), run('awaiting_dev', null), false)).toBe(false);
  });

  it('pilihan terpakai ditandai', () => {
    expect(chosenIndex(q('answered', 'Replace'))).toBe(1);
    expect(chosenIndex(q('answered_in_chat', 'Replace'))).toBe(-1);
  });

  it('Approve disembunyikan selama ada pertanyaan (R15, AE6)', () => {
    expect(approveHidden(run('awaiting_dev', 'q-1'))).toBe(true);
    expect(approveHidden(run('awaiting_dev', null))).toBe(false);
  });

  it('Send dan Discard antrean hanya saat ditahan pertanyaan (R13, AE9)', () => {
    expect(queueHeld(run('awaiting_dev', 'q-1', ['catatan']))).toBe(true);
    expect(queueHeld(run('awaiting_dev', 'q-1', []))).toBe(false);
    expect(queueHeld(run('running', 'q-1', ['catatan']))).toBe(false);
    expect(queueHeld(run('awaiting_dev', null, ['catatan']))).toBe(false);
  });
});
