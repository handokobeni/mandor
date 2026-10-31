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

/**
 * Baris pertama sebuah run di laporan; satu run grup punya satu baris per
 * kartu (KTD-G16), dan aksi run (Send to review, Take over, jawaban) hanya
 * tampil sekali.
 */
export function leadRow<C extends { run_id: string }>(c: C, list: C[]): boolean {
  return list.find((x) => x.run_id === c.run_id) === c;
}

type HeldRow = { run_id: string; outcome: string; hold_reason: string | null; changed_in_lark: boolean };

/**
 * Aksi untuk run tertahan di laporan: `answer` (jawab agent atau antre lagi,
 * termasuk grup yang kartu berikutnya dikeluarkan), `confirm` (Confirm and
 * push), atau `null`. Selama kartu grup menunggu Keep/Revert (R13), keputusan
 * ada di baris kartunya.
 */
export function heldAction<C extends HeldRow>(c: C, list: C[]): 'answer' | 'confirm' | null {
  if (c.outcome !== 'held') return null;
  if (list.some((x) => x.run_id === c.run_id && x.changed_in_lark)) return null;
  if (c.hold_reason === 'stuck' || c.hold_reason === 'usage_limit' || c.hold_reason === 'card_removed') return 'answer';
  return c.hold_reason === 'needs_confirmation' ? 'confirm' : null;
}

const GROUP_CARD_LABEL: Record<string, string> = {
  committed: 'Committed',
  active: 'In progress',
  pending: 'Not started',
  removed: 'Removed'
};

/** Keadaan kartu di grupnya pada laporan pagi. */
export function groupCardLabel(state: string): string {
  return GROUP_CARD_LABEL[state] ?? state;
}

const HOLD_LABEL: Record<string, string> = {
  stuck: 'Agent needs an answer',
  ac_gate: 'AC gate',
  needs_confirmation: 'Sensitive files changed',
  usage_limit: 'Usage limit reached',
  card_removed: 'Card changed in Lark',
  window_ended: 'Paused at the end of the window',
  yield_to_fix: 'Paused for a Waiting Fix',
  plan_outdated: 'Group plan outdated'
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
export function queueForVmBlocker(q: {
  target: string | null;
  follow: string[];
  designSources: string[];
  laptopRepos: string[];
  targetRepos: string[];
}): string | null {
  if (!q.target) return 'Choose a repo on the VM';
  if (!sameRepos(q.laptopRepos, q.targetRepos)) {
    return `This plan covers ${q.laptopRepos.join(', ')}. Pick a VM target with the same repos.`;
  }
  if (q.follow.length > 0 && q.designSources.length === 0) return 'Some AC follow the design. Pick a design source on the VM.';
  return null;
}

function sameRepos(a: string[], b: string[]): boolean {
  const set = (v: string[]) => [...new Set(v)].sort().join('\n');
  return set(a) === set(b);
}

interface VmTargets {
  repos: { id: string }[];
  workspaces: { id: string; repo_ids: string[] }[];
}

/**
 * Target VM bawaan untuk run laptop: repo yang sama, atau workspace VM yang
 * memuat semua repo run workspace (id workspace boleh berbeda). `''` bila
 * tidak ada yang cocok, agar dev memilih sendiri.
 */
export function defaultVmTarget(laptopRepos: string[], workspaceId: string | null, vm: VmTargets): string {
  if (laptopRepos.length > 1 || workspaceId) {
    const fits = vm.workspaces.filter((w) => laptopRepos.every((r) => w.repo_ids.includes(r)));
    const ws = fits.find((w) => w.id === workspaceId) ?? fits[0];
    return ws ? `ws:${ws.id}` : '';
  }
  const repo = laptopRepos[0];
  return repo && vm.repos.some((r) => r.id === repo) ? `repo:${repo}` : '';
}

/** Repo yang dikerjakan target VM: anggota workspace yang juga ada di run laptop. */
export function vmTargetRepos(value: string, laptopRepos: string[], workspaces: VmTargets['workspaces']): string[] {
  if (value.startsWith('repo:')) return [value.slice(5)];
  const ws = workspaces.find((w) => `ws:${w.id}` === value);
  return ws ? ws.repo_ids.filter((r) => laptopRepos.includes(r)) : [];
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

/** Teks baris antrean grup (KTD-G10): jumlah kartu dan urutan sesudah kartu pertama. */
export function queueGroupText(
  item: Pick<QueueItemView, 'record_id' | 'group' | 'group_keys'>
): { count: number; then: string } | null {
  const keys = item.group_keys ?? [];
  if (keys.length < 2) return null;
  return { count: keys.length, then: `then ${keys.slice(1).join(', ')}` };
}

/** Label tombol tambah di tab Overnight; lebih dari satu kartu = "Queue together" (R20). */
export function queueAddLabel(cards: number): string {
  return cards > 1 ? `Queue ${cards} cards together` : 'Add to queue';
}

/**
 * Tombol "Approve & queue for VM" tampil (R1): VM siap, dan run kartu
 * tunggal atau run grup yang kartu pertamanya masih di-plan (R24, KTD-G14).
 */
export function showQueueForVm(
  vm: { statusLoaded: boolean; vmRole: boolean; larkReady: boolean },
  cards: { state: string }[]
): boolean {
  const firstPlanning = cards.every((c) => c.state === 'active' || c.state === 'pending');
  return vm.statusLoaded && vm.vmRole && vm.larkReady && firstPlanning;
}

/**
 * Workspace laptop untuk Take over run workspace VM: yang memuat semua repo
 * run (id boleh berbeda dengan VM).
 *
 * @throws Error bila tidak ada, dengan langkah perbaikannya.
 */
export function laptopWorkspaceFor<W extends { id: string; repo_ids: string[] }>(repoIds: string[], workspaces: W[]): W {
  const ws = workspaces.find((w) => repoIds.every((r) => w.repo_ids.includes(r)));
  if (!ws) throw new Error(`Add a workspace on this laptop with ${repoIds.join(', ')}, then take over again.`);
  return ws;
}

/** Kartu laporan yang bisa di-Take over ke laptop: draft, tertahan, gagal, atau sudah masuk Waiting Fix. */
export function canTakeOverReport(c: { outcome: string; entered_waiting_fix: boolean }): boolean {
  return c.outcome === 'draft' || c.outcome === 'held' || c.outcome === 'failed' || c.entered_waiting_fix;
}
