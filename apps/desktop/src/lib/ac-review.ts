// Logika tab AC review dan gerbang Push & PR (Rilis 2): label, hitungan, state
// tombol gerbang, pemilihan baris, navigasi keyboard, dan parameter publish.
// Murni agar teruji tanpa render komponen.

import type { AcLedgerView } from '#lib/protocol/AcLedgerView.ts';
import type { AcRowView } from '#lib/protocol/AcRowView.ts';
import type { PublishParams } from '#lib/protocol/PublishParams.ts';
import type { Tone } from '#lib/board.ts';

export interface ChipMeta {
  label: string;
  tone: Tone | 'pr';
}

export const DESIGN_META: Record<string, ChipMeta> = {
  not_checked: { label: 'Not checked yet', tone: 'muted' },
  in_design: { label: 'In design', tone: 'ok' },
  partly: { label: 'Partly in design', tone: 'waiting' },
  missing: { label: 'Missing from design', tone: 'warning' },
  conflicts: { label: 'Conflicts with design', tone: 'failed' },
  follow_design: { label: 'Follow design', tone: 'pr' }
};

export const VERDICT_META: Record<string, ChipMeta> = {
  not_reviewed: { label: 'Not reviewed', tone: 'muted' },
  pass: { label: 'Pass', tone: 'ok' },
  fail: { label: 'Fail', tone: 'failed' },
  cant_verify: { label: "Can't verify", tone: 'warning' },
  checked_manually: { label: 'Checked manually', tone: 'ok' },
  skipped: { label: 'Skipped', tone: 'muted' },
  fix_in_progress: { label: 'Fix in progress', tone: 'working' },
  follows_design: { label: 'Follows design', tone: 'pr' }
};

/** Verdict yang ditampilkan: baris Follow design tidak dinilai terhadap AC. */
export function shownVerdict(r: AcRowView): string {
  return r.design === 'follow_design' ? 'follows_design' : r.verdict;
}

export function label(r: AcRowView): string {
  return r.label ?? 'AC';
}

/** Baris aktif (bukan Removed in Lark). */
export function activeRows(rows: AcRowView[]): AcRowView[] {
  return rows.filter((r) => r.status !== 'removed');
}

export function removedRows(rows: AcRowView[]): AcRowView[] {
  return rows.filter((r) => r.status === 'removed');
}

export interface Counts {
  total: number;
  ok: number;
  pass: number;
  fail: number;
  cant: number;
  manual: number;
  skipped: number;
  follows: number;
  notReviewed: number;
  fixing: number;
  stale: number;
}

/** Hitungan per verdict; baris Removed tidak dihitung. */
export function counts(rows: AcRowView[]): Counts {
  const c: Counts = {
    total: 0,
    ok: 0,
    pass: 0,
    fail: 0,
    cant: 0,
    manual: 0,
    skipped: 0,
    follows: 0,
    notReviewed: 0,
    fixing: 0,
    stale: 0
  };
  for (const r of activeRows(rows)) {
    c.total++;
    if (r.stale) c.stale++;
    switch (shownVerdict(r)) {
      case 'pass':
        c.pass++;
        c.ok++;
        break;
      case 'checked_manually':
        c.manual++;
        c.ok++;
        break;
      case 'skipped':
        c.skipped++;
        c.ok++;
        break;
      case 'follows_design':
        c.follows++;
        c.ok++;
        break;
      case 'fail':
        c.fail++;
        break;
      case 'cant_verify':
        c.cant++;
        break;
      case 'fix_in_progress':
        c.fixing++;
        break;
      default:
        c.notReviewed++;
    }
  }
  return c;
}

export type GateState =
  | 'no_ac'
  | 'reviewing'
  | 'review_needed'
  | 'warn'
  | 'blocked'
  | 'needs_confirmation'
  | 'ready';

export interface Gate {
  state: GateState;
  text: string;
  tone: Tone;
  /** Fail + Can't verify (mode peringatan). */
  warnings: number;
  counts: Counts;
}

/**
 * State gerbang dari baris buku AC, sama dengan evaluasi daemon (KTD-35).
 * Mode peringatan tidak pernah menghasilkan Blocked atau Needs confirmation.
 */
export function gate(rows: AcRowView[], mode: string, reviewing: boolean): Gate {
  const c = counts(rows);
  const warnings = c.fail + c.cant;
  let state: GateState;
  if (c.total === 0) state = 'no_ac';
  else if (reviewing) state = 'reviewing';
  else if (c.stale > 0 || c.notReviewed > 0 || c.fixing > 0) state = 'review_needed';
  else if (mode === 'warn' && warnings > 0) state = 'warn';
  else if (c.fail > 0) state = 'blocked';
  else if (c.cant > 0) state = 'needs_confirmation';
  else state = 'ready';
  const why = c.stale > 0 ? 'code changed after the last review' : `${c.notReviewed + c.fixing} AC not reviewed`;
  const warnParts = [c.fail ? `${c.fail} AC failed` : '', c.cant ? `${c.cant} can't be verified` : ''].filter(Boolean);
  const text: Record<GateState, string> = {
    no_ac: 'No AC linked',
    reviewing: 'Reviewing AC',
    review_needed: `Review needed: ${why}`,
    warn: `${warnParts.join(', ')} (warn mode)`,
    blocked: `Push blocked by ${c.fail} AC`,
    needs_confirmation: `Needs your check: ${c.cant} AC can't be verified`,
    ready: 'Ready to push'
  };
  const tone: Record<GateState, Tone> = {
    no_ac: 'muted',
    reviewing: 'working',
    review_needed: 'waiting',
    warn: 'warning',
    blocked: 'failed',
    needs_confirmation: 'warning',
    ready: 'ok'
  };
  return { state, text: text[state], tone: tone[state], warnings, counts: c };
}

/** Label tombol Push & PR per state gerbang. */
export function gateButtonLabel(g: Gate): string {
  switch (g.state) {
    case 'reviewing':
      return 'Reviewing AC…';
    case 'blocked':
      return 'Push blocked';
    case 'warn':
      return `Push with ${g.warnings} warning${g.warnings === 1 ? '' : 's'}`;
    default:
      return 'Push & create PR';
  }
}

/** Hitungan di label tab: "5/7" dan penanda gagal; kosong bila tanpa AC. */
export function tabCount(rows: AcRowView[]): { text: string; failed: number } | null {
  const c = counts(rows);
  if (c.total === 0) return null;
  return { text: `${c.ok}/${c.total}`, failed: c.fail };
}

/** Baris Fail yang masih berlaku boleh dikirim ke agent; Follow design dan Pass tidak. */
export function canSend(r: AcRowView): boolean {
  return r.selectable && r.verdict === 'fail' && !r.stale && r.status !== 'removed';
}

/** Penanda baris dari data tracker. */
export function rowMarks(r: AcRowView): ('new' | 'changed' | 'removed')[] {
  if (r.status === 'new' || r.status === 'changed' || r.status === 'removed') return [r.status];
  return [];
}

/** Indeks fokus berikutnya untuk navigasi panah, Home, dan End. */
export function moveFocus(index: number, key: string, length: number): number {
  if (length === 0) return 0;
  switch (key) {
    case 'ArrowDown':
      return Math.min(length - 1, index + 1);
    case 'ArrowUp':
      return Math.max(0, index - 1);
    case 'Home':
      return 0;
    case 'End':
      return length - 1;
    default:
      return index;
  }
}

/** Space memilih hanya baris yang boleh dikirim. */
export function toggleSelection(selected: Set<string>, r: AcRowView): Set<string> {
  const next = new Set(selected);
  if (!canSend(r)) return next;
  if (next.has(r.ac_id)) next.delete(r.ac_id);
  else next.add(r.ac_id);
  return next;
}

/** Alasan Push anyway wajib berisi selain spasi. */
export function validReason(reason: string): boolean {
  return reason.trim().length > 0;
}

/** Parameter `runs.publish` dari keputusan dev di dialog gerbang. */
export function publishParams(runId: string, confirmed: string[], reason: string | null): PublishParams {
  const p: PublishParams = { run_id: runId };
  if (confirmed.length > 0) p.confirmed = [...confirmed];
  if (reason !== null && validReason(reason)) p.push_anyway_reason = reason.trim();
  return p;
}

/** Bagian bukti: teks biasa dan rujukan `berkas:baris` yang membuka tab Changes. */
export type EvidencePart = { kind: 'text'; text: string } | { kind: 'file'; path: string; line: number; text: string };

const FILE_REF = /([\w.@/-]+\.[A-Za-z0-9]+):(\d+)/g;

export function evidenceParts(text: string): EvidencePart[] {
  const out: EvidencePart[] = [];
  let last = 0;
  for (const m of text.matchAll(FILE_REF)) {
    const at = m.index ?? 0;
    if (at > last) out.push({ kind: 'text', text: text.slice(last, at) });
    out.push({ kind: 'file', path: m[1], line: Number(m[2]), text: m[0] });
    last = at + m[0].length;
  }
  if (last < text.length) out.push({ kind: 'text', text: text.slice(last) });
  return out;
}

/** Ringkasan "No design source" sekali di header kolom Design. */
export function noDesignSource(ledger: Pick<AcLedgerView, 'design_sources'>): boolean {
  return ledger.design_sources.length === 0;
}
