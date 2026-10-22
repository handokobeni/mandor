// Transport tiruan untuk melihat UI di browser tanpa daemon dan Lark (`bun run dev`, buka `/?mock`).
// Hanya dimuat lewat dynamic import di mode dev; tidak masuk build produksi.

import type { DesignSourceView } from '#lib/protocol/DesignSourceView.ts';
import type { ReviewerSettingsView } from '#lib/protocol/ReviewerSettingsView.ts';
import type { CardView } from '#lib/protocol/CardView.ts';
import type { MessageView } from '#lib/protocol/MessageView.ts';
import type { RunView } from '#lib/protocol/RunView.ts';
import type { Transport } from '#lib/api/client.ts';
import type { AcRowView } from '#lib/protocol/AcRowView.ts';
import type { AppRunView } from '#lib/protocol/AppRunView.ts';
import { activeRows, gate } from '#lib/ac-review.ts';
import { history, ledger, scenarioFor, scenarioRows } from './mock-ac.ts';

/** State Run app tiruan per run. */
const APP = new Map<string, AppRunView>();
let chromeConnected = false;
function appView(runId: string, state: string): AppRunView {
  return {
    run_id: runId,
    state,
    members: [{ repo_id: 'pegasus-web', url: 'http://localhost:5173', state }],
    database_confirmed: false,
    login_confirmed: false,
    error: state === 'failed' ? 'pegasus-web: the app exited (code 1) before opening its port' : null
  };
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
    quiet_until_ms: state === 'wf_quiet' ? Date.now() + 95_000 : null,
    started_at_ms: Date.now() - 42 * 60_000,
    updated_at_ms: Date.now() - 60_000,
    ac: null,
    ...extra
  };
}

const RUNS: RunView[] = [
  run('1', 'PGS-1402', 'Filter invoices by status and date range', 'awaiting_dev', {
    ac: { total: 5, ok: 3, failed: 1, unverified: 1, not_reviewed: 0, stale: false, design_gaps: 1 }
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
  run('10', 'PGS-1347', 'Overtime approval reminders', 'awaiting_dev', { phase: 'brainstorm' })
];

// Sumber desain dan setelan reviewer tiruan (Rilis 2 U14).
let designSources: DesignSourceView[] = [
  { id: 'ds1', name: 'pegasus-mock', kind: 'folder', path: '/home/beni/work/pegasus-mock', ready: true, status: 'Ready' },
  { id: 'ds2', name: 'old-figma-export', kind: 'folder', path: '/home/beni/old/figma', ready: false, status: 'Path missing' }
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
  card(null, { record_id: 'recE', key: 'PGS-1340', title: 'Sort employees by join date', status: 'Delivered', column: 'done' }),
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
    { id: 5, role: 'agent', body: { type: 'done', is_error: false, result: 'ok' }, created_at_ms: 0 }
  ]
};

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
          browser_tests: true
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
          browser_tests: true
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

function larkStatus(): unknown {
  return { configured, logged_in: loggedIn, open_id: loggedIn ? 'ou_7f3a91' : null, name: loggedIn ? 'Beni Handoko' : null, reauth_required: false };
}

function handle(method: string, params: Record<string, unknown>): unknown {
  switch (method) {
    case 'hello':
      return { daemon_version: '0.1.0-mock', protocol: '1.0' };
    case 'status':
      return {
        version: '0.1.0-mock',
        uptime_ms: 1000,
        repos: 1,
        lark: larkStatus()
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
          browser_tests: true
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
    case 'runs.send': {
      const list = (MESSAGES[String(params.run_id)] ??= []);
      list.push({ id: nextMsg++, role: 'dev', body: { text: params.text }, created_at_ms: Date.now() });
      return { outcome: 'sent' };
    }
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
          flagged: ['package.json']
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
    case 'design_sources.test':
      return { ready: true, status: 'Ready' };
    case 'design_sources.add': {
      const p = params as { name: string; path: string };
      const s = { id: `ds${designSources.length + 1}`, name: p.name, kind: 'folder', path: p.path, ready: true, status: 'Ready' };
      designSources = [...designSources, s];
      return s;
    }
    case 'design_sources.update': {
      const p = params as { id: string; name: string; path: string };
      designSources = designSources.map((s) => (s.id === p.id ? { ...s, name: p.name, path: p.path } : s));
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
      const base = { pr_url: null, failed: [], unverified: [], warnings: [], sync_warning: null, review_id: null };
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
    return { token: 'mock', protocol: '1.0' };
  }

  async open(onLine: (line: string) => void): Promise<void> {
    this.onLine = onLine;
  }

  async send(line: string): Promise<void> {
    const req = JSON.parse(line) as { id: number; method: string; params?: Record<string, unknown> };
    if (req.method === 'runs.start') {
      ['claiming', 'creating_worktree', 'starting_agent', 'moving_status'].forEach((step, i) =>
        setTimeout(
          () =>
            this.onLine?.(
              JSON.stringify({ jsonrpc: '2.0', method: 'start.progress', params: { record_id: req.params?.record_id, step } })
            ),
          i * 280
        )
      );
    }
    const result = handle(req.method, req.params ?? {});
    const delay = req.method === 'runs.start' || req.method === 'runs.takeover' ? 1200 : 120;
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
      if (req.method === 'runs.send') {
        this.onLine?.(JSON.stringify({ jsonrpc: '2.0', method: 'run.message', params: { run_id: req.params?.run_id } }));
      }
      if (req.method === 'review.start') this.playReview(String(req.params?.run_id));
      if (req.method === 'app.run') this.playApp(String(req.params?.run_id));
    }, delay);
  }
}
