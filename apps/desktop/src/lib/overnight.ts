// Logika tab Overnight dan badge kartu VM di board laptop (U10 plan VM).

import type { CardView } from '#lib/protocol/CardView.ts';
import type { DesignSourceView } from '#lib/protocol/DesignSourceView.ts';
import type { QueueItemView } from '#lib/protocol/QueueItemView.ts';
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
  const n = formatTokens;
  if (!r.limit) return `${n(r.used)} tokens · no limit set`;
  return `${n(r.used)} of ${n(r.limit)} tokens (${Math.round((r.used / r.limit) * 100)}%)`;
}

/** Jam mulai/selesai lembur tiap 30 menit; nilai tersimpan di luar langkah ikut ditampilkan. */
export function timeOptions(current: string): string[] {
  const out: string[] = [];
  for (let m = 0; m < 24 * 60; m += 30) {
    out.push(`${String(Math.floor(m / 60)).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`);
  }
  if (current && !out.includes(current)) {
    out.push(current);
    out.sort();
  }
  return out;
}

const ZONE_NAMES: Record<string, string> = { '+07:00': 'WIB', '+08:00': 'WITA', '+09:00': 'WIT' };

/** Offset UTC per jam dari −12:00 sampai +14:00; offset tersimpan lain (mis. +05:45) ikut. */
export function offsetOptions(current: string): { value: string; label: string }[] {
  const values: string[] = [];
  for (let h = -12; h <= 14; h++) {
    values.push(`${h < 0 ? '-' : '+'}${String(Math.abs(h)).padStart(2, '0')}:00`);
  }
  if (current && !values.includes(current)) values.push(current);
  const minutes = (v: string): number => (v.startsWith('-') ? -1 : 1) * (Number(v.slice(1, 3)) * 60 + Number(v.slice(4, 6)));
  return values
    .sort((a, b) => minutes(a) - minutes(b))
    .map((value) => ({ value, label: ZONE_NAMES[value] ? `UTC${value} · ${ZONE_NAMES[value]}` : `UTC${value}` }));
}

/** Jumlah token dengan pemisah ribuan. */
export function formatTokens(n: number): string {
  return n.toLocaleString('en-US');
}

/**
 * Kartu laporan yang tertahan atau gagal tetapi sudah kembali ke antrean VM:
 * run-nya baru berubah saat VM melanjutkannya, jadi laporan tidak lagi
 * meminta keputusan. Link VM terputus (`queued` null) = tidak diketahui.
 */
export function backInQueue(card: Pick<ReportCardView, 'record_id' | 'outcome'>, queued: Set<string> | null): boolean {
  return (card.outcome === 'held' || card.outcome === 'failed') && (queued?.has(card.record_id) ?? false);
}

/** Label pemilih sumber desain VM; id dipakai bila sumber sudah dihapus. */
export function designPickerLabel(sources: Pick<DesignSourceView, 'id' | 'name'>[], selected: string[]): string {
  if (selected.length === 0) return 'No design source';
  const names = selected.map((id) => sources.find((s) => s.id === id)?.name ?? id);
  return `Design: ${names.join(', ')}`;
}

/** Alasan tombol "Queue" untuk kartu Planned dinonaktifkan; `null` bila boleh (R11). */
export function queueForVmBlocker(q: { target: string | null; follow: string[]; designSources: string[] }): string | null {
  if (!q.target) return 'Choose a repo on the VM';
  if (q.follow.length > 0 && q.designSources.length === 0) return 'Some AC follow the design. Pick a design source on the VM.';
  return null;
}

/** Teks progres "Approve and queue for VM". */
export function queueStepLabel(step: 'pushing' | 'queuing' | 'handing_over'): string {
  switch (step) {
    case 'pushing':
      return 'Pushing the branch…';
    case 'queuing':
      return 'Adding to the VM queue…';
    case 'handing_over':
      return 'Handing over the card…';
  }
}

/**
 * Aksi baris antrean: `pending` = klaim belum pindah (Retry atau Remove),
 * `planned` = VM memegang kartu Planned (Take over, bukan Remove), `no_plan`
 * = kartu biasa (Remove).
 */
export function queueRowMode(item: Pick<QueueItemView, 'planned_state'>): 'pending' | 'planned' | 'no_plan' {
  if (item.planned_state === 'handover_pending') return 'pending';
  return item.planned_state ? 'planned' : 'no_plan';
}

/** Tombol "Approve & queue for VM" tampil (R1): VM siap dan run bukan run grup. */
export function showQueueForVm(vm: { statusLoaded: boolean; vmRole: boolean; larkReady: boolean }, grouped: boolean): boolean {
  return vm.statusLoaded && vm.vmRole && vm.larkReady && !grouped;
}
