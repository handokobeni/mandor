// Logika papan murni: status run dan flag, kolom, filter, aksi kartu, dan panel agent.

import type { CardView } from '#lib/protocol/CardView.ts';
import type { FlagView } from '#lib/protocol/FlagView.ts';
import type { RunView } from '#lib/protocol/RunView.ts';
import type { SprintView } from '#lib/protocol/SprintView.ts';

export const COLUMNS = [
  { id: 'todo', label: 'Todo' },
  { id: 'in_progress', label: 'In Progress' },
  { id: 'review', label: 'Review' },
  { id: 'done', label: 'Done' }
] as const;

export const UNKNOWN_COLUMN = { id: 'unknown', label: 'Unknown' } as const;

export type ColumnId = (typeof COLUMNS)[number]['id'] | 'unknown';

/** Tone menentukan token warna `--status-*`; warna tidak pernah tampil sendirian. */
export type Tone = 'working' | 'attention' | 'waiting' | 'failed' | 'warning' | 'ok' | 'muted';

export interface StateMeta {
  label: string;
  tone: Tone;
  spin: boolean;
}

const RUN_STATES: Record<string, StateMeta> = {
  preparing: { label: 'Preparing', tone: 'working', spin: false },
  running: { label: 'Working', tone: 'working', spin: true },
  awaiting_dev: { label: 'Waiting for you', tone: 'attention', spin: false },
  publishing: { label: 'Creating PR', tone: 'working', spin: false },
  in_review_loop: { label: 'Done, waiting for review', tone: 'waiting', spin: false },
  wf_awaiting_comment: { label: 'Waiting for reviewer comments', tone: 'waiting', spin: false },
  wf_quiet: { label: 'Quiet period', tone: 'waiting', spin: false },
  wf_running: { label: 'Fixing', tone: 'working', spin: false },
  reviewing: { label: 'Reviewing AC', tone: 'working', spin: true },
  failed: { label: 'Failed', tone: 'failed', spin: false },
  orphaned: { label: 'Parked: claim lost', tone: 'warning', spin: false },
  stopped: { label: 'Stopped', tone: 'muted', spin: false }
};

export function runMeta(state: string): StateMeta {
  return RUN_STATES[state] ?? { label: state, tone: 'muted', spin: false };
}

/** Label run di fase brainstorm atau plan; `undefined` = label bawaan state. */
export function runLabel(run: Pick<RunView, 'state' | 'phase'>): string | undefined {
  const labels: Record<string, [string, string]> = {
    brainstorm: ['Brainstorming', 'Question for you'],
    plan: ['Planning', 'Plan ready']
  };
  const pair = labels[run.phase];
  if (!pair) return undefined;
  if (run.state === 'running') return pair[0];
  if (run.state === 'awaiting_dev') return pair[1];
  return undefined;
}

export function stateLabel(state: string): string {
  return runMeta(state).label;
}

export interface FlagMeta {
  label: string;
  tone: 'failed' | 'warning';
  action: 'retry' | 'close' | null;
}

const FLAGS: Record<string, FlagMeta> = {
  overwritten: { label: 'Overwritten by Lark', tone: 'warning', action: null },
  sync_failed: { label: 'Sync failed', tone: 'failed', action: 'retry' },
  deleted: { label: 'Deleted in Lark', tone: 'failed', action: 'close' },
  unassigned: { label: 'No longer assigned', tone: 'warning', action: 'close' }
};

export function flagMeta(kind: string): FlagMeta {
  return FLAGS[kind] ?? { label: kind, tone: 'warning', action: null };
}

/** State saat agent sedang bekerja; pesan dev diantrekan. */
export const WORKING_STATES = ['running', 'preparing', 'wf_running', 'reviewing'];

export function isWorking(state: string): boolean {
  return WORKING_STATES.includes(state);
}

export interface BoardFilter {
  sprint: string | null;
  query: string;
  /**
   * "Mine": kartu yang assignee-nya hanya saya, ditambah yang sedang saya kerjakan
   * (klaim saya atau run di mesin ini). Kartu yang diklaim dev lain dan kartu rekan
   * (papan Team) disembunyikan. Selain itu papan Team: semua kartu.
   */
  mineOnly: boolean;
}

function isMine(c: CardView): boolean {
  if (c.run || c.mine) return true;
  if (c.owner) return false;
  return c.assignees.length <= 1;
}

export function filterCards(cards: CardView[], f: BoardFilter): CardView[] {
  const q = f.query.trim().toLowerCase();
  return cards.filter((c) => {
    if (f.sprint && !c.sprint_ids.includes(f.sprint)) return false;
    if (f.mineOnly && (c.team || !isMine(c))) return false;
    if (q && !`${c.key ?? ''} ${c.title}`.toLowerCase().includes(q)) return false;
    return true;
  });
}

const CARD_RANK: Record<string, number> = {
  awaiting_dev: 0,
  failed: 1,
  orphaned: 2,
  stopped: 2,
  running: 3,
  preparing: 3,
  publishing: 3,
  wf_running: 3,
  wf_quiet: 4,
  wf_awaiting_comment: 5,
  in_review_loop: 6
};

function rank(c: CardView): number {
  if (c.column === 'todo' && !c.run) return c.start_blocked ? 1 : 0;
  if (c.flag) return 1.5;
  return c.run ? (CARD_RANK[c.run.state] ?? 7) : 7;
}

/** Yang butuh dev di atas; di Todo kartu yang bisa dimulai di atas. Urutan stabil. */
export function orderCards(cards: CardView[]): CardView[] {
  return [...cards].sort((a, b) => rank(a) - rank(b));
}

/** Kartu per kolom (sudah diurutkan); kolom Unknown hanya muncul bila ada isinya. */
export function groupByColumn(cards: CardView[]): { id: ColumnId; label: string; cards: CardView[] }[] {
  const known = new Set<string>(COLUMNS.map((c) => c.id));
  const out: { id: ColumnId; label: string; cards: CardView[] }[] = COLUMNS.map((col) => ({
    id: col.id,
    label: col.label,
    cards: orderCards(cards.filter((c) => c.column === col.id))
  }));
  const unknown = cards.filter((c) => !known.has(c.column));
  if (unknown.length > 0) out.push({ ...UNKNOWN_COLUMN, cards: orderCards(unknown) });
  return out;
}

/** Semua sprint yang muncul di kartu, untuk filter. */
export function sprintsOf(cards: CardView[]): string[] {
  return [...new Set(cards.flatMap((c) => c.sprint_ids))].sort();
}

/** Kartu milik dev lain yang klaimnya masih aktif hanya bisa dilihat. */
export function isReadOnly(card: CardView): boolean {
  if (card.team) return true;
  return !!card.owner && !card.mine && card.owner_active && !card.run;
}

/** Nama PIC (assignee) kartu; `null` bila kartu tanpa assignee. */
export function assigneeLabel(card: CardView, nameOf: (openId: string) => string = (id) => id): string | null {
  if (card.assignees.length === 0) return null;
  return card.assignees.map(nameOf).join(', ');
}

/** Gerbang readiness (domain `gate`) lolos; alasan blokir lain seperti klaim tidak dihitung. */
export function isReady(card: CardView): boolean {
  return !card.start_blocked?.startsWith('task is not ready');
}

export function ownerLabel(card: CardView, nameOf: (openId: string) => string = (id) => id): string | null {
  if (!card.owner || card.mine) return null;
  return `${nameOf(card.owner)} · ${card.owner_active ? 'active' : 'inactive'}`;
}

export type CardAction = 'open' | 'takeover' | 'start';

/** Aksi utama kartu; Start tetap ditampilkan (nonaktif) bila `start_blocked`. */
export function cardAction(card: CardView): CardAction | null {
  if (card.run) return 'open';
  if (card.can_takeover) return 'takeover';
  if (card.column === 'todo' && !isReadOnly(card)) return 'start';
  return null;
}

/** Penjelasan singkat di kartu: kenapa tidak bisa Start, status tanpa kolom, atau error run. */
export function cardReason(card: CardView): string | null {
  // Kartu rekan sudah ditandai hanya baca dan menampilkan assignee-nya.
  if (card.team) return null;
  if (card.column === 'todo' && card.start_blocked && !card.run) {
    return `Can't start: ${card.start_blocked.replace(/^task is not ready: /, 'not ready, ')}`;
  }
  if (card.column === 'unknown') {
    return `Lark status "${card.status}" has no column. Change it in Lark to move this card.`;
  }
  if (card.run && (card.run.state === 'failed' || card.run.state === 'orphaned')) {
    return card.run.last_error;
  }
  return null;
}

/** Run yang menunggu jawaban di atas, lalu yang gagal, lalu sisanya. */
export function sortRuns(runs: RunView[]): RunView[] {
  const order = (r: RunView) => CARD_RANK[r.state] ?? 7;
  return [...runs].sort((a, b) => order(a) - order(b));
}

export type AgentGroupId = 'you' | 'attention' | 'working' | 'waiting';

const GROUPS: { id: AgentGroupId; label: string; match: (r: RunView, flag: FlagView | undefined) => boolean }[] = [
  { id: 'you', label: 'Waiting for you', match: (r) => r.state === 'awaiting_dev' },
  {
    id: 'attention',
    label: 'Needs attention',
    match: (r, flag) => !!flag || ['failed', 'orphaned', 'stopped'].includes(r.state)
  },
  { id: 'working', label: 'Working', match: (r) => isWorking(r.state) || r.state === 'publishing' },
  { id: 'waiting', label: 'Waiting', match: () => true }
];

/** Kelompok panel agent; kelompok kosong tidak ditampilkan. */
export function agentGroups(
  runs: RunView[],
  flags: Map<string, FlagView>
): { id: AgentGroupId; label: string; runs: RunView[] }[] {
  const used = new Set<string>();
  return GROUPS.map((g) => {
    const list = runs.filter((r) => {
      if (used.has(r.id) || !g.match(r, flags.get(r.record_id))) return false;
      used.add(r.id);
      return true;
    });
    return { id: g.id, label: g.label, runs: list };
  }).filter((g) => g.runs.length > 0);
}

export function canChat(run: RunView): boolean {
  return isWorking(run.state) || run.state === 'awaiting_dev';
}

const COMPOSER_BLOCKED: Record<string, string> = {
  in_review_loop: 'The agent is idle while the PR waits for review. Feedback goes through Start fix.',
  wf_awaiting_comment: 'Use Start fix to send your instruction with the reviewer comments.',
  wf_quiet: 'Trusted reviewer comments are forwarded when the quiet period ends.',
  publishing: 'Creating the PR. You can reply after it is opened.',
  failed: 'Resume the run to continue the session.',
  orphaned: 'This run is parked. Close it, or ask the new owner to hand it back.',
  stopped: 'The agent is stopped. Close the run to clean up the worktree.'
};

/** Alasan composer nonaktif; `null` bila dev boleh mengirim pesan. */
export function composerBlocked(run: RunView): string | null {
  if (canChat(run)) return null;
  return COMPOSER_BLOCKED[run.state] ?? 'The agent cannot take messages right now.';
}

export function relTime(thenMs: number, nowMs: number): string {
  const s = Math.floor((nowMs - thenMs) / 1000);
  if (s < 1) return 'just now';
  if (s < 60) return `${s}s ago`;
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  return `${Math.floor(s / 3600)}h ago`;
}

/** Sisa jeda tenang dalam format m:ss; dibulatkan ke atas agar 0:00 berarti habis. */
export function quietLeft(untilMs: number, nowMs: number): string {
  const s = Math.max(0, Math.ceil((untilMs - nowMs) / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

export interface SprintOption {
  id: string;
  /** Nama, mis. "Sprint 42 · Oct 13 to Oct 24". */
  label: string;
  name: string;
}

function shortDate(ms: number): string {
  return new Date(ms).toLocaleDateString('en-US', { month: 'short', day: 'numeric', timeZone: 'UTC' });
}

/** Opsi filter sprint: sprint terbaru (tanggal mulai) dulu; sprint tanpa data dari Lark memakai ID-nya. */
export function sprintOptions(ids: string[], sprints: SprintView[]): SprintOption[] {
  const byId = new Map(sprints.map((s) => [s.id, s]));
  return [...new Set(ids)]
    .map((id) => {
      const s = byId.get(id);
      if (!s) return { id, name: id, label: id, start: -Infinity };
      const range = s.start_ms && s.end_ms ? ` · ${shortDate(s.start_ms)} to ${shortDate(s.end_ms)}` : '';
      return { id, name: s.name, label: `${s.name}${range}`, start: s.start_ms ?? -Infinity };
    })
    .sort((a, b) => b.start - a.start)
    .map(({ id, name, label }) => ({ id, name, label }));
}
