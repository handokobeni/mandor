// Data AC review tiruan untuk `/?mock` (Rilis 2). Skenario dipilih per run, atau
// dipaksa lewat `?ac=<nama>`: design, current, blocked, confirm, warn, stale,
// reviewing, failed, tracker, pass, no_ac, no_source.

import type { AcLedgerView } from '#lib/protocol/AcLedgerView.ts';
import type { AcRowView } from '#lib/protocol/AcRowView.ts';
import type { ReviewView } from '#lib/protocol/ReviewView.ts';

const TEXT: [string, string][] = [
  ['AC-1', 'The invoice list can be filtered by status (Draft, Sent, Paid, Overdue).'],
  ['AC-2', 'A date range filter limits invoices by issue date, inclusive of both ends.'],
  ['AC-3', 'Clearing all filters restores the full list without a page reload.'],
  ['AC-4', 'Overdue invoices show a red Overdue badge with the number of days.'],
  ['AC-5', 'Filters are kept in the URL so the view can be shared.']
];

function row(i: number, verdict: string, extra: Partial<AcRowView> = {}): AcRowView {
  const [lbl, text] = TEXT[i];
  return {
    ac_id: `ac${i + 1}`,
    label: lbl,
    text,
    design: 'not_checked',
    design_note: null,
    verdict,
    evidence: null,
    stale: false,
    status: 'active',
    reason: null,
    decided_by: null,
    in_gate: true,
    selectable: verdict === 'fail',
    ...extra
  };
}

const DESIGNED: Partial<AcRowView>[] = [
  { design: 'in_design', design_note: 'Invoices › Filters panel, status chips' },
  { design: 'partly', design_note: 'Date picker shown; inclusive end date not specified' },
  { design: 'in_design', design_note: 'Filters panel › Clear all' },
  { design: 'conflicts', design_note: 'Design shows Overdue as an amber dot, not a red badge' },
  { design: 'missing', design_note: 'URL state is not covered by the design' }
];

function designed(i: number, verdict: string, extra: Partial<AcRowView> = {}): AcRowView {
  return row(i, verdict, { ...DESIGNED[i], ...extra });
}

const EVIDENCE: Record<string, string> = {
  pass1: 'pegasus-web/src/routes/invoices/Filters.svelte:48 renders the status chips; filters.test.ts covers each status (4 passed).',
  pass2: 'pegasus-web/src/lib/api/invoices.ts:31 sends from and to; invoices.test.ts "inclusive range" passed.',
  fail3: 'Clear all resets the chips but keeps the date range (pegasus-web/src/routes/invoices/Filters.svelte:97).',
  cant4: "Can't verify: the app isn't running, so the badge colour could not be checked.",
  pass5: '1. Open /invoices 2. Pick Paid 3. Copy the URL into a new tab: the Paid filter is kept.'
};

function current(): AcRowView[] {
  return [
    designed(0, 'pass', { evidence: EVIDENCE.pass1 }),
    designed(1, 'pass', { evidence: EVIDENCE.pass2 }),
    designed(2, 'fail', { evidence: EVIDENCE.fail3 }),
    designed(3, 'cant_verify', { evidence: EVIDENCE.cant4 }),
    designed(4, 'pass', { evidence: EVIDENCE.pass5 })
  ];
}

export function scenarioRows(name: string): AcRowView[] {
  switch (name) {
    case 'no_ac':
      return [];
    case 'design':
      return TEXT.map((_, i) => designed(i, 'not_reviewed'));
    case 'no_source':
      return current().map((r) => ({ ...r, design: 'not_checked', design_note: null }));
    case 'confirm':
      return current().map((r) => (r.verdict === 'fail' ? { ...r, verdict: 'pass', selectable: false } : r));
    case 'pass':
      return current().map((r) => ({ ...r, verdict: 'pass', selectable: false }));
    case 'stale':
      return current().map((r) => ({ ...r, stale: true }));
    case 'reviewing':
      return current().map((r, i) => (i < 2 ? r : { ...r, verdict: 'not_reviewed', evidence: null, selectable: false }));
    case 'failed':
      return current().map((r, i) => (i < 3 ? r : { ...r, verdict: 'not_reviewed', evidence: null, selectable: false }));
    case 'tracker':
      return [
        ...current().slice(0, 3),
        designed(3, 'not_reviewed', { status: 'changed', evidence: null }),
        row(4, 'not_reviewed', { status: 'new' }),
        row(4, 'pass', { ac_id: 'ac9', label: 'AC-9', text: 'Old: show totals per currency.', status: 'removed', in_gate: false })
      ];
    case 'follow':
      return current().map((r, i) =>
        i === 3 ? { ...r, design: 'follow_design', verdict: 'not_reviewed', in_gate: false, selectable: false } : r
      );
    default:
      return current();
  }
}

/** Skenario bawaan per run mock; `?ac=` menimpanya. */
const BY_RUN: Record<string, string> = { '1': 'current', '2': 'design', '5': 'pass', '8': 'design', '9': 'tracker' };

export function scenarioFor(runId: string): string {
  const forced = typeof location !== 'undefined' ? new URLSearchParams(location.search).get('ac') : null;
  return forced ?? BY_RUN[runId] ?? 'no_ac';
}

export function ledger(runId: string, rows: AcRowView[], scenario: string): AcLedgerView {
  return {
    run_id: runId,
    rows,
    synced_at_ms: Date.now() - 4 * 60_000,
    sync_error: scenario === 'sync' ? 'Lark returned 503' : null,
    design_sources: scenario === 'no_source' ? [] : ['pegasus-mock'],
    gate_mode: scenario === 'warn' ? 'warn' : 'block'
  };
}

export function history(runId: string, scenario: string): ReviewView[] {
  if (scenario === 'design' || scenario === 'no_ac' || scenario === 'no_source') return [];
  const verdicts = current().map((r) => ({ ac_id: r.ac_id, label: r.label, verdict: r.verdict, evidence: r.evidence }));
  const base: ReviewView = {
    id: `rv-${runId}-1`,
    run_id: runId,
    trigger: 'manual',
    status: 'finished',
    browser_used: false,
    browser_reason: "The app isn't running",
    error: null,
    cost_usd: 0.42,
    input_tokens: 84_000,
    output_tokens: 3_100,
    stale: false,
    skill: null,
    started_at_ms: Date.now() - 35 * 60_000,
    finished_at_ms: Date.now() - 31 * 60_000,
    verdicts
  };
  if (scenario === 'failed') {
    return [
      {
        ...base,
        id: `rv-${runId}-2`,
        trigger: 'push_and_pr',
        status: 'failed',
        error: 'Review stopped at the cost limit',
        started_at_ms: Date.now() - 6 * 60_000,
        verdicts: verdicts.map((v, i) => (i < 3 ? v : { ...v, verdict: 'not_reviewed', evidence: null }))
      },
      base
    ];
  }
  return [base];
}
