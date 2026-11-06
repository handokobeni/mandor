// Logika tab AC review dan gerbang Push & PR (Rilis 2): label, hitungan, state
// tombol gerbang, pemilihan baris, navigasi keyboard, dan parameter publish.
// Murni agar teruji tanpa render komponen.

import type { AcLedgerView } from '#lib/protocol/AcLedgerView.ts';
import type { AcRowView } from '#lib/protocol/AcRowView.ts';
import type { PublishParams } from '#lib/protocol/PublishParams.ts';
import type { Tone } from '#lib/board.ts';

/** Baris tanpa hasil tampilan (run tanpa prototype); dipakai fixture dan mock. */
export const NO_VISUAL: Pick<AcRowView, 'visual' | 'visual_detail'> = {
  visual: 'not_compared',
  visual_detail: {
    description: null,
    app_path: null,
    prototype_path: null,
    prototype_rev: null,
    screenshots: [],
    decision_reason: null,
    decided_by: null
  }
};

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

/**
 * Verdict untuk hitungan dan gerbang, sama dengan daemon: perilaku lulus
 * tetapi tampilan berbeda dihitung gagal, tampilan yang tidak bisa dipastikan
 * dihitung Can't verify (plan 2026-11-03-002, R20).
 */
export function effectiveVerdict(r: AcRowView): string {
  const v = shownVerdict(r);
  if (v === 'pass' || v === 'checked_manually') {
    if (r.visual === 'differs') return 'fail';
    if (r.visual === 'cant_verify') return 'cant_verify';
  }
  return v;
}

export const VISUAL_META: Record<string, ChipMeta> = {
  match: { label: 'Looks like the prototype', tone: 'ok' },
  differs: { label: 'Looks different', tone: 'failed' },
  cant_verify: { label: 'Look not verified', tone: 'warning' },
  accepted: { label: 'Difference accepted', tone: 'muted' },
  checked_manually: { label: 'Look checked', tone: 'ok' }
};

/** Chip hasil tampilan; `null` bila baris tidak dibandingkan. */
export function visualMeta(visual: string): ChipMeta | null {
  return VISUAL_META[visual] ?? null;
}

/** Baris yang ditawarkan dialog gerbang: gagal (Push anyway) dan Can't verify (cek manual). */
export function gateRows(rows: AcRowView[]): { fails: AcRowView[]; cants: AcRowView[] } {
  const live = rows.filter((r) => r.in_gate && !r.stale && r.status !== 'removed');
  return {
    fails: live.filter((r) => effectiveVerdict(r) === 'fail'),
    cants: live.filter((r) => effectiveVerdict(r) === 'cant_verify')
  };
}

/** Alasan baris memblokir push: bukti perilaku yang gagal, atau perbedaan
 * tampilannya dengan prototype. */
export function blockReason(r: AcRowView): string {
  if (r.verdict === 'fail') return r.evidence ?? r.text;
  const why = r.visual_detail.description;
  return why ? `Looks different: ${why}` : 'Looks different from the prototype';
}

/** Alasan baris diminta dikonfirmasi di gerbang: bukti perilaku yang tak
 * terverifikasi dan/atau alasan tampilannya tak terverifikasi. */
export function cantReasons(r: AcRowView): string[] {
  const out: string[] = [];
  if (r.verdict === 'cant_verify' && r.evidence) out.push(r.evidence);
  if (r.visual === 'cant_verify') {
    const why = r.visual_detail.description;
    out.push(why ? `Look not verified: ${why}` : 'Look not verified');
  }
  return out;
}

/** Link halaman yang dibandingkan reviewer; hanya path dengan origin yang sama (KTD-P6). */
export function visualLinks(
  r: AcRowView,
  appUrl: string | null,
  prototypeUrl: string | null
): { app: string | null; prototype: string | null } {
  const link = (path: string | null, base: string | null): string | null => {
    if (!path || !base) return null;
    try {
      const origin = new URL(base).origin;
      const url = new URL(path, origin);
      return url.origin === origin && path.startsWith('/') && !path.startsWith('//') ? url.href : null;
    } catch {
      return null;
    }
  };
  return {
    app: link(r.visual_detail.app_path, appUrl),
    prototype: link(r.visual_detail.prototype_path, prototypeUrl)
  };
}

/** Judul AC: baris pertama yang tidak kosong (teks bisa memuat skenario Gherkin). */
export function acTitle(text: string): string {
  return (
    text
      .split('\n')
      .map((l) => l.trim())
      .find((l) => l.length > 0) ?? ''
  );
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
    switch (effectiveVerdict(r)) {
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
export function gateButtonLabel(g: Gate, hasPr = false): string {
  switch (g.state) {
    case 'reviewing':
      return 'Reviewing AC…';
    case 'blocked':
      return 'Push blocked';
    case 'warn':
      return `Push with ${g.warnings} warning${g.warnings === 1 ? '' : 's'}`;
    default:
      return hasPr ? 'Push to PR' : 'Push & create PR';
  }
}

/** Hitungan di label tab: "5/7" dan penanda gagal; kosong bila tanpa AC. */
export function tabCount(rows: AcRowView[]): { text: string; failed: number } | null {
  const c = counts(rows);
  if (c.total === 0) return null;
  return { text: `${c.ok}/${c.total}`, failed: c.fail };
}

/**
 * Baris Fail, atau yang tampilannya berbeda dari prototype, yang masih berlaku
 * boleh dikirim ke agent; Follow design dan Pass tidak.
 */
export function canSend(r: AcRowView): boolean {
  return r.selectable && (r.verdict === 'fail' || r.visual === 'differs') && !r.stale && r.status !== 'removed';
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

/** Pilihan Send to agent yang masih bisa dikirim: Fail atau tampilan berbeda
 * (baris lain dibuang, mis. setelah review baru). */
export function pruneSelection(selected: Set<string>, rows: AcRowView[]): Set<string> {
  const ids = new Set(rows.filter(canSend).map((r) => r.ac_id));
  return new Set([...selected].filter((id) => ids.has(id)));
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

/**
 * Keterangan header tab AC review saat belum ada riwayat review di daemon ini:
 * verdict yang ada dibawa dari review VM saat Take over.
 */
export function reviewMeta(rows: AcRowView[]): string {
  return rows.some((r) => r.verdict !== 'not_reviewed') ? 'Verdicts from the VM review' : 'No review yet';
}

/** Label screenshot tampilan: reviewer menyimpan halaman app lalu prototype. */
export function shotLabels(n: number): string[] {
  if (n === 2) return ['App page', 'Prototype page'];
  return Array.from({ length: n }, (_, i) => `Screenshot ${i + 1}`);
}

/** Tingkat zoom penampil screenshot; 1 = ukuran asli (piksel tajam). */
export const ZOOM_STEPS = [0.25, 0.5, 0.75, 1, 1.5, 2, 3, 4] as const;

/** Zoom berikutnya dari `fit` atau tingkat sekarang, dibatasi rentang. */
export function nextZoom(current: 'fit' | number, dir: 1 | -1): number {
  if (current === 'fit') return dir > 0 ? 1 : 0.5;
  const i = ZOOM_STEPS.findIndex((z) => z >= current);
  const at = i === -1 ? ZOOM_STEPS.length - 1 : i;
  const next = Math.min(ZOOM_STEPS.length - 1, Math.max(0, at + dir));
  return ZOOM_STEPS[next];
}
