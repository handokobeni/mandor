// Transport tiruan untuk melihat UI di browser tanpa daemon dan Lark (`bun run dev`, buka `/?mock`).
// Hanya dimuat lewat dynamic import di mode dev; tidak masuk build produksi.

import type { DesignSourceView } from '#lib/protocol/DesignSourceView.ts';
import type { ReviewerSettingsView } from '#lib/protocol/ReviewerSettingsView.ts';
import type { CardView } from '#lib/protocol/CardView.ts';
import type { MessageView } from '#lib/protocol/MessageView.ts';
import type { ApprovalView } from '#lib/protocol/ApprovalView.ts';
import type { QuestionView } from '#lib/protocol/QuestionView.ts';
import type { RunView } from '#lib/protocol/RunView.ts';
import type { Transport } from '#lib/api/client.ts';
import type { AcRowView } from '#lib/protocol/AcRowView.ts';
import type { AppRunView } from '#lib/protocol/AppRunView.ts';
import type { LabSetParams } from '#lib/protocol/LabSetParams.ts';
import type { LabView } from '#lib/protocol/LabView.ts';
import { activeRows, gate } from '#lib/ac-review.ts';
import { background } from '#lib/stores/background.svelte.ts';
import { history, ledger, mockShot, scenarioFor, scenarioRows } from './mock-ac.ts';

/** State Run app tiruan per run. */
const APP = new Map<string, AppRunView>();
let chromeConnected = false;
function appView(runId: string, state: string): AppRunView {
  return {
    run_id: runId,
    state,
    members: [{ repo_id: 'pegasus-web', url: 'http://localhost:5173', state, prototype: null, browser_tests: true }],
    database_confirmed: false,
    login_confirmed: false,
    error: state === 'failed' ? 'pegasus-web: the app exited (code 1) before opening its port' : null
  };
}

/** Perlengkapan lab tiruan per repo (plan 2026-11-10-001). */
const LAB = new Map<string, LabView>();
function mockLab(method: string, params: Record<string, unknown>): LabView {
  const repo = String(params.repo_id);
  const cur: LabView = LAB.get(repo) ?? {
    repo_id: repo,
    equipment: null,
    approved: false,
    approved_at_ms: null,
    proposing: false,
    proposal: null,
    proposal_error: null
  };
  let next = cur;
  if (method === 'lab.propose') {
    next = {
      ...cur,
      proposing: false,
      proposal_error: null,
      proposal: {
        accounts: [
          { role: 'admin', username: 'admin@pegasus.test', default: true, has_password: false },
          { role: 'manager', username: 'manager@pegasus.test', default: false, has_password: false }
        ],
        login: {
          url: 'http://localhost:5173/login',
          username_field: '#email',
          password_field: '#password',
          submit: 'text=Sign in',
          submit_path: '/api/auth/login',
          success_kind: 'path',
          success: '/dashboard',
          authenticated_url: 'http://localhost:5173/dashboard'
        },
        safe_posts: ['/api/invoices/search'],
        fixtures_dir: 'review-fixtures'
      }
    };
  } else if (method === 'lab.set') {
    const p = params as unknown as LabSetParams;
    const has = new Set([
      ...(cur.equipment?.accounts.filter((a) => a.has_password).map((a) => a.role) ?? []),
      ...p.passwords.filter((x) => x.password).map((x) => x.role)
    ]);
    next = {
      ...cur,
      equipment: {
        ...p.equipment,
        accounts: p.equipment.accounts.map((a) => ({ ...a, has_password: has.has(a.role) }))
      },
      approved: false,
      approved_at_ms: null,
      proposal: null
    };
  } else if (method === 'lab.approve') {
    next = { ...cur, approved: true, approved_at_ms: Date.now() };
  }
  LAB.set(repo, next);
  return next;
}

/** Buku AC tiruan per run; berubah saat review atau Push & PR. */
const AC = new Map<string, AcRowView[]>();
function acRows(runId: string): AcRowView[] {
  if (!AC.has(runId)) AC.set(runId, scenarioRows(scenarioFor(runId)));
  return AC.get(runId) ?? [];
}

function run(id: string, key: string, title: string, state: string, extra: Partial<RunView> = {}): RunView {
  return {
    id,
    record_id: `rec${id}`,
    repo_id: 'pegasus-web',
    key,
    title,
    state,
    branch: `beni/${key.toLowerCase()}-${title.toLowerCase().split(' ').slice(0, 3).join('-')}`,
    pr_url: null,
    workspace_id: null,
    repos: [{ repo_id: extra.repo_id ?? 'pegasus-web', pr_url: extra.pr_url ?? null }],
    phase: 'implement',
    plan: null,
    last_error: null,
    note: null,
    queued: [],
    automatic: false,
    draft: false,
    quiet_until_ms: state === 'wf_quiet' ? Date.now() + 95_000 : null,
    started_at_ms: Date.now() - 42 * 60_000,
    updated_at_ms: Date.now() - 60_000,
    ac: null,
    cards: [],
    one_plan: false,
    pending_approvals: 0,
    pending_question: null,
    ...extra
  };
}

const RUNS: RunView[] = [
  run('1', 'PGS-1402', 'Filter invoices by status and date range', 'awaiting_dev', {
    ac: { total: 5, ok: 3, failed: 1, unverified: 1, not_reviewed: 0, stale: false, design_gaps: 1 },
    pending_approvals: 1
  }),
  run('2', 'PGS-1388', 'Map cluster markers for branch offices', 'running', { queued: ['Also run the e2e tests'] }),
  run('3', 'PGS-1379', 'Export payroll report to XLSX', 'failed', {
    last_error: 'claude exited with code 1: session limit reached'
  }),
  run('4', 'PGS-1371', 'Policy limit error message on claims form', 'wf_quiet'),
  run('5', 'PGS-1365', 'Add audit log to approval flow', 'in_review_loop', {
    ac: { total: 5, ok: 5, failed: 0, unverified: 0, not_reviewed: 0, stale: false, design_gaps: 0 },
    pr_url: 'https://github.com/pegasus/pegasus-web/pull/482'
  }),
  run('6', 'PGS-1360', 'Retry webhook delivery with backoff', 'orphaned', {
    last_error: 'Another developer claimed this task in Lark at 09:41.'
  }),
  run('7', 'PGS-1355', 'Rename leave balance fields', 'wf_awaiting_comment'),
  run('8', 'PGS-1350', 'Cache employee search results', 'awaiting_dev', {
    phase: 'plan',
    pending_question: 'q-mock1',
    plan: [
      '# PGS-1350: Cache employee search results',
      '',
      '## Approach',
      '',
      'Most of the search flow already exists. Add a **per-tenant cache** for 60 seconds in `searchEmployees` and clear it on employee edits.',
      '',
      '## Files to change',
      '',
      '- `pegasus-web/src/lib/api/search.ts`: wrap `searchEmployees` with the cache',
      '- `pegasus-web/src/lib/api/search.test.ts`: cover hits, expiry, and invalidation',
      '',
      '## Acceptance criteria',
      '',
      '| AC | How it is met | Test |',
      '| --- | --- | --- |',
      '| AC-1 repeated search is instant | cache hit within 60 s | `search.test.ts` hit case |',
      '| AC-2 edits show up | clear the tenant cache on save | invalidation case |',
      '| AC-3 no cross-tenant data | key includes `tenantId` | two-tenant case |',
      '',
      '## Steps',
      '',
      '- [ ] Add `createCache(ttlMs)` helper',
      '- [ ] Use it in `searchEmployees`',
      '- [ ] Clear on `employee.updated`',
      '- [ ] Tests',
      '',
      '```ts',
      'const cache = createCache<Employee[]>(60_000);',
      '```',
      '',
      '## Risks',
      '',
      '> Stale results for up to 60 seconds when another user edits.',
      '',
      '## Open questions',
      '',
      '1. Should the cache clear on employee edits from other tabs?'
    ].join('\n')
  }),
  // Run workspace multi-repo: satu PR per repo yang berubah.
  run('9', 'PGS-1344', 'Show payroll cut-off on web and API', 'in_review_loop', {
    repo_id: 'pegasus-api',
    workspace_id: 'pegasus',
    ac: { total: 5, ok: 3, failed: 0, unverified: 0, not_reviewed: 2, stale: true, design_gaps: 0 },
    pr_url: 'https://github.com/pegasus/pegasus-api/pull/77\nhttps://github.com/pegasus/pegasus-web/pull/483',
    repos: [
      { repo_id: 'pegasus-api', pr_url: 'https://github.com/pegasus/pegasus-api/pull/77' },
      { repo_id: 'pegasus-web', pr_url: 'https://github.com/pegasus/pegasus-web/pull/483' }
    ]
  }),
  run('10', 'PGS-1347', 'Overtime approval reminders', 'awaiting_dev', { phase: 'brainstorm' }),
  // Run grup satu flow (plan run grup): kartu 2 dari 4 baru selesai gilirannya.
  run('11', 'PGS-1401', 'Job list', 'awaiting_dev', {
    branch: 'mandor/pgs-1401-1404-job-position',
    cards: [
      { record_id: 'rec11', key: 'PGS-1401', title: 'Job list', state: 'done', changed_in_lark: false, kept_reason: null, in_fix: false },
      { record_id: 'recG2', key: 'PGS-1402', title: 'Job card', state: 'active', changed_in_lark: false, kept_reason: null, in_fix: false },
      { record_id: 'recG3', key: 'PGS-1403', title: 'Create job', state: 'pending', changed_in_lark: false, kept_reason: null, in_fix: false },
      { record_id: 'recG4', key: 'PGS-1404', title: 'Position mapping', state: 'pending', changed_in_lark: false, kept_reason: null, in_fix: false }
    ]
  })
];

const GROUP = RUNS[10];
const GROUP_REASON = 'In group PGS-1401–PGS-1404';

// Sumber desain dan setelan reviewer tiruan (Rilis 2 U14).
let designSources: DesignSourceView[] = [
  { id: 'ds1', name: 'pegasus-mock', kind: 'folder', path: '/home/beni/work/pegasus-mock', ready: true, status: 'Ready', app: null },
  { id: 'ds2', name: 'old-figma-export', kind: 'folder', path: '/home/beni/old/figma', ready: false, status: 'Path missing', app: null }
];
let reviewer: ReviewerSettingsView = { model: null, budget_cents: 200, timeout_secs: 900 };

function card(r: RunView | null, extra: Partial<CardView>): CardView {
  return {
    record_id: r?.record_id ?? `rec${Math.random()}`,
    key: r?.key ?? null,
    title: r?.title ?? '',
    status: 'In Progress',
    column: 'in_progress',
    readiness: 'Ready To Dev',
    start_blocked: null,
    sprint_ids: ['spr42'],
    assignees: ['ou_7f3a91'],
    team: false,
    owner: r ? 'ou_7f3a91' : null,
    owner_active: true,
    mine: !!r,
    can_takeover: false,
    flag: null,
    run: r,
    holder: null,
    pr_url: r?.pr_url ?? null,
    branch: r?.branch ?? null,
    repo_id: r?.repo_id ?? null,
    last_modified_ms: 0,
    ...extra
  };
}

const CARDS: CardView[] = [
  card(null, {
    record_id: 'recA',
    key: 'PGS-1410',
    title: 'Bulk approve overtime requests',
    status: 'To Do',
    column: 'todo',
    assignees: ['ou_7f3a91', 'ou_9e07b2']
  }),
  card(null, {
    record_id: 'recB',
    key: 'PGS-1411',
    title: 'Show remaining leave on the dashboard card',
    status: 'To Do',
    column: 'todo',
    readiness: 'Review AC By PM',
    start_blocked: 'task is not ready: Development Readiness "Review AC By PM"'
  }),
  card(null, {
    record_id: 'recC',
    key: 'PGS-1398',
    title: 'Payslip PDF header uses the new logo',
    status: 'To Do',
    column: 'todo',
    owner: 'ou_9e07b2',
    owner_active: true,
    start_blocked: 'claimed by ou_9e07b2'
  }),
  card(RUNS[0], {}),
  card(RUNS[1], { flag: { kind: 'sync_failed', detail: 'Lark returned 503 while moving the status.' } }),
  card(RUNS[2], {}),
  card(RUNS[3], { status: 'Waiting Fix' }),
  card(RUNS[5], {}),
  card(RUNS[6], { status: 'Waiting Fix' }),
  card(RUNS[7], {}),
  card(null, {
    record_id: 'recD',
    key: 'PGS-1390',
    title: 'Timezone for attendance clock-in',
    owner: 'ou_2c81d4',
    owner_active: false,
    can_takeover: true
  }),
  card(RUNS[4], { status: 'Waiting Review', column: 'review' }),
  card(RUNS[8], { status: 'Waiting Review', column: 'review', pr_url: RUNS[8].pr_url }),
  card(RUNS[9], {}),
  ...GROUP.cards.map((g) =>
    card(GROUP, { record_id: g.record_id, key: g.key, title: g.title, start_blocked: GROUP_REASON })
  ),
  card(null, { record_id: 'recE', key: 'PGS-1340', title: 'Sort employees by join date', status: 'Delivered', column: 'done' }),
  card(null, {
    record_id: 'recV3',
    key: 'PGS-1423',
    title: 'Reminder email for pending approvals',
    owner: 'ou_7f3a91',
    mine: true,
    holder: 'own_other_daemon'
  }),
  card(null, {
    record_id: 'recT1',
    key: 'PGS-1468',
    title: 'Integro: email notification for new users',
    status: 'In Review',
    column: 'review',
    team: true,
    assignees: ['ou_2c81d4'],
    owner: null,
    start_blocked: 'Assigned to another developer'
  }),
  card(null, {
    record_id: 'recT2',
    key: 'PGS-1422',
    title: 'Base Management: show the source of spreadsheet data',
    status: 'To Do',
    column: 'todo',
    team: true,
    assignees: ['ou_9e07b2', 'ou_51aa6c'],
    owner: null,
    start_blocked: 'Assigned to another developer'
  }),
  card(null, { record_id: 'recF', key: 'PGS-1337', title: 'Blocked by vendor API', status: 'Blocked', column: 'unknown' })
];

let nextMsg = 100;
const MESSAGES: Record<string, MessageView[]> = {
  '1': [
    { id: 1, role: 'dev', body: { text: 'Start with the API filter, then the URL params.' }, created_at_ms: 0 },
    {
      id: 2,
      role: 'agent',
      body: { type: 'assistant_message', text: 'I will add `status` and a date range to `listInvoices`, then read them from the URL.' },
      created_at_ms: 0
    },
    { id: 3, role: 'agent', body: { type: 'tool_use', id: 't1', name: 'Read', preview: 'src/lib/api/invoice.ts' }, created_at_ms: 0 },
    { id: 4, role: 'agent', body: { type: 'tool_use', id: 't2', name: 'Edit', preview: 'src/lib/api/invoice.ts' }, created_at_ms: 0 },
    { id: 5, role: 'agent', body: { type: 'tool_use', id: 't3', name: 'Bash', preview: 'git push origin HEAD' }, created_at_ms: 0 },
    { id: 6, role: 'agent', body: { type: 'tool_result', tool_use_id: 't1', is_error: false, preview: 'export async function listInvoices() {…' }, created_at_ms: 0 },
    { id: 7, role: 'agent', body: { type: 'tool_result', tool_use_id: 't2', is_error: false, preview: '+7 −2' }, created_at_ms: 0 },
    { id: 8, role: 'agent', body: { type: 'permission_denied', tool_use_id: 't3', reason: 'mandor: git push is run by the daemon, not the agent.' }, created_at_ms: 0 },
    {
      id: 9,
      role: 'agent',
      body: { type: 'assistant_message', text: 'Done. The filter works and the tests pass:\n\n- `pnpm test invoice` ✔\n- `pnpm lint` ✔' },
      created_at_ms: 0
    },
    { id: 10, role: 'agent', body: { type: 'done', is_error: false, result: 'ok' }, created_at_ms: 0 }
  ],
  // Giliran plan: chat tetap berisi aktivitas walau run terbuka di tab Plan.
  '8': [
    { id: 1, role: 'dev', body: { type: 'text', text: 'Started: plan with Mandor built-in.' }, created_at_ms: 0 },
    { id: 2, role: 'agent', body: { type: 'tool_use', id: 'p1', name: 'Read', preview: 'src/lib/api/search.ts' }, created_at_ms: 0 },
    { id: 3, role: 'agent', body: { type: 'tool_result', tool_use_id: 'p1', is_error: false, preview: 'export async function searchEmployees(…' }, created_at_ms: 0 },
    {
      id: 4,
      role: 'agent',
      body: {
        type: 'assistant_message',
        text: [
          '# PGS-1350 — Cache employee search results',
          '',
          '## Pendekatan',
          '',
          'Tambahkan cache per tenant selama 60 detik di `searchEmployees`.',
          '',
          '- **`src/lib/api/search.ts`** — bungkus `searchEmployees` dengan cache',
          '- **`src/lib/api/search.test.ts`** — uji hit, kedaluwarsa, dan invalidasi',
          '',
          '**Nilai Design per AC** sudah dicatat lewat `ac_set_design`:',
          '',
          '| AC | Design | Catatan |',
          '| --- | --- | --- |',
          '| AC-8807 (AC-1) | `in_design` | Frame 01: F(10)=55; frame 02: pesan "n harus 0 atau lebih" |',
          '| AC-8811 (AC-5) | `conflicts` | Desain memakai 3 tab (`nav.tabs`), bukan dropdown |',
          '',
          '> Plan ada di tab Plan. Balas di sini untuk merevisi.'
        ].join('\n')
      },
      created_at_ms: 0
    },
    { id: 5, role: 'agent', body: { type: 'done', is_error: false, result: 'ok' }, created_at_ms: 0 },
    // Revisi plan yang bertanya lewat kartu (plan 2026-11-07-001).
    { id: 6, role: 'dev', body: { text: 'Bagaimana dengan hasil lama?' }, created_at_ms: 0 },
    { id: 7, role: 'agent', body: { type: 'tool_use', id: 'q1', name: 'mcp__mandor__ask_developer', preview: '' }, created_at_ms: 0 },
    {
      id: 8,
      role: 'agent',
      body: { type: 'tool_result', tool_use_id: 'q1', is_error: false, preview: 'q-mock1 recorded. End your turn now; the developer answers in the chat.' },
      created_at_ms: 0
    },
    { id: 9, role: 'agent', body: { type: 'done', is_error: false, result: '' }, created_at_ms: 0 }
  ]
};

/** Kartu pertanyaan tiruan untuk run 8 (plan 2026-11-07-001). */
let mockQuestions: QuestionView[] = [
  {
    id: 'q-mock1',
    run_id: '8',
    question: 'Should cached results expire after a fixed time, or only when an employee record changes?',
    options: [
      { label: 'Fixed time (5 min)', description: 'Simple; results can be up to 5 minutes old.', recommended: true },
      { label: 'On record change', description: 'Always fresh; needs an invalidation hook in the employee API.', recommended: false },
      { label: 'Both', description: 'Expire after 5 minutes and also on change.', recommended: false }
    ],
    status: 'pending',
    answer: null,
    created_at_ms: Date.now() - 2 * 60_000,
    closed_at_ms: null
  }
];

const PATCH = `diff --git a/src/lib/api/invoice.ts b/src/lib/api/invoice.ts
--- a/src/lib/api/invoice.ts
+++ b/src/lib/api/invoice.ts
@@ -20,4 +20,9 @@ export type InvoiceFilter
 import { api } from './client';
-export async function listInvoices() {
-  const res = await api('/invoices');
+export async function listInvoices(filter: InvoiceFilter = {}) {
+  const q = new URLSearchParams(clean(filter));
+  const res = await api(\`/invoices?\${q}\`);
   return res.json();
 }`;

// `?mock&setup`: Lark belum diatur dan belum ada repo, untuk meninjau onboarding.
const setup = typeof location !== 'undefined' && location.search.includes('setup');
let configured = !setup;
let loggedIn = !setup;
const repos: unknown[] = setup
  ? []
  : [
      {
        id: 'pegasus-web',
        path: '/home/beni/work/pegasus-web',
        default_branch: 'main',
        gh_profile: null,
        companions: [],
        trusted_reviewers: ['ou_51aa6c'],
        extra_allowed: ['Bash(pnpm test *)'],
        gate_mode: null,
        app: {
          test_mode: 'web',
          setup_command: 'pnpm install',
          start_command: 'pnpm dev',
          port: 5173,
          app_url: null,
          env_keys: ['VITE_API_URL'],
          browser_tests: true,
          auto_restart: false,
          unattended_login: false
        }
      },
      {
        id: 'pegasus-api',
        path: '/home/beni/work/pegasus-api',
        default_branch: 'main',
        gh_profile: null,
        companions: [],
        trusted_reviewers: [],
        extra_allowed: ['Bash(cargo *)'],
        gate_mode: null,
        app: {
          test_mode: 'no_app',
          setup_command: null,
          start_command: null,
          port: null,
          app_url: null,
          env_keys: [],
          browser_tests: true,
          auto_restart: false,
          unattended_login: false
        }
      }
    ];
const workspaces: Record<string, unknown>[] = setup
  ? []
  : [
      {
        id: 'pegasus',
        name: 'pegasus',
        root: '/home/beni/work',
        repo_ids: ['pegasus-api', 'pegasus-web'],
        mcp_servers: ['graft'],
        browser_tests: true,
        gate_mode: 'block'
      }
    ];

/** Kartu persetujuan tiruan untuk run 1 (plan 2026-11-06-001). */
let mockApprovals: ApprovalView[] = [
  {
    id: 'ap-1',
    run_id: '1',
    turn: 'implement',
    tool: 'Bash',
    input: { command: 'docker ps -a', description: 'Cek container yang berjalan' },
    description: 'Cek container yang berjalan',
    cwd: '/home/beni/.local/share/mandor/workspaces/pgs-1402/pegasus-web',
    status: 'answered',
    answer: 'allow_once',
    created_at_ms: Date.now() - 20 * 60_000,
    deadline_ms: Date.now() - 10 * 60_000,
    closed_at_ms: Date.now() - 19 * 60_000
  },
  {
    id: 'ap-2',
    run_id: '1',
    turn: 'implement',
    tool: 'Bash',
    input: { command: 'curl -s https://api.github.com/repos/x/y \u202e && echo ok', description: 'Ambil metadata repo' },
    description: 'Ambil metadata repo',
    cwd: '/home/beni/.local/share/mandor/workspaces/pgs-1402/pegasus-web',
    status: 'pending',
    answer: null,
    created_at_ms: Date.now() - 60_000,
    deadline_ms: Date.now() + 9 * 60_000,
    closed_at_ms: null
  }
];

function larkStatus(): unknown {
  return { configured, logged_in: loggedIn, open_id: loggedIn ? 'ou_7f3a91' : null, name: loggedIn ? 'Beni Handoko' : null, reauth_required: false };
}

/** Run kartu selesai tiruan (plan 2026-11-03-001). */
let mockFinished: { run_id: string; keys: string[]; title: string; bytes: number | null }[] = [
  { run_id: 'm-f1', keys: ['PGS-1558', 'PGS-1559', 'PGS-1560'], title: 'Kalkulator diskon', bytes: 412_000 },
  { run_id: 'm-f2', keys: ['PGS-1563'], title: 'Daftar tamu', bytes: 180_000 },
  { run_id: 'm-f3', keys: ['PGS-1366'], title: 'Export payroll report', bytes: 640_000_000 }
];
let mockAutoClose = false;

function handle(method: string, params: Record<string, unknown>): unknown {
  switch (method) {
    case 'hello':
      return { daemon_version: '0.1.0-mock', protocol: '1.1' };
    case 'status':
      return {
        version: '0.1.0-mock',
        uptime_ms: 1000,
        repos: 1,
        lark: larkStatus(),
        busy: true,
        managed_by: 'systemd',
        draining: false,
        keep_awake: 'held'
      };
    case 'lark.settings.get':
      return {
        app_id: 'cli_a6f2c81d93b8d00e',
        has_secret: true,
        app_token: 'NkQ2bXr7YcPq1sZt9vLmHg4eAbc',
        table_id: 'tbl8kX2pQm4vR7aW',
        ac_table_id: 'tblD3nF6hJ9kL2pS',
        redirect_port: 53682,
        open_base: 'https://open.larksuite.com',
        accounts_base: 'https://accounts.larksuite.com',
        quiet_secs: 120
      };
    case 'repos.list':
      return repos;
    case 'repos.add':
      repos.push({
        id: 'pegasus-web',
        path: String(params.path),
        default_branch: 'main',
        gh_profile: null,
        companions: [],
        trusted_reviewers: [],
        extra_allowed: [],
        gate_mode: null,
        app: {
          test_mode: 'no_app',
          setup_command: null,
          start_command: null,
          port: null,
          app_url: null,
          env_keys: [],
          browser_tests: true,
          auto_restart: false,
          unattended_login: false
        }
      });
      return repos.at(-1);
    case 'lark.settings.set':
      configured = true;
      return larkStatus();
    case 'lark.login.begin':
      return { url: 'https://example.invalid', port: 53682, state: 'mock' };
    case 'lark.login.complete':
      loggedIn = true;
      return larkStatus();
    case 'board.list':
      return {
        me: 'ou_7f3a91',
        cards: CARDS,
        sprints: [{ id: 'spr42', name: 'Sprint 42', start_ms: Date.UTC(2026, 9, 13), end_ms: Date.UTC(2026, 9, 24), status: 'In Progress' }],
        synced_at_ms: Date.now() - 12_000,
        sync_error: null
      };
    case 'runs.list':
      if (scenarioFor('1') === 'reviewing') RUNS[0].state = 'reviewing';
      return RUNS;
    case 'approvals.list':
      return mockApprovals.filter((a) => a.run_id === String(params.run_id));
    case 'approvals.answer':
      mockApprovals = mockApprovals.map((a) =>
        a.id === params.approval_id ? { ...a, status: 'answered', answer: String(params.answer), closed_at_ms: Date.now() } : a
      );
      RUNS[0].pending_approvals = mockApprovals.filter((a) => a.status === 'pending').length;
      return {};
    case 'approvals.seen':
      return {};
    case 'questions.list':
      return mockQuestions.filter((q) => q.run_id === String(params.run_id));
    case 'questions.answer': {
      const q = mockQuestions.find((x) => x.id === params.question_id);
      const label = q?.options[Number(params.option)]?.label ?? null;
      mockQuestions = mockQuestions.map((x) =>
        x.id === params.question_id ? { ...x, status: 'answered', answer: label, closed_at_ms: Date.now() } : x
      );
      for (const r of RUNS) if (r.pending_question === params.question_id) r.pending_question = null;
      return {};
    }
    case 'runs.queued.send':
    case 'runs.queued.discard':
      return {};
    case 'runs.messages': {
      const all = MESSAGES[String(params.run_id)] ?? [];
      return all.filter((m) => m.id > Number(params.after_id ?? 0));
    }
    case 'agent.skills':
      return [
        { name: 'compound-engineering:ce-plan', description: 'Create structured plans for multi-step work.' },
        { name: 'compound-engineering:ce-brainstorm', description: 'Explore vague ideas into requirements.' }
      ];
    case 'runs.approve_plan':
    case 'runs.continue_to_plan':
      return {};
    case 'workspaces.list':
      return workspaces;
    case 'workspaces.add': {
      const ws = { id: String(params.name).toLowerCase().replace(/[^a-z0-9]+/g, '-'), browser_tests: true, gate_mode: 'block', ...params };
      workspaces.push(ws);
      return ws;
    }
    case 'workspaces.update': {
      const ws = workspaces.find((w) => w.id === params.id);
      if (ws) Object.assign(ws, { repo_ids: params.repo_ids, mcp_servers: params.mcp_servers });
      return ws;
    }
    case 'workspaces.remove':
      workspaces.splice(
        workspaces.findIndex((w) => w.id === params.id),
        1
      );
      return {};
    case 'workspaces.mcp_servers':
      return params.root === '/home/beni/work' ? ['figma', 'graft'] : [];
    case 'runs.changes':
      if (params.run_id === '9')
        return [
          { path: 'pegasus-api/src/payroll/cutoff.rs', old_path: null, status: 'modified', additions: 18, deletions: 3 },
          { path: 'pegasus-web/src/routes/payroll/+page.svelte', old_path: null, status: 'modified', additions: 9, deletions: 1 }
        ];
      return params.run_id === '1'
        ? [
            { path: 'src/lib/api/invoice.ts', old_path: null, status: 'modified', additions: 3, deletions: 2 },
            { path: 'src/routes/invoices/+page.svelte', old_path: null, status: 'modified', additions: 22, deletions: 4 },
            { path: 'src/lib/api/filter.ts', old_path: null, status: 'added', additions: 14, deletions: 0 }
          ]
        : [];
    case 'runs.file_diff':
      return PATCH;
    case 'card.comments':
      return [
        { reply_id: 'r1', author: 'ou_51aa6c', create_time: 1792370000, text: 'The error should mention the policy limit.', trusted: true, sent: true },
        { reply_id: 'r2', author: 'ou_2c81d4', create_time: 1792371000, text: 'Maybe also make it red?', trusted: false, sent: false }
      ];
    case 'runs.send':
      // Pesan tercatat dan agent membalas lewat `playSend` (jeda seperti daemon asli).
      return { outcome: 'sent' };
    case 'card.details': {
      const c = CARDS.find((x) => x.record_id === params.record_id);
      return {
        record_id: params.record_id,
        key: c?.key ?? null,
        title: c?.title ?? '',
        description: 'Users need to filter the invoice list.\n\n- by **status**\n- by a date range',
        status: c?.status ?? 'In Progress',
        readiness: c?.readiness ?? null,
        assignees: ['ou_7f3a91'],
        reporter: 'ou_51aa6c',
        sprint_ids: c?.sprint_ids ?? [],
        acs: [
          { id: 'ac1', label: 'AC-101', text: 'Status filter keeps its value in the URL', dropped: false },
          { id: 'ac2', label: 'AC-102', text: 'Date range defaults to this month', dropped: false },
          { id: 'ac3', label: 'AC-103', text: 'Export the filtered list to CSV', dropped: true }
        ]
      };
    }
    case 'people.names': {
      const known: Record<string, string> = {
        ou_7f3a91: 'Beni Handoko',
        ou_9e07b2: 'Dimas Aditya',
        ou_2c81d4: 'Rizky Pratama',
        ou_51aa6c: 'Sari Wulandari'
      };
      return (params.open_ids as string[]).filter((id) => known[id]).map((id) => ({ open_id: id, name: known[id] }));
    }
    case 'people.known':
      return [
        { open_id: 'ou_7f3a91', name: 'Beni Handoko' },
        { open_id: 'ou_9e07b2', name: 'Dimas Aditya' },
        { open_id: 'ou_2c81d4', name: 'Rizky Pratama' },
        { open_id: 'ou_51aa6c', name: 'Sari Wulandari' }
      ];
    case 'runs.discard_file':
      return {};
    case 'repos.discover':
      return [
        { path: '/home/beni/work/pegasus-web', name: 'pegasus-web', origin: 'git@github.com:talentlytica/pegasus-web.git', added: true },
        { path: '/home/beni/work/pegasus-api', name: 'pegasus-api', origin: 'git@github.com:talentlytica/pegasus-api.git', added: false },
        { path: '/home/beni/work/tools/design-tokens', name: 'design-tokens', origin: 'git@github.com:talentlytica/design-tokens.git', added: false }
      ];
    case 'fs.list_dirs': {
      const p = (params.path as string | null) ?? '/home/beni';
      const parent = p === '/' ? null : p.replace(/\/[^/]+$/, '') || '/';
      const kids: Record<string, [string, boolean][]> = {
        '/home/beni': [['Downloads', false], ['work', false]],
        '/home/beni/work': [['pegasus-api', true], ['pegasus-web', true], ['tools', false]],
        '/home/beni/work/tools': [['design-tokens', true]]
      };
      return {
        path: p,
        parent,
        entries: (kids[p] ?? []).map(([name, is_repo]) => ({ name, path: `${p === '/' ? '' : p}/${name}`, is_repo }))
      };
    }
    case 'system.check':
      return [
        { name: 'git', ok: true, detail: 'git version 2.47.1' },
        { name: 'gh', ok: true, detail: 'gh version 2.63.0 (2026-09-30)' },
        { name: 'gh login', ok: false, detail: 'You are not logged into any GitHub hosts. To log in, run: gh auth login' },
        { name: 'claude', ok: true, detail: '2.4.1 (Claude Code)' }
      ];
    case 'notifications.list':
      return [
        { id: 3, read: false, kind: 'awaiting_answer', record_id: 'rec1', run_id: '1', title: 'PGS-1402 is waiting for you', body: 'The agent finished its turn.', created_at_ms: Date.now() - 120_000 },
        { id: 2, read: false, kind: 'failed', record_id: 'rec3', run_id: '3', title: 'PGS-1379 failed', body: 'claude exited with code 1', created_at_ms: Date.now() - 900_000 },
        { id: 1, read: true, kind: 'pr_created', record_id: 'rec5', run_id: '5', title: 'PR #482 opened for PGS-1365', body: 'Lark moved to Waiting Review.', created_at_ms: Date.now() - 3_600_000 }
      ];
    case 'notifications.read':
      return {};
    case 'notifications.take':
      return [];
    case 'runs.start':
    case 'runs.takeover':
      return RUNS[0];
    case 'runs.finished':
      return {
        closable: mockFinished,
        skipped: [{ run: { run_id: 'm-skip', keys: ['PGS-1404'], title: 'Edit a job', bytes: 29_100_000 }, reason: 'unpushed_changes' }]
      };
    case 'runs.close_finished':
      return { count: (params as { run_ids: string[] }).run_ids.length };
    case 'runs.storage':
      return [
        { run_id: '1', keys: ['PGS-1401'], title: 'Job list', run_state: 'awaiting_dev', card_status: ['In Progress'], bytes: 1_001_000_000, updated_at_ms: Date.now() - 86_400_000 },
        { run_id: 'm-skip', keys: ['PGS-1404'], title: 'Edit a job', run_state: 'awaiting_dev', card_status: ['Waiting Deploy'], bytes: 29_100_000, updated_at_ms: Date.now() - 3_600_000 },
        ...mockFinished.map((r) => ({ run_id: r.run_id, keys: r.keys, title: r.title, run_state: 'in_review_loop', card_status: ['Delivered'], bytes: r.bytes, updated_at_ms: Date.now() }))
      ];
    case 'settings.cleanup.get':
      return { auto_close: mockAutoClose };
    case 'settings.cleanup.set':
      mockAutoClose = (params as { auto_close: boolean }).auto_close;
      return {};
    case 'groups.candidates': {
      // Kartu di antrean VM tidak bisa dipilih (KTD-G17).
      const queued = (params as { queued?: string[] }).queued ?? [];
      return [
        { record_id: 'recA', key: 'PGS-1410', title: 'Bulk approve overtime requests', blocked: null },
        { record_id: 'recF', key: 'PGS-1412', title: 'Overtime approval history', blocked: null },
        { record_id: 'recH', key: 'PGS-1413', title: 'Overtime export', blocked: 'Task already has an active run' }
      ].map((c) => (c.blocked === null && queued.includes(c.record_id) ? { ...c, blocked: 'Queued for VM' } : c));
    }
    case 'runs.start_group':
      return GROUP;
    case 'runs.next_card':
    case 'runs.continue_group':
    case 'runs.remove_card':
    case 'runs.stop_group':
      return {};
    case 'runs.gate_card_decision':
      return { outcome: params.decision === 'revert' ? 'reverted' : 'kept' };
    case 'app.run':
      return appView(String(params.run_id), 'starting');
    case 'app.status':
      return APP.get(String(params.run_id)) ?? appView(String(params.run_id), 'off');
    case 'app.owner':
      return [...APP.values()].find((a) => a.state !== 'off' && a.state !== 'stopped_by_other_run') ?? null;
    case 'app.changes':
      return [
        {
          repo_id: 'pegasus-web',
          snapshot: 'tree-mock',
          files: ['src/routes/invoices/Filters.svelte', 'package.json', 'src/lib/api/invoices.ts'],
          flagged: ['package.json'],
          prototype: null,
          confirmed: false
        }
      ];
    case 'app.stop': {
      const a = appView(String(params.run_id), 'off');
      APP.set(a.run_id, a);
      return a;
    }
    case 'app.confirm': {
      const a = APP.get(String(params.run_id)) ?? appView(String(params.run_id), 'off');
      if (typeof params.database === 'boolean') a.database_confirmed = params.database;
      if (typeof params.login === 'boolean') a.login_confirmed = params.login;
      return a;
    }
    case 'app.env_keys':
      return {
        found: true,
        keys: ['KELOLA_API_URL', 'KELOLA_API_MODE', 'KELOLA_APP_HOSTS', 'KELOLA_APP_BASE_URL', 'NEXT_PUBLIC_DEMO_TODAY']
      };
    case 'app.logs':
      return [{ repo_id: 'pegasus-web', text: '$ pnpm dev\n  VITE v6 ready in 412 ms\n  ➜  Local: http://localhost:5173/' }];
    case 'review.readiness': {
      const a = APP.get(String(params.run_id));
      const running = a?.state === 'running';
      const ready = params.run_id === '1';
      return {
        needs_dialog: ready,
        browser_tests: true,
        app: running ? 'current' : 'not_running',
        other_owner: null,
        confirmed: { database: a?.database_confirmed ?? false, login: a?.login_confirmed ?? false },
        app_urls: running ? ['http://localhost:5173'] : [],
        chrome_connected: chromeConnected,
        cant_verify_reason: running ? null : 'The app is not running'
      };
    }
    case 'design_sources.list':
      return designSources;
    case 'design_sources.revision':
      return { current: 'git:4f2a9c1', confirmed: 'git:1b7e3d0', files: ['src/app/(app)/jabatan/page.tsx'] };
    case 'design_sources.confirm': {
      const p = params as { revision: string };
      return { current: p.revision, confirmed: p.revision, files: [] };
    }
    case 'design_sources.test':
      return { ready: true, status: 'Ready' };
    case 'design_sources.add': {
      const p = params as { name: string; path: string; app: DesignSourceView['app'] };
      const s: DesignSourceView = {
        id: `ds${designSources.length + 1}`,
        name: p.name,
        kind: 'folder',
        path: p.path,
        ready: true,
        status: 'Ready',
        app: p.app ?? null
      };
      designSources = [...designSources, s];
      return s;
    }
    case 'design_sources.update': {
      const p = params as { id: string; name: string; path: string; app: DesignSourceView['app'] };
      designSources = designSources.map((s) =>
        s.id === p.id ? { ...s, name: p.name, path: p.path, app: p.app ?? null } : s
      );
      return designSources.find((s) => s.id === p.id) ?? null;
    }
    case 'design_sources.remove':
      designSources = designSources.filter((s) => s.id !== String(params.id));
      return {};
    case 'settings.reviewer.get':
      return reviewer;
    case 'settings.reviewer.set':
      reviewer = params as unknown as ReviewerSettingsView;
      return reviewer;
    case 'system.chrome_check':
      chromeConnected = true;
      return { connected: true, detail: '', checked_at_ms: Date.now() };
    case 'review.screenshot':
      return { data_url: mockShot(Number(params.index)) };
    case 'review.findings':
      return scenarioFor(String(params.run_id)) === 'lab'
        ? [
            {
              review_id: 'rv-lab',
              card: null,
              title: 'The success toast covers the Save button',
              detail: 'Seen while testing AC-3 as manager; the toast stays for 8 seconds.',
              steps: ['Sign in as manager', 'Save a filter', 'The toast covers Save']
            }
          ]
        : [];
    case 'ledger.look_decision': {
      const id = String(params.run_id);
      const rows = acRows(id).map((r) =>
        r.ac_id === params.ac_id && r.look_recommendation
          ? {
              ...r,
              visual: params.approve && r.look_recommendation.advice === 'accept' ? 'accepted' : r.visual,
              visual_detail: params.approve
                ? { ...r.visual_detail, decision_reason: r.look_recommendation.reason, decided_by: 'You' }
                : r.visual_detail,
              look_recommendation: null
            }
          : r
      );
      AC.set(id, rows);
      return rows;
    }
    case 'lab.get':
    case 'lab.set':
    case 'lab.approve':
    case 'lab.propose':
      return mockLab(method, params);
    case 'ledger.get': {
      const id = String(params.run_id);
      return ledger(id, acRows(id), scenarioFor(id));
    }
    case 'review.history':
      return history(String(params.run_id), scenarioFor(String(params.run_id)));
    case 'review.start':
    case 'review.cancel':
    case 'review.send_findings':
      return {};
    case 'runs.publish': {
      const id = String(params.run_id);
      const rows = activeRows(acRows(id));
      const reason = typeof params.push_anyway_reason === 'string' ? params.push_anyway_reason : null;
      const confirmed = Array.isArray(params.confirmed) ? (params.confirmed as string[]) : [];
      const g = gate(
        rows.map((r) =>
          r.verdict === 'fail' && reason
            ? { ...r, verdict: 'skipped' }
            : r.verdict === 'cant_verify' && confirmed.includes(r.ac_id)
              ? { ...r, verdict: 'checked_manually' }
              : r
        ),
        scenarioFor(id) === 'warn' ? 'warn' : 'block',
        false
      );
      const base = { pr_url: null, failed: [], unverified: [], warnings: [], sync_warning: null, review_id: null, changed_cards: [] };
      if (g.state === 'review_needed') return { ...base, outcome: 'review_started', review_id: 'rv-mock' };
      if (g.state === 'blocked') return { ...base, outcome: 'blocked', failed: ['AC-3'] };
      if (g.state === 'needs_confirmation') return { ...base, outcome: 'needs_confirmation', unverified: ['AC-4'] };
      return { ...base, outcome: 'published', pr_url: 'https://github.com/pegasus/pegasus-web/pull/490' };
    }
    default:
      return null;
  }
}

export class MockTransport implements Transport {
  private onLine: ((line: string) => void) | null = null;

  constructor() {
    // `/?mock&linux`: Background work seperti paket Linux (daemon dipasang app lokal).
    if (typeof location !== 'undefined' && location.search.includes('linux')) {
      background.provision = { state: 'up_to_date', systemd: true, linger: true, runner: 'local' };
    }
  }

  private emit(method: string, params: unknown): void {
    this.onLine?.(JSON.stringify({ jsonrpc: '2.0', method, params }));
  }

  /** Run app tiruan: setup, start, tunggu port, lalu Running. */
  private playApp(runId: string): void {
    ['setting_up', 'starting', 'waiting_for_port', 'running'].forEach((state, i) =>
      setTimeout(() => {
        APP.set(runId, appView(runId, state));
        this.emit('app.updated', { run_id: runId });
      }, 600 * i)
    );
  }

  /** Review tiruan: run menjadi Reviewing, baris terisi satu per satu, lalu kembali ke dev. */
  private playReview(runId: string): void {
    const r = RUNS.find((x) => x.id === runId);
    if (!r) return;
    const done = scenarioRows('current');
    let rows = acRows(runId).map((x) => (x.in_gate ? { ...x, verdict: 'not_reviewed', evidence: null, stale: false } : x));
    AC.set(runId, rows);
    r.state = 'reviewing';
    this.emit('run.updated', { run_id: runId });
    this.emit('ledger.updated', { run_id: runId, rows });
    rows.forEach((row, i) =>
      setTimeout(() => {
        const result = done.find((d) => d.ac_id === row.ac_id) ?? { ...row, verdict: 'pass' };
        rows = rows.map((x) => (x.ac_id === row.ac_id && x.in_gate ? { ...result, status: x.status } : x));
        AC.set(runId, rows);
        this.emit('ledger.updated', { run_id: runId, rows });
        if (i === rows.length - 1) {
          r.state = 'awaiting_dev';
          this.emit('run.updated', { run_id: runId });
          this.emit('review.progress', { run_id: runId });
        }
      }, 700 * (i + 1))
    );
  }

  async token(): Promise<{ token: string; protocol: string }> {
    return { token: 'mock', protocol: '1.1' };
  }

  async open(onLine: (line: string) => void): Promise<void> {
    this.onLine = onLine;
  }

  /** Penutupan tiruan di latar belakang: run hilang satu per satu, lalu notifikasi ringkasan. */
  private playClose(ids: string[]): void {
    const freed = mockFinished.filter((r) => ids.includes(r.run_id)).reduce((n, r) => n + (r.bytes ?? 0), 0);
    ids.forEach((id, i) =>
      setTimeout(() => {
        mockFinished = mockFinished.filter((r) => r.run_id !== id);
        this.emit('run.updated', { run_id: id });
        if (i === ids.length - 1) {
          this.emit('notice', {
            id: Date.now(),
            read: false,
            kind: 'runs_closed',
            record_id: '',
            run_id: null,
            title: 'Finished runs closed',
            body: `Closed ${ids.length} finished runs, freed ${(freed / 1e6).toFixed(1)} MB`,
            created_at_ms: Date.now()
          });
        }
      }, 800 * (i + 1))
    );
  }

  /** Daemon asli butuh beberapa detik untuk memulai sesi sebelum pesan tercatat dan agent membalas. */
  private playSend(runId: string, text: string): void {
    const run = RUNS.find((r) => r.id === runId);
    const list = (MESSAGES[runId] ??= []);
    setTimeout(() => {
      list.push({ id: nextMsg++, role: 'dev', body: { text }, created_at_ms: Date.now() });
      if (run) run.state = 'running';
      this.emit('run.message', { run_id: runId });
      this.emit('run.updated', { run_id: runId });
    }, 1500);
    setTimeout(() => {
      list.push({ id: nextMsg++, role: 'agent', body: { type: 'assistant_message', text: 'Noted. Working on it.' }, created_at_ms: Date.now() });
      list.push({ id: nextMsg++, role: 'agent', body: { type: 'done', is_error: false, result: 'ok' }, created_at_ms: Date.now() });
      if (run) run.state = 'awaiting_dev';
      this.emit('run.message', { run_id: runId });
      this.emit('run.updated', { run_id: runId });
    }, 5000);
  }

  async send(line: string): Promise<void> {
    const req = JSON.parse(line) as { id: number; method: string; params?: Record<string, unknown> };
    if (req.method === 'runs.close_finished') {
      this.playClose((req.params?.run_ids as string[] | undefined) ?? []);
    }
    if (req.method === 'runs.start' || req.method === 'runs.start_group') {
      const ids = req.params?.record_ids;
      const progressId = Array.isArray(ids) ? ids[0] : req.params?.record_id;
      ['claiming', 'creating_worktree', 'starting_agent', 'moving_status'].forEach((step, i) =>
        setTimeout(
          () =>
            this.onLine?.(
              JSON.stringify({ jsonrpc: '2.0', method: 'start.progress', params: { record_id: progressId, step } })
            ),
          i * 280
        )
      );
    }
    const result = handle(req.method, req.params ?? {});
    const delay = ['runs.start', 'runs.start_group', 'runs.takeover'].includes(req.method) ? 1200 : 120;
    setTimeout(() => {
      this.onLine?.(JSON.stringify({ jsonrpc: '2.0', id: req.id, result }));
      if (req.method === 'board.refresh') {
        setTimeout(
          () =>
            this.onLine?.(
              JSON.stringify({ jsonrpc: '2.0', method: 'sync.status', params: { synced_at_ms: Date.now(), sync_error: null } })
            ),
          1500
        );
      }
      if (req.method === 'runs.send') this.playSend(String(req.params?.run_id), String(req.params?.text ?? ''));
      if (req.method === 'review.start') this.playReview(String(req.params?.run_id));
      if (req.method === 'app.run') this.playApp(String(req.params?.run_id));
    }, delay);
  }
}
