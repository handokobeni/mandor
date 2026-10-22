/* Mandor prototype data. Mirrors the daemon's board + runs payloads.
   Shapes follow the domain in the design prompt section 6. */
(function () {
  var people = {
    me: { id: 'ou_7f3a91', name: 'Beni Handoko', initials: 'BH' },
    rizky: { id: 'ou_2c81d4', name: 'Rizky Pratama', initials: 'RP' },
    dimas: { id: 'ou_9e07b2', name: 'Dimas Aditya', initials: 'DA' },
    sari: { id: 'ou_51aa6c', name: 'Sari Wulandari', initials: 'SW' },
    ayu: { id: 'ou_d4e210', name: 'Ayu Lestari', initials: 'AL' },
    yoga: { id: 'ou_88b3f0', name: 'Yoga Saputra', initials: 'YS' }
  };

  /* Run states: label, Lucide icon, tone (tone -> status token) */
  var runStates = {
    preparing:           { label: 'Preparing', icon: 'folder-git-2', tone: 'working', spin: false },
    running:             { label: 'Working', icon: 'loader-circle', tone: 'working', spin: true },
    awaiting_dev:        { label: 'Waiting for you', icon: 'hand', tone: 'attention' },
    publishing:          { label: 'Creating PR', icon: 'git-pull-request-arrow', tone: 'working', spin: false },
    in_review_loop:      { label: 'Done, waiting for review', icon: 'git-pull-request', tone: 'waiting' },
    wf_awaiting_comment: { label: 'Waiting for reviewer comments', icon: 'message-square-dashed', tone: 'waiting' },
    wf_quiet:            { label: 'Quiet period', icon: 'timer', tone: 'waiting' },
    wf_running:          { label: 'Fixing', icon: 'wrench', tone: 'working' },
    failed:              { label: 'Failed', icon: 'circle-x', tone: 'failed' },
    orphaned:            { label: 'Parked: claim lost', icon: 'square-parking', tone: 'warning' },
    stopped:             { label: 'Stopped', icon: 'circle-pause', tone: 'muted' },
    reviewing:           { label: 'Reviewing AC', icon: 'scan-search', tone: 'working', spin: false },
    /* Before coding (Release 1 as built) */
    brainstorming:       { label: 'Brainstorming', icon: 'lightbulb', tone: 'working' },
    bs_question:         { label: 'Question for you', icon: 'message-circle-question', tone: 'attention' },
    planning:            { label: 'Planning', icon: 'list-tree', tone: 'working' },
    plan_ready:          { label: 'Plan ready', icon: 'file-check', tone: 'attention' },
    /* Automatic fix finished but an AC failed or could not be verified */
    push_held:           { label: 'Push held', icon: 'shield-alert', tone: 'attention' }
  };

  var flags = {
    overwritten: { label: 'Overwritten by Lark', icon: 'triangle-alert', tone: 'warning', action: null },
    sync_failed: { label: 'Sync failed', icon: 'refresh-cw-off', tone: 'failed', action: 'Retry' },
    deleted:     { label: 'Deleted in Lark', icon: 'trash-2', tone: 'failed', action: 'Close run' },
    unassigned:  { label: 'No longer assigned', icon: 'user-x', tone: 'warning', action: 'Close run' }
  };

  var columns = [
    { id: 'todo', name: 'Todo' },
    { id: 'inprogress', name: 'In Progress' },
    { id: 'review', name: 'Review' },
    { id: 'done', name: 'Done', collapsible: true },
    { id: 'unknown', name: 'Unknown', hideWhenEmpty: true }
  ];

  var statusToColumn = {
    'To Do': 'todo', 'In Progress': 'inprogress', 'Waiting Fix': 'inprogress',
    'Waiting Review': 'review', 'In Review': 'review', 'Waiting Deploy': 'done', 'Delivered': 'done'
  };

  var READY = ['Ready To Dev', 'Commit To Sprint'];

  var sprints = [
    { id: 's42', name: 'Sprint 42', range: 'Oct 13 to Oct 24' },
    { id: 's41', name: 'Sprint 41', range: 'Sep 29 to Oct 10' }
  ];

  var repos = [
    { id: 'pegasus-web', path: '/home/beni/work/pegasus-web', branch: 'main', gh: 'pegasus-bot',
      companions: ['/home/beni/work/pegasus-design-tokens'], reviewers: ['ou_51aa6c', 'ou_88b3f0'],
      allowlist: ['Bash(pnpm test *)', 'Bash(pnpm lint)', 'Bash(just *)'] },
    { id: 'pegasus-api', path: '/home/beni/work/pegasus-api', branch: 'develop', gh: 'default',
      companions: [], reviewers: ['ou_51aa6c'], allowlist: ['Bash(cargo test *)', 'Bash(cargo clippy *)'] },
    /* Release 2: kelola-v2 workspace repos with Test mode = Web */
    { id: 'kelola-api', path: '/home/beni/work/kelola-v2/kelola-api', branch: 'main', gh: 'default', companions: [], reviewers: ['ou_51aa6c'], allowlist: ['Bash(cargo test *)'],
      test: { mode: 'web', setup: 'cargo build', start: 'cargo run --bin kelola-api', port: 8080, url: 'http://localhost:8080', env: ['DATABASE_URL', 'JWT_SECRET'] }, gate: 'workspace', browser: false },
    { id: 'kelola-web', path: '/home/beni/work/kelola-v2/kelola-web', branch: 'main', gh: 'default', companions: [], reviewers: ['ou_51aa6c'], allowlist: ['Bash(bun run test *)'],
      test: { mode: 'web', setup: 'bun install', start: 'bun run dev', port: 5173, url: 'http://localhost:5173', env: ['PUBLIC_API_URL'] }, gate: 'workspace', browser: true }
  ];
  var workspaces = [{ id: 'kelola-v2', path: '/home/beni/work/kelola-v2', repos: ['kelola-api', 'kelola-web'], browser: true, gate: 'block' }];

  function t(o) { return o; }
  var tasks = [
    /* ---- Todo ---- */
    t({ key: 'PGS-1402', title: 'Invoice list: filter by payment status and date range', status: 'To Do', readiness: 'Ready To Dev', sprint: 's42', repo: 'pegasus-web', owner: null, run: null }),
    t({ key: 'PGS-1409', title: 'Export attendance recap to CSV for HR', status: 'To Do', readiness: 'Commit To Sprint', sprint: 's42', repo: 'pegasus-api', owner: null, run: null, simulateClaimLost: true }),
    t({ key: 'PGS-1405', title: 'Bulk approve leave requests from the team calendar', status: 'To Do', readiness: 'Review AC By PM', sprint: 's42', repo: 'pegasus-web', owner: null, run: null }),
    t({ key: 'PGS-1411', title: 'Show contract end date on the employee profile header', status: 'To Do', readiness: 'Not Ready', sprint: 's42', repo: 'pegasus-web', owner: null, run: null }),

    /* ---- In Progress ---- */
    /* Release 2: board and sidebar variants. acs = AC summary from the run's ledger. */
    t({ key: 'PGS-1429', title: 'PO list: filter by supplier and order date', status: 'In Progress', readiness: 'Ready To Dev', sprint: 's42', repo: 'kelola-v2', repos: ['kelola-api', 'kelola-web'], owner: 'me',
        run: { state: 'awaiting_dev', branch: 'beni/pgs-1429-po-filters', since: '25m' }, acs: { ok: 5, total: 7, fail: 2, gaps: 2 } }),
    t({ key: 'PGS-1431', title: 'Supplier form: validate NPWP before saving', status: 'In Progress', readiness: 'Ready To Dev', sprint: 's42', repo: 'kelola-v2', repos: ['kelola-api', 'kelola-web'], owner: 'me',
        run: { state: 'push_held', branch: 'beni/pgs-1431-supplier-npwp', since: '8m', pr: 41, prs: [['kelola-web', 41]] }, acs: { ok: 4, total: 5, cant: 1, held: "AC-2 can't be verified: browser testing was off for this automatic fix." } }),
    t({ key: 'PGS-1433', title: 'Goods receipt: block quantities above the PO line', status: 'In Progress', readiness: 'Ready To Dev', sprint: 's42', repo: 'kelola-v2', repos: ['kelola-api', 'kelola-web'], owner: 'me',
        run: { state: 'awaiting_dev', branch: 'beni/pgs-1433-gr-qty', since: '1h' }, acs: { ok: 3, total: 4, cant: 1 } }),
    /* Release 2: multi-repo workspace run with an AC ledger (js/ac-data.js) */
    t({ key: 'PGS-1426', title: 'Purchase orders: approve or reject with a required reason', status: 'To Do', readiness: 'Ready To Dev', sprint: 's42', repo: 'kelola-v2', repos: ['kelola-api', 'kelola-web'], ac: true, owner: null, run: null }),
    t({ key: 'PGS-1391', title: 'Invoice detail: show partial payments as a timeline', status: 'In Progress', readiness: 'Ready To Dev', sprint: 's42', repo: 'pegasus-web', owner: 'me',
        run: { state: 'awaiting_dev', branch: 'beni/pgs-1391-partial-payments', since: '2m', sensitive: true } }),
    t({ key: 'PGS-1388', title: 'Fix rounding in overtime pay for half-hour shifts', status: 'In Progress', readiness: 'Commit To Sprint', sprint: 's42', repo: 'pegasus-api', owner: 'me',
        run: { state: 'running', branch: 'beni/pgs-1388-overtime-rounding', since: '6m' } }),
    t({ key: 'PGS-1384', title: 'Write an audit log entry when payroll is locked', status: 'In Progress', readiness: 'Ready To Dev', sprint: 's42', repo: 'pegasus-api', owner: 'me',
        run: { state: 'publishing', branch: 'beni/pgs-1384-payroll-lock-audit', since: '20s' } }),
    t({ key: 'PGS-1371', title: 'Employee search: match nickname and NIK', status: 'Waiting Fix', readiness: 'Ready To Dev', sprint: 's42', repo: 'pegasus-web', owner: 'me',
        run: { state: 'wf_quiet', branch: 'beni/pgs-1371-search-nik', pr: 407, quietLeft: 104, since: '16s' } }),
    t({ key: 'PGS-1366', title: 'Reimbursement form: block amounts above the policy limit', status: 'Waiting Fix', readiness: 'Ready To Dev', sprint: 's42', repo: 'pegasus-web', owner: 'me',
        run: { state: 'wf_awaiting_comment', branch: 'beni/pgs-1366-reimburse-limit', pr: 403, since: '1h', untrustedNotes: true } }),
    t({ key: 'PGS-1362', title: 'Shift schedule: prevent overlapping shifts for one employee', status: 'Waiting Fix', readiness: 'Ready To Dev', sprint: 's42', repo: 'pegasus-api', owner: 'me',
        run: { state: 'wf_running', branch: 'beni/pgs-1362-shift-overlap', pr: 399, since: '3m' } }),
    t({ key: 'PGS-1379', title: 'Payslip PDF: render Indonesian month names', status: 'In Progress', readiness: 'Ready To Dev', sprint: 's42', repo: 'pegasus-api', owner: 'me',
        run: { state: 'failed', branch: 'beni/pgs-1379-payslip-month', since: '14m', error: 'the daemon restarted while this run was active; resume to continue the session' } }),
    t({ key: 'PGS-1376', title: 'Department dropdown loads slowly with 2k+ rows', status: 'In Progress', readiness: 'Ready To Dev', sprint: 's42', repo: 'pegasus-web', owner: 'rizky', ownerActive: true,
        run: { state: 'orphaned', branch: 'beni/pgs-1376-dept-dropdown', since: '9m', error: 'Rizky Pratama claimed this task in Lark at 09:12. Your branch is kept; the agent is stopped.' } }),
    t({ key: 'PGS-1398', title: 'Login: rate-limit OTP requests per phone number', status: 'In Progress', readiness: 'Commit To Sprint', sprint: 's42', repo: 'pegasus-api', owner: 'rizky', ownerActive: true, ownerSeen: '3 min ago', run: null }),
    t({ key: 'PGS-1390', title: 'Attendance map: cluster pins at low zoom levels', status: 'In Progress', readiness: 'Ready To Dev', sprint: 's42', repo: 'pegasus-web', owner: 'dimas', ownerActive: false, ownerSeen: '47 min ago', run: null }),
    t({ key: 'PGS-1393', title: 'Leave balance: carry over unused days at year end', status: 'In Progress', readiness: 'Ready To Dev', sprint: 's42', repo: 'pegasus-api', owner: 'me',
        run: { state: 'in_review_loop', branch: 'beni/pgs-1393-leave-carryover', pr: 409, since: '25m' },
        flag: { type: 'sync_failed', reason: 'Lark returned 429 (rate limited) while moving the status to Waiting Review' } }),
    t({ key: 'PGS-1359', title: 'Import national holidays into the company calendar', status: 'In Progress', readiness: 'Ready To Dev', sprint: 's42', repo: 'pegasus-api', owner: 'me',
        run: { state: 'stopped', branch: 'beni/pgs-1359-holiday-import', since: '1h' },
        flag: { type: 'deleted', reason: 'The record was deleted from Sprint Tasks' } }),
    t({ key: 'PGS-1385', title: 'Overtime request: notify the approver in Lark', status: 'In Progress', readiness: 'Ready To Dev', sprint: 's42', repo: 'pegasus-api', owner: 'me',
        run: { state: 'stopped', branch: 'beni/pgs-1385-overtime-notify', since: '2h' },
        flag: { type: 'unassigned', reason: 'You were removed from Assignee in Lark by Ayu Lestari' } }),

    /* ---- Review ---- */
    t({ key: 'PGS-1380', title: 'Payroll summary: group totals by cost center', status: 'Waiting Review', readiness: 'Ready To Dev', sprint: 's42', repo: 'pegasus-api', owner: 'me',
        run: { state: 'in_review_loop', branch: 'beni/pgs-1380-cost-center', pr: 412, since: '1h' } }),
    t({ key: 'PGS-1395', title: 'Employee import: validate NPWP format per row', status: 'In Review', readiness: 'Ready To Dev', sprint: 's42', repo: 'pegasus-web', owner: 'me',
        run: { state: 'in_review_loop', branch: 'beni/pgs-1395-npwp-validate', pr: 415, since: '40m' },
        flag: { type: 'overwritten', reason: 'Status in Lark is now In Review, not In Progress' } }),

    /* ---- Done ---- */
    t({ key: 'PGS-1350', title: 'Dark mode for the HR admin portal', status: 'Delivered', readiness: 'Ready To Dev', sprint: 's41', repo: 'pegasus-web', owner: 'me', run: null, pr: 388 }),
    t({ key: 'PGS-1355', title: 'Payslip email: attach the PDF instead of a link', status: 'Waiting Deploy', readiness: 'Ready To Dev', sprint: 's42', repo: 'pegasus-api', owner: 'me', run: null, pr: 396 }),
    t({ key: 'PGS-1352', title: 'Remove the legacy attendance CSV importer', status: 'Delivered', readiness: 'Ready To Dev', sprint: 's41', repo: 'pegasus-api', owner: 'me', run: null, pr: 391 }),

    /* ---- Unknown status ---- */
    t({ key: 'PGS-1368', title: 'BPJS contribution rates for 2027', status: 'On Hold', readiness: 'Ready To Dev', sprint: 's42', repo: 'pegasus-api', owner: null, run: null })
  ];

  /* Rich task details (Task tab) for the main demo tasks; others use a generic body */
  var details = {
    'PGS-1391': {
      description: 'Finance needs to see how an invoice was paid when a client pays in several transfers. Today the detail page only shows the remaining balance.\n\nShow each payment as a row in a vertical timeline under the totals: date, amount, method, and who recorded it. The remaining balance after each payment is shown on the right.',
      ac: [
        { id: 'AC-5521', text: 'The timeline lists payments newest first, with date, amount in Rupiah, method, and recorder.' },
        { id: 'AC-5522', text: 'Each row shows the remaining balance after that payment.' },
        { id: 'AC-5523', text: 'An invoice with no payments shows "No payments recorded yet".' },
        { id: 'AC-5524', text: 'Payments can be edited inline from the timeline.', dropped: true },
        { id: 'AC-5525', text: 'Amounts use id-ID formatting, e.g. Rp 1.250.000.' }
      ],
      reporter: 'ayu', assignees: ['me']
    }
  };

  details['PGS-1426'] = {
    description: 'Approvers in kelola need to approve or reject purchase orders from the PO pages. Rejections must carry a reason the requester can read, so a rejected PO is not resubmitted unchanged.\n\nThe design for the list and detail pages is in kelola-design. Bulk approve was moved to a separate Approvals page in the latest design.',
    ac: [
      { id: 'AC-1', text: 'Approvers see a "Pending approval" filter on the purchase order list, with the number of POs waiting.' },
      { id: 'AC-2', text: 'Approve and Reject appear only for users with the approver role; other users see the status only.' },
      { id: 'AC-3', text: 'Rejecting a PO requires a reason of at least 10 characters; the reason is shown to the requester.' },
      { id: 'AC-4', text: 'When a PO is approved, the requester gets a Lark message with the PO number and approver.' },
      { id: 'AC-5', text: 'Approvers can bulk approve up to 20 POs from the list page.' },
      { id: 'AC-6', text: 'The PO detail shows an approval history with who, when, and the reason.' },
      { id: 'AC-7', text: 'Every approve and reject is written to the audit log.' },
      { id: 'AC-8', text: 'Approved POs can be exported to CSV.', dropped: true }
    ],
    reporter: 'ayu', assignees: ['me']
  };

  var comments = {
    'PGS-1371': [
      { author: 'sari', time: '09:41', trusted: true, sent: false, text: 'Searching "Budi" finds nobody whose nickname is Budi. The NIK match works but only on the full 16 digits; partial NIK (last 6) should work too.' },
      { author: 'sari', time: '09:42', trusted: true, sent: false, text: 'Also, results should highlight which field matched (name, nickname, or NIK).' }
    ],
    'PGS-1366': [
      { author: 'yoga', time: 'Yesterday 17:20', trusted: true, sent: true, text: 'Limit check passes, but the error message says "Invalid amount". Please show the policy limit in the message.' },
      { author: 'rizky', time: '08:55', trusted: false, sent: false, text: 'Maybe also round to the nearest thousand before comparing? Not sure that is in the AC.' }
    ],
    'PGS-1362': [
      { author: 'sari', time: '10:02', trusted: true, sent: true, text: 'Overlap check ignores night shifts that cross midnight. 22:00 to 06:00 followed by 05:00 to 13:00 is accepted.' }
    ]
  };

  /* ---------------- Store (overrides persisted in localStorage) ------ */
  var KEY = 'mandor:v1:runs';
  var RKEY = 'mandor:v1:repos';
  /* In the frames gallery (?frame=1) each view starts from base data and keeps
     its changes in memory, so it never touches the prototype's saved state. */
  var QS = new URLSearchParams(location.search);
  /* Preview links (?ac=, ?app=, ?gate=, ?phase=) also keep changes in memory */
  var FRAME = QS.get('frame') === '1' || ['ac', 'app', 'gate', 'phase'].some(function (k) { return QS.get(k); });
  var mem = {};
  var store = FRAME
    ? { getItem: function (k) { return k in mem ? mem[k] : null; }, setItem: function (k, v) { mem[k] = String(v); }, removeItem: function (k) { delete mem[k]; } }
    : localStorage;
  function readOverrides() { try { return JSON.parse(store.getItem(KEY) || '{}'); } catch (e) { return {}; } }
  function writeOverrides(o) { try { store.setItem(KEY, JSON.stringify(o)); } catch (e) {} }

  function getTask(key) {
    var base = tasks.find(function (x) { return x.key === key; });
    if (!base) return null;
    var o = readOverrides()[key];
    var task = JSON.parse(JSON.stringify(base));
    if (o) {
      if (o.status) task.status = o.status;
      if ('owner' in o) task.owner = o.owner;
      if ('ownerActive' in o) task.ownerActive = o.ownerActive;
      if ('flag' in o) task.flag = o.flag;
      if ('run' in o) task.run = o.run ? Object.assign({}, task.run || {}, o.run) : null;
      if (o.closed) task.closed = true;
    }
    previewRun(task);
    task.column = statusToColumn[task.status] || 'unknown';
    return task;
  }
  /* Preview links (?ac=, ?app=, ?gate=, ?phase=) show PGS-1426 mid-run without
     starting it from the board. Nothing is persisted. */
  var PHASE = { brainstorming: 'brainstorming', question: 'bs_question', planning: 'planning', plan: 'plan_ready', implementing: 'running', reviewing: 'reviewing' };
  function previewRun(task) {
    if (task.key !== 'PGS-1426' || task.run) return;
    var q = new URLSearchParams(location.search), ph = q.get('phase');
    if (!(ph || q.get('ac') || q.get('app') || q.get('gate'))) return;
    task.status = 'In Progress'; task.owner = 'me';
    task.run = { state: PHASE[ph] || 'awaiting_dev', branch: 'beni/pgs-1426-po-approval', since: '14m', planned: true, approved: !(ph && /brainstorming|question|planning|plan$/.test(ph)) };
    task.preview = true;
  }
  function baseTask(key) {
    var b = tasks.find(function (x) { return x.key === key; });
    if (!b) return null;
    var c = JSON.parse(JSON.stringify(b)); c.column = statusToColumn[c.status] || 'unknown'; return c;
  }
  function allTasks() { return tasks.map(function (x) { return getTask(x.key); }).filter(function (x) { return !x.closed; }); }
  function update(key, patch) {
    var all = readOverrides();
    var cur = all[key] || {};
    if (patch.run && cur.run) patch = Object.assign({}, patch, { run: Object.assign({}, cur.run, patch.run) });
    all[key] = Object.assign(cur, patch);
    writeOverrides(all);
    window.dispatchEvent(new CustomEvent('mandor:change', { detail: { key: key } }));
  }
  function reset() { store.removeItem(KEY); store.removeItem(RKEY); store.removeItem(AKEY); store.removeItem(SKEY); store.removeItem('mandor:v1:startprefs'); Object.keys(localStorage).forEach(function (k) { if (k.indexOf('mandor:v1:ac:') === 0) localStorage.removeItem(k); }); }
  function getRepos() { try { var r = JSON.parse(store.getItem(RKEY)); return r || repos; } catch (e) { return repos; } }
  function setRepos(list) { store.setItem(RKEY, JSON.stringify(list)); }

  /* Local app (Release 2, Run app). Only one run's app runs at a time, so
     the state is global: { owner: task key, state, startedAt }.
     ?app=<state> previews a state for PGS-1426; ?app=other shows PGS-1391 owning it. */
  var AKEY = 'mandor:v1:app';
  function getApp() {
    var q = new URLSearchParams(location.search).get('app');
    if (q === 'other') return { owner: 'PGS-1391', state: 'running', startedAt: '09:12' };
    if (q) return { owner: 'PGS-1426', state: q, startedAt: '10:58' };
    try { return JSON.parse(store.getItem(AKEY)) || { owner: null, state: 'off' }; } catch (e) { return { owner: null, state: 'off' }; }
  }
  function setApp(a) {
    if (new URLSearchParams(location.search).get('app')) return;
    store.setItem(AKEY, JSON.stringify(a));
    window.dispatchEvent(new CustomEvent('mandor:app'));
  }

  /* Design sources (Release 2). Global, set in Settings › Design sources.
     ?sources=empty previews the empty state. Kinds other than repo are 2.1. */
  var SKEY = 'mandor:v1:sources';
  var designSources = [
    { id: 'kelola-design', kind: 'repo', name: 'kelola-design', location: '/home/beni/work/kelola-design', status: 'ok' },
    { id: 'pegasus-mock', kind: 'repo', name: 'pegasus-mock', location: '/home/beni/work/pegasus-mock', status: 'error', error: 'Path missing' },
    { id: 'kelola-figma', kind: 'figma', name: 'Kelola v2 screens', location: 'https://www.figma.com/design/k3LvQ/Kelola-v2', status: 'ok' },
    { id: 'kelola-proto', kind: 'url', name: 'Kelola prototype', location: 'https://proto.kelola.example.com', status: 'error', error: 'URL unreachable' },
    { id: 'po-exports', kind: 'images', name: 'PO flow exports', location: '/home/beni/designs/po-flow', status: 'untested' }
  ];
  function getSources() {
    if (QS.get('sources') === 'empty') return [];
    try { return JSON.parse(store.getItem(SKEY)) || designSources; } catch (e) { return designSources; }
  }
  function setSources(list) { store.setItem(SKEY, JSON.stringify(list)); }

  function startGate(task) {
    if (task.run) return 'a run already exists for this task';
    if (task.owner && task.owner !== 'me' && task.ownerActive) return people[task.owner].name + ' is working on this task';
    if (task.status !== 'To Do') return 'status in Lark is "' + task.status + '", not To Do';
    if (READY.indexOf(task.readiness) === -1) return 'task is not ready: Development Readiness "' + task.readiness + '"';
    return null;
  }

  /* Notification events: same list feeds desktop notifications and the in-app center.
     kind matches Settings > General notification toggles. */
  var notifications = [
    { id: 'n6', kind: 'waiting', task: 'PGS-1391', icon: 'hand', ago: '2m', title: 'PGS-1391 is waiting for you', body: 'The agent finished its turn on "Invoice detail: show partial payments as a timeline".' },
    { id: 'n5', kind: 'attention', task: 'PGS-1391', icon: 'shield-alert', ago: '3m', title: 'PGS-1391: push held for your confirmation', body: 'The changes touch .github/workflows/ci.yml. Confirm or discard in the workspace.' },
    { id: 'n4', kind: 'fix', task: 'PGS-1371', icon: 'message-square-text', ago: '6m', title: 'Reviewer requested fixes on PGS-1371', body: 'Sari Wulandari moved it to Waiting Fix. 2 trusted comments will be forwarded after the quiet period.' },
    { id: 'n3', kind: 'failed', task: 'PGS-1379', icon: 'circle-x', ago: '14m', title: 'Run failed: PGS-1379', body: 'The daemon restarted while this run was active. Resume it from the workspace.' },
    { id: 'n2', kind: 'attention', task: 'PGS-1376', icon: 'square-parking', ago: '22m', title: 'PGS-1376 is parked: claim lost', body: 'Rizky Pratama claimed the task in Lark. Your branch is kept; the agent is stopped.' },
    { id: 'n1', kind: 'pr', task: 'PGS-1380', icon: 'git-pull-request', ago: '1h', title: 'PR #412 created for PGS-1380', body: 'Lark moved the task to Waiting Review.' }
  ];
  /* Release 2 notifications */
  notifications.unshift(
    { id: 'n9', kind: 'review', task: 'PGS-1426', href: 'workspace.html?task=PGS-1426&tab=ac&ac=current', icon: 'scan-search', ago: 'now', title: 'Review finished on PGS-1426: 4 passed, 1 failed', body: "Reviewer found AC-3 failing and couldn't verify AC-4. Nothing was pushed. Press Push & create PR again when ready." },
    { id: 'n8', kind: 'attention', task: 'PGS-1431', icon: 'shield-alert', ago: '1m', title: 'PGS-1431: automatic fix push held', body: "AC-2 can't be verified. Confirm it yourself, then push the fix." },
    { id: 'n7', kind: 'failed', task: 'PGS-1426', href: 'workspace.html?task=PGS-1426&app=failed&logs=open', icon: 'app-window', ago: '4m', title: 'App failed to start for PGS-1426', body: 'kelola-web exited with code 1. See the logs in the workspace.' }
  );
  /* AC summary for cards and the sidebar: fixture acs, or the saved ledger */
  function acSummary(task) {
    if (task.acs) return task.acs;
    if (!task.ac || !task.run) return null;
    var l; try { l = JSON.parse(localStorage.getItem('mandor:v1:ac:' + task.key)); } catch (e) { l = null; }
    if (!l || !l.rows || !l.rows.length) return null;
    var c = { ok: 0, total: 0, fail: 0, cant: 0, gaps: 0, stale: false, reviewing: !!l.reviewing, reviewed: !!l.lastReview };
    l.rows.forEach(function (r) {
      c.total++; var v = r.verdict.v;
      if (v === 'pass' || v === 'manual' || v === 'skipped' || v === 'follows') c.ok++;
      if (v === 'fail') c.fail++; if (v === 'cant') c.cant++; if (r.verdict.stale) c.stale = true;
      if (r.design && ['missing', 'partly', 'conflicts'].indexOf(r.design.v) > -1) c.gaps++;
    });
    return c;
  }
  var NKEY = 'mandor:v1:notif-read';
  function readSet() { try { return JSON.parse(store.getItem(NKEY) || '["n1","n2"]'); } catch (e) { return []; } }
  function getNotifications() { var r = readSet(); return notifications.map(function (n) { return Object.assign({ read: r.indexOf(n.id) > -1 }, n); }); }
  function markRead(ids) { var r = readSet(); ids.forEach(function (i) { if (r.indexOf(i) === -1) r.push(i); }); store.setItem(NKEY, JSON.stringify(r)); }

  window.MANDOR = {
    people: people, runStates: runStates, flags: flags, columns: columns, sprints: sprints,
    details: details, comments: comments, READY: READY,
    getTask: getTask, baseTask: baseTask, getApp: getApp, setApp: setApp, getSources: getSources, setSources: setSources, workspaces: workspaces, getNotifications: getNotifications, acSummary: acSummary, markRead: markRead, notificationsRaw: notifications, allTasks: allTasks, update: update, reset: reset,
    getRepos: getRepos, setRepos: setRepos, startGate: startGate,
    version: { app: '0.4.2', daemon: '0.4.2' }
  };
})();
