import { describe, expect, it } from 'vitest';
import type { ApprovalView } from '#lib/protocol/ApprovalView.ts';
import {
  answerLabel,
  canAnswer,
  inputText,
  oldestPending,
  remaining,
  sortCards,
  visible
} from '#lib/approvals.ts';

function card(over: Partial<ApprovalView> = {}): ApprovalView {
  return {
    id: 'a1',
    run_id: 'r1',
    turn: 'implement',
    tool: 'Bash',
    input: { command: 'docker ps', description: 'List containers' },
    description: 'List containers',
    cwd: '/w/api',
    status: 'pending',
    answer: null,
    created_at_ms: 1_000,
    deadline_ms: 601_000,
    closed_at_ms: null,
    ...over
  };
}

describe('kartu persetujuan', () => {
  it('karakter kontrol dan bidi di-escape agar terlihat', () => {
    expect(visible('ls ‮gnp.exe')).toBe('ls \\u{202e}gnp.exe');
    expect(visible('a\u0007b\tc')).toBe('a\\u{7}b\tc');
    expect(visible('baris\nkedua')).toBe('baris\nkedua');
  });

  it('input panjang tampil utuh', () => {
    const big = 'x'.repeat(64 * 1024);
    expect(inputText(card({ input: { command: big } }))).toHaveLength(big.length);
  });

  it('input non-Bash tampil sebagai pasangan kunci-nilai tanpa deskripsi', () => {
    const text = inputText(card({ tool: 'WebFetch', input: { url: 'https://docs.x/a', prompt: 'p' } }));
    expect(text).toContain('"url": "https://docs.x/a"');
    expect(inputText(card())).toBe('docker ps');
  });

  it('sisa waktu dari tenggat berhenti di 0', () => {
    expect(remaining(card(), 1_000)).toBe('10:00');
    expect(remaining(card(), 540_500)).toBe('1:01');
    expect(remaining(card(), 999_999)).toBe('0:00');
  });

  it('urut dari yang terlama dan kartu menunggu terlama', () => {
    const cards = [
      card({ id: 'b', created_at_ms: 3 }),
      card({ id: 'a', created_at_ms: 2, status: 'answered' }),
      card({ id: 'c', created_at_ms: 1 })
    ];
    expect(sortCards(cards).map((c) => c.id)).toEqual(['c', 'a', 'b']);
    expect(oldestPending(cards)?.id).toBe('c');
  });

  it('tombol hanya untuk kartu menunggu yang tidak sedang dikirim', () => {
    expect(canAnswer(card(), false)).toBe(true);
    expect(canAnswer(card(), true)).toBe(false);
    for (const status of ['answered', 'expired', 'cancelled']) {
      expect(canAnswer(card({ status }), false)).toBe(false);
    }
  });

  it('keputusan kartu yang sudah selesai tetap tertulis', () => {
    expect(answerLabel(card({ status: 'answered', answer: 'allow_run' }))).toBe('Allowed for this run');
    expect(answerLabel(card({ status: 'answered', answer: 'deny' }))).toBe('Denied');
    expect(answerLabel(card({ status: 'expired' }))).toBe('Expired after 10 minutes, denied');
    expect(answerLabel(card({ status: 'cancelled' }))).toBe('Cancelled, the run stopped');
    expect(answerLabel(card())).toBe('Waiting for you');
  });
});
