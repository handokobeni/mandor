// Logika tab Overnight dan badge kartu VM di board laptop (U10 plan VM).

import type { CardView } from '#lib/protocol/CardView.ts';
import type { LoginCheck } from '#lib/protocol/LoginCheck.ts';
import type { OvernightReportView } from '#lib/protocol/OvernightReportView.ts';
import type { OvernightSettingsView } from '#lib/protocol/OvernightSettingsView.ts';
import type { ReportCardView } from '#lib/protocol/ReportCardView.ts';

export type VmBadge = 'on_vm' | 'queued';

/**
 * Badge kartu di board laptop: "On VM" bila klaimnya dipegang daemon VM milik
 * saya (R17), "Queued for VM" bila ada di antrean VM (R25, KTD-70).
 * `queued` `null` berarti link VM terputus: antrean tidak diketahui.
 */
export function vmBadge(card: CardView, queued: Set<string> | null): VmBadge | null {
  if (card.holder === 'own_other_daemon') return 'on_vm';
  if (queued?.has(card.record_id)) return 'queued';
  return null;
}

/**
 * Kartu yang bisa dikirim ke antrean VM: To Do di papan saya (bukan papan
 * Team), tanpa run, dan bisa di-Start (lolos readiness, tidak diklaim orang lain).
 */
export function canQueue(card: CardView, queued: Set<string>): boolean {
  return (
    card.column === 'todo' &&
    !card.team &&
    !card.run &&
    card.holder !== 'own_other_daemon' &&
    card.start_blocked === null &&
    !queued.has(card.record_id)
  );
}

/** VM mulai bekerja hanya bila batas pemakaian per siklus diisi (R10). */
export function canActivate(settings: OvernightSettingsView | null): boolean {
  return (settings?.token_limit ?? 0) > 0;
}

const LOGIN_LABEL: Record<string, string> = {
  lark: 'Lark',
  gh: 'GitHub CLI',
  claude: 'Claude Code (agent user)'
};

export interface LoginNotice {
  label: string;
  tone: 'ok' | 'warning' | 'failed';
  state: string;
  fix: string | null;
}

/** Tiga login VM untuk ditampilkan; `expired` peringatan, `missing` gagal. */
export function loginNotices(logins: LoginCheck[]): LoginNotice[] {
  return logins.map((l) => ({
    label: LOGIN_LABEL[l.service] ?? l.service,
    tone: l.state === 'ok' ? 'ok' : l.state === 'expired' ? 'warning' : 'failed',
    state: l.state,
    fix: l.fix
  }));
}

export interface ReportGroup {
  id: 'draft' | 'ac_gate' | 'held' | 'sent_to_review' | 'failed' | 'working';
  label: string;
  cards: ReportCardView[];
}

const GROUPS: { id: ReportGroup['id']; label: string; match: (c: ReportCardView) => boolean }[] = [
  { id: 'draft', label: 'Draft PRs ready', match: (c) => c.outcome === 'draft' },
  { id: 'ac_gate', label: 'Parked: AC gate', match: (c) => c.outcome === 'held' && c.hold_reason === 'ac_gate' },
  { id: 'held', label: 'Parked: needs you', match: (c) => c.outcome === 'held' && c.hold_reason !== 'ac_gate' },
  { id: 'sent_to_review', label: 'Sent to review', match: (c) => c.outcome === 'sent_to_review' },
  { id: 'failed', label: 'Failed', match: (c) => c.outcome === 'failed' },
  { id: 'working', label: 'Working', match: (c) => c.outcome === 'working' }
];

/** Kartu laporan dikelompokkan per hasil (R18); grup kosong tidak ditampilkan. */
export function reportGroups(report: OvernightReportView): ReportGroup[] {
  return GROUPS.map((g) => ({ id: g.id, label: g.label, cards: report.cards.filter(g.match) })).filter(
    (g) => g.cards.length > 0
  );
}

const HOLD_LABEL: Record<string, string> = {
  stuck: 'Agent needs an answer',
  ac_gate: 'AC gate',
  needs_confirmation: 'Sensitive files changed',
  usage_limit: 'Usage limit reached'
};

export function holdLabel(reason: string | null): string {
  return reason ? (HOLD_LABEL[reason] ?? reason) : '';
}

/** Pemakaian siklus terhadap batasnya. */
export function usageText(r: Pick<OvernightReportView, 'used' | 'limit'>): string {
  const n = (v: number) => v.toLocaleString('en-US');
  if (!r.limit) return `${n(r.used)} tokens · no limit set`;
  return `${n(r.used)} of ${n(r.limit)} tokens (${Math.round((r.used / r.limit) * 100)}%)`;
}
