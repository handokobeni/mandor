/* Component sheet: renders every shadcn component used by Mandor and every
   Mandor component with its variants, in dark, light, or both themes.
   Mandor components use the same renderers as the screens (js/components.js
   and js/shell.js), so the sheet cannot drift from the product. */
(function () {
  var M = window.MANDOR, U = window.UI, esc = U.esc, ic = U.ic, $ = U.$, $$ = U.$$;

  function spec(label, html, cls) {
    return '<div class="spec' + (cls ? ' ' + cls : '') + '"><div class="spec-body">' + html + '</div><div class="spec-label">' + esc(label) + '</div></div>';
  }
  function sec(id, title, desc, body) {
    return '<section class="sec" data-sec="' + id + '" id="' + id + '-{{p}}"><h2>' + title + '</h2>' + (desc ? '<p>' + desc + '</p>' : '') + '<div class="specs">' + body + '</div></section>';
  }

  var SECTIONS = [
    ['Foundations', [['theme', 'Theme tokens'], ['type', 'Typography']]],
    ['shadcn', [['button', 'Button'], ['badge', 'Badge, Kbd, Avatar'], ['form', 'Form controls'], ['tabs', 'Tabs, Toggle group'], ['alert', 'Alert'], ['card', 'Card'], ['overlay', 'Dialog, Alert Dialog'], ['menu', 'Dropdown, Command, Tooltip'], ['feedback', 'Sonner, Skeleton, Spinner']]],
    ['Mandor', [['runstatus', 'RunStatusBadge'], ['flags', 'CardFlagBadge'], ['taskcard', 'TaskCard'], ['agentrow', 'AgentRow'], ['chat', 'ChatMessage'], ['tools', 'ToolCallGroup'], ['diff', 'DiffViewer'], ['composer', 'Composer'], ['steps', 'StepProgress'], ['comment', 'CommentItem'], ['empty', 'Empty state'], ['stepper', 'Stepper'], ['notif', 'NotificationCenter'], ['tray', 'TrayMenu, tray icon']]],
    ['Release 2', [['acchips', 'DesignChip, VerdictChip'], ['acrow', 'ACLedgerRow'], ['acsummary', 'ACReviewSummary'], ['gatebtn', 'GateButton'], ['runapp', 'RunAppControl'], ['readiness', 'ReadinessItem'], ['sources', 'DesignSourceItem'], ['plangaps', 'PlanTab design gaps'], ['r2board', 'Board, sidebar, title bar'], ['logs', 'AppLogs drawer']]],
    ['Release 3', [['planvm', 'Plan by day, queue for VM'], ['groupvm', 'Grouped cards on the VM']]],
    ['Reserved', [['reserved', 'Slots for later releases']]]
  ];

  /* ---------- foundations ---------- */
  var SHADCN_VARS = [
    ['--background', 'App background'], ['--foreground', 'Main text'], ['--card', 'Cards, panels'], ['--popover', 'Menus, dialogs'],
    ['--primary', 'Primary actions'], ['--primary-foreground', 'Text on primary'], ['--secondary', 'Secondary buttons'], ['--muted', 'Subtle surfaces'],
    ['--muted-foreground', 'Secondary text'], ['--accent', 'Hover, selected rows'], ['--destructive', 'Destructive actions'], ['--border', 'Borders'],
    ['--input', 'Input borders'], ['--ring', 'Focus ring'], ['--sidebar', 'Agents sidebar']
  ];
  var STATUS_VARS = [
    ['--status-working', 'preparing, running, publishing, wf_running'], ['--status-attention', 'awaiting_dev (filled chip)'], ['--status-waiting', 'in_review_loop, wf_awaiting_comment, wf_quiet'],
    ['--status-failed', 'failed, sync_failed, deleted'], ['--status-warning', 'overwritten, unassigned, orphaned'], ['--status-ok', 'done, success steps'], ['--status-pr', 'PR links'],
    ['--diff-add', 'Diff added text'], ['--diff-add-bg', 'Diff added row'], ['--diff-del', 'Diff deleted text'], ['--diff-del-bg', 'Diff deleted row']
  ];
  function tokTable(list) {
    return '<table class="tok-table">' + list.map(function (v) {
      return '<tr><td style="width:40px"><div class="sw" style="background:var(' + v[0] + ')"></div></td><td class="mono">' + v[0] + '</td><td class="val" data-var="' + v[0] + '"></td><td class="muted">' + v[1] + '</td></tr>';
    }).join('') + '</table>';
  }
  function theme() {
    return sec('theme', 'Theme tokens', 'shadcn variable names, so they map 1:1 to the shadcn-svelte theme. Values are read live from this theme. Radius <span class="mono">--radius: 6px</span>.',
      spec('shadcn variables', tokTable(SHADCN_VARS), 'block') + spec('Mandor status tokens (always paired with an icon and a label)', tokTable(STATUS_VARS), 'block'));
  }
  function type() {
    var rows = [['22 / 600 · page title', 'Settings', 'font-size:22px;font-weight:600;letter-spacing:-0.015em'], ['18 / 600 · toolbar title', 'Board', 'font-size:18px;font-weight:600;letter-spacing:-0.01em'],
      ['15 / 600 · dialog, workspace title', 'Invoice detail: show partial payments as a timeline', 'font-size:15px;font-weight:600'], ['13 / 400 · body', 'Turn finished. Reply, or push when the changes look right.', 'font-size:13px'],
      ['12 / 500 · secondary, buttons sm', 'Synced 12s ago', 'font-size:12px;font-weight:500'], ['11 / 500 caps +0.07em · group labels', '<span class="caps">Waiting for you</span>', ''],
      ['JetBrains Mono 12 · keys, branches, paths', '<span class="mono">PGS-1388 · beni/pgs-1388-overtime-rounding</span>', '']];
    return sec('type', 'Typography', 'Space Grotesk for UI, JetBrains Mono for code, task keys, branches, and tool calls.',
      rows.map(function (r) { return spec(r[0], '<span style="' + r[2] + '">' + r[1] + '</span>', 'block'); }).join(''));
  }

  /* ---------- shadcn ---------- */
  function button() {
    var v = [['default', 'Start', 'play'], ['secondary', 'Queue', 'list-plus'], ['outline', 'Retry', 'refresh-cw'], ['ghost', 'Open', 'arrow-right'], ['destructive', 'Close run', null]];
    var sizes = [['', 'default'], ['btn-sm', 'sm'], ['btn-xs', 'xs']];
    var out = sizes.map(function (s) {
      return spec('Button / size=' + s[1], v.map(function (x) { return '<button class="btn btn-' + x[0] + ' ' + s[0] + '" title="variant=' + x[0] + '">' + (x[2] ? ic(x[2]) : '') + x[1] + '</button>'; }).join(''), 'block');
    }).join('');
    out += spec('Button / size=icon', '<button class="btn btn-ghost btn-icon" aria-label="Settings">' + ic('settings') + '</button><button class="btn btn-ghost btn-sm btn-icon" aria-label="More">' + ic('ellipsis') + '</button><button class="btn btn-outline btn-xs btn-icon" aria-label="Copy">' + ic('copy', 'icon-sm') + '</button>');
    out += spec('Button / state=disabled', '<button class="btn btn-default btn-sm" disabled>' + ic('play') + 'Start</button><button class="btn btn-secondary btn-sm" disabled>Send</button>');
    out += spec('Button / state=loading', '<button class="btn btn-default btn-sm" disabled>' + ic('loader-circle', 'spin') + 'Starting</button><button class="btn btn-outline btn-xs" disabled>' + ic('loader-circle', 'spin icon-sm') + 'Retrying</button>');
    out += spec('Button / variant=link', '<button class="btn btn-link">Setup guide</button>');
    out += spec('ButtonGroup', '<div class="btn-group"><button class="btn btn-outline btn-sm">' + ic('square') + 'Stop</button><button class="btn btn-outline btn-sm">' + ic('zap') + 'Send now</button><button class="btn btn-outline btn-sm">' + ic('list-plus') + 'Queue</button></div>');
    return sec('button', 'Button', 'One primary (amber) action per view. Everything else is secondary, outline, or ghost.', out);
  }
  function badge() {
    return sec('badge', 'Badge, Kbd, Avatar, Separator', '',
      spec('Badge / variant=outline', '<span class="badge">' + ic('circle-dot') + 'Lark: In Progress</span><span class="badge">' + ic('circle-check') + 'Ready To Dev</span>') +
      spec('Badge / variant=secondary', '<span class="badge badge-secondary">' + ic('eye') + 'Read-only</span><span class="badge badge-secondary">' + ic('lock') + 'Will be sent</span>') +
      spec('Badge / trust', '<span class="badge trust" data-trusted="true">' + ic('shield-check') + 'Trusted</span><span class="badge trust" data-trusted="false">' + ic('shield-question') + 'Not trusted</span>') +
      spec('Kbd', '<span class="kbd">Ctrl K</span><span class="kbd">⌘ ↵</span><span class="kbd">Esc</span>') +
      spec('Avatar / size=sm, md', U.avatar('me') + U.avatar('sari') + U.avatar('rizky', true)) +
      spec('Separator', '<div style="width:160px"><hr class="separator"></div>'));
  }
  function form() {
    return sec('form', 'Form controls', 'Labels are always visible. Errors are shown under the field with an icon, and the field gets aria-invalid.',
      spec('Label + Input + help', '<div class="field" style="width:280px"><label class="label">App ID</label><input class="input mono" value="cli_a6f2c81d93b8d00e"><p class="field-help">Starts with <span class="mono">cli_</span>.</p></div>') +
      spec('Input / state=invalid', '<div class="field" style="width:280px"><label class="label">Sprint Tasks table ID</label><input class="input mono" value="8kX2pQm4" aria-invalid="true"><p class="field-error">' + ic('circle-alert', 'icon-sm') + 'Table IDs start with tbl.</p></div>') +
      spec('Input / type=password, saved', '<div class="input-group" style="width:280px"><input class="input" type="password" placeholder="•••••••• saved" disabled><button class="btn btn-outline">Replace</button></div>') +
      spec('Input / size=sm, search', '<div class="toolbar" style="border:0;padding:0"><div class="search"><i data-lucide="search" class="icon-sm" aria-hidden="true"></i><input class="input input-sm" placeholder="Search key or title"></div></div>') +
      spec('Select / size=sm', '<select class="select input-sm" style="width:auto"><option>Sprint 42 · Oct 13 to Oct 24</option></select>') +
      spec('Textarea', '<textarea class="textarea" style="width:320px" placeholder="Extra instructions for the agent"></textarea>') +
      spec('Checkbox / checked, unchecked, locked', '<input type="checkbox" class="checkbox" checked aria-label="checked"><input type="checkbox" class="checkbox" aria-label="unchecked"><input type="checkbox" class="checkbox" checked disabled aria-label="locked">') +
      spec('Switch / on, off', '<input type="checkbox" class="switch" role="switch" checked aria-label="on"><input type="checkbox" class="switch" role="switch" aria-label="off">') +
      spec('RadioGroup / card', '<div class="radio-row"><label class="radio-card"><input type="radio" name="th-{{p}}" checked>' + ic('moon') + 'Dark</label><label class="radio-card"><input type="radio" name="th-{{p}}">' + ic('sun') + 'Light</label></div>'));
  }
  function tabs() {
    var list = [['messages-square', 'Chat', ''], ['file-diff', 'Changes', '5'], ['list-checks', 'Task', ''], ['message-square-text', 'Review comments', '2']];
    return sec('tabs', 'Tabs, Toggle group', 'Tabs are clickable here; arrow keys move between them in the workspace.',
      spec('Tabs', '<div class="tabs-list sheet-tabs" role="tablist" style="padding:0;width:520px">' + list.map(function (x, i) { return '<button class="tabs-trigger" role="tab" aria-selected="' + (i === 0) + '">' + ic(x[0]) + x[1] + (x[2] ? ' <span class="count">' + x[2] + '</span>' : '') + '</button>'; }).join('') + '</div>') +
      spec('ToggleGroup / size=sm', '<div class="toggle-group"><button type="button" aria-pressed="false">Mine</button><button type="button" aria-pressed="true">All assigned</button></div>'));
  }
  function alert() {
    return sec('alert', 'Alert', 'Used for global banners and workspace callouts. Factual, not alarming.',
      spec('Alert / variant=default', '<div class="alert" style="width:560px">' + ic('rotate-ccw') + '<div><div class="alert-title">The daemon restarted at 09:58</div><div class="muted">1 run was interrupted. Resume it from its workspace.</div></div><div class="alert-actions"><button class="btn btn-ghost btn-sm btn-icon" aria-label="Dismiss">' + ic('x') + '</button></div></div>', 'block') +
      spec('Alert / variant=warning', '<div class="alert alert-warning" style="width:560px">' + ic('shield-alert') + '<div><div class="alert-title">Changes touch CI, agent config, or git hooks; push needs developer confirmation</div><div class="muted">Held file: <span class="mono">.github/workflows/ci.yml</span></div></div><div class="alert-actions"><button class="btn btn-ghost btn-sm">Discard</button><button class="btn btn-secondary btn-sm">Confirm push</button></div></div>', 'block') +
      spec('Alert / variant=failed', '<div class="alert alert-failed" style="width:560px">' + ic('circle-x') + '<div><div class="alert-title">Run failed</div><div class="muted">the daemon restarted while this run was active; resume to continue the session</div></div></div>', 'block') +
      spec('Alert / variant=info', '<div class="alert alert-info" style="width:560px">' + ic('shield-question') + '<div><div class="alert-title">Fix Notes are from an untrusted author; confirm via Start fix</div></div></div>', 'block'));
  }
  function card() {
    return sec('card', 'Card', '',
      spec('Card / header, content, footer', '<div class="card" style="width:420px"><div class="card-header"><div class="card-title">Custom App</div><div class="card-description">From the Lark Developer Console.</div></div><div class="card-content"><div class="field"><label class="label">App ID</label><input class="input mono" value="cli_a6f2c81d93b8d00e"></div></div><div class="card-footer"><button class="btn btn-ghost">Reset</button><button class="btn btn-default">Save</button></div></div>'));
  }
  function overlay() {
    return sec('overlay', 'Dialog, Alert Dialog', 'Native dialog element in the product (focus trap, Esc to close). Shown here inline.',
      spec('Dialog / StartTask', '<div class="dialog-static"><div class="dialog-header"><span class="mono muted">PGS-1402</span><div class="dialog-title" style="margin-top:6px">Invoice list: filter by payment status and date range</div><p class="dialog-description">Mandor claims the task, creates a worktree, starts an agent, and moves Lark to In Progress.</p></div><div class="dialog-body"><div class="field"><label class="label">Repo</label><select class="select"><option>pegasus-web · /home/beni/work/pegasus-web</option></select><p class="field-help">Last used for this task: pegasus-web.</p></div></div><div class="dialog-footer"><span class="left"><span class="kbd">Esc</span> to cancel</span><button class="btn btn-ghost">Cancel</button><button class="btn btn-default">' + ic('play') + 'Start</button></div></div>', 'block') +
      spec('AlertDialog / destructive', '<div class="dialog-static" style="max-width:440px"><div class="dialog-header"><div class="dialog-title">Close the run for PGS-1359?</div><p class="dialog-description">Mandor stops the agent and removes the worktree. The branch stays on origin.</p></div><div class="dialog-footer" style="margin-top:16px"><button class="btn btn-ghost">Cancel</button><button class="btn btn-destructive">Close run</button></div></div>', 'block'));
  }
  function menu() {
    return sec('menu', 'Dropdown Menu, Command, Tooltip', 'A tooltip never carries the only copy of a reason: disabled Start shows the reason inline on the card too.',
      spec('DropdownMenu', '<div class="popover popover-static"><button class="menu-item">' + ic('external-link') + 'Open in Lark</button><button class="menu-item">' + ic('copy') + 'Copy task key</button><button class="menu-item">' + ic('git-branch') + 'Copy branch name</button><div class="menu-sep"></div><button class="menu-item danger">' + ic('circle-x') + 'Close run</button></div>') +
      spec('Command / dialog', '<div class="dialog-static" style="max-width:480px"><div class="command-input">' + ic('search') + '<input placeholder="Type a task key, a title, or a command" aria-label="Search" value="1391"><span class="kbd">Esc</span></div><div class="command-list"><div class="command-group caps muted">Tasks</div><div class="command-item" aria-selected="true">' + ic('bot') + '<span>PGS-1391  Invoice detail: show partial payments</span><span class="hint">Waiting for you</span></div><div class="command-group caps muted">Actions</div><div class="command-item">' + ic('play') + '<span>Start PGS-1402</span><span class="hint">Invoice list: filter…</span></div></div></div>') +
      spec('Tooltip', '<span class="tooltip">Open PR #412 on GitHub</span>'));
  }
  function feedback() {
    return sec('feedback', 'Sonner, Skeleton, Spinner', '',
      spec('Sonner / toast', '<div class="toast" style="width:340px">' + ic('git-pull-request') + '<div><div>PR #418 created</div><div class="t-desc">PGS-1391 moved to Waiting Review in Lark</div></div></div>') +
      spec('Skeleton / card', '<div class="task-card" style="width:272px"><div class="skeleton" style="height:10px;width:30%"></div><div class="skeleton" style="height:12px;width:90%"></div><div class="skeleton" style="height:12px;width:60%"></div></div>') +
      spec('Spinner', ic('loader-circle', 'spin') + '<span class="muted">Syncing with Lark…</span>'));
  }

  /* ---------- Mandor ---------- */
  function runstatus() {
    return sec('runstatus', 'RunStatusBadge', 'Icon + label for every run state. Only Waiting for you is a filled chip.',
      Object.keys(M.runStates).filter(function (s) { return s !== 'stopped'; }).map(function (s) { return spec('state=' + s, U.runBadge(s)); }).join('') +
      spec('dot (sidebar, attention pulses 3x)', U.dotFor('awaiting_dev') + U.dotFor('running') + U.dotFor('wf_quiet') + U.dotFor('failed') + U.dotFor('orphaned')));
  }
  function flags() {
    var ex = { overwritten: 'Status in Lark is now In Review, not In Progress', sync_failed: 'Lark returned 429 (rate limited) while moving the status to Waiting Review', deleted: 'The record was deleted from Sprint Tasks', unassigned: 'You were removed from Assignee in Lark by Ayu Lestari' };
    return sec('flags', 'CardFlagBadge', 'Sync flags with the reason. Actions (Retry, Close run) sit in the card footer.',
      Object.keys(M.flags).map(function (k) { return spec('flag=' + k + (M.flags[k].action ? ' · action=' + M.flags[k].action : ''), '<div style="width:272px">' + U.flagBadge({ type: k, reason: ex[k] }) + '</div>'); }).join(''));
  }
  var VARIANTS = [['PGS-1402', 'ready to start'], ['PGS-1405', 'blocked by readiness'], ['PGS-1398', 'owned by another dev, read-only'], ['PGS-1390', 'take over available'],
    ['PGS-1388', 'working'], ['PGS-1391', 'waiting for you'], ['PGS-1384', 'creating PR'], ['PGS-1380', 'waiting for review, PR'], ['PGS-1366', 'waiting for reviewer comments'],
    ['PGS-1371', 'quiet period'], ['PGS-1362', 'fixing'], ['PGS-1379', 'failed'], ['PGS-1376', 'parked: claim lost'], ['PGS-1395', 'overwritten by Lark'],
    ['PGS-1393', 'sync failed'], ['PGS-1359', 'deleted in Lark'], ['PGS-1385', 'no longer assigned'], ['PGS-1368', 'unknown status'], ['PGS-1355', 'done']];
  function taskcard() {
    return sec('taskcard', 'TaskCard', 'No drag handle and draggable=false: cards move only through actions or Lark. Branch name shows on hover.',
      VARIANTS.map(function (v) { return spec('TaskCard / ' + v[1], U.taskCard(M.baseTask(v[0])), 'card-w'); }).join(''));
  }
  function agentrow() {
    var rows = [['PGS-1391', 'variant=attention'], ['PGS-1388', 'state=running'], ['PGS-1393', 'flag=sync_failed'], ['PGS-1380', 'state=in_review_loop']];
    return sec('agentrow', 'AgentRow', 'Sidebar row. Below 1100px wide the sidebar collapses to the dot and the key number (see Board at 960 in the index).',
      '<div class="spec"><div class="spec-body"><div class="sidebar" style="width:256px;border:1px solid var(--sidebar-border);border-radius:var(--radius);padding:8px 0">' +
        '<div class="sidebar-group-label caps" style="padding-top:4px"><span class="label-text">Waiting for you</span><span class="count mono">1</span></div>' +
        '<div style="display:grid;gap:1px;padding:0 8px">' + rows.map(function (r, i) { return U.agentRow(M.baseTask(r[0]), i === 1); }).join('') + '</div></div></div>' +
        '<div class="spec-label">AgentRow / attention, selected (aria-current), flag, waiting</div></div>');
  }
  function chat() {
    var msgs = [
      { type: 'system', icon: 'git-branch', text: 'Run started on beni/pgs-1391-partial-payments from origin/main' },
      { type: 'dev', text: 'Use the Timeline component from the leave history page if it fits.' },
      { type: 'agent', html: '<p>The payments timeline is on the invoice detail page. <code>PaymentTimeline.svelte</code> wraps the existing <code>Timeline</code>.</p>' },
      { type: 'turn', text: 'Turn finished', dur: '4m 12s', tok: '38.2k in · 6.1k out' },
      { type: 'review', author: 'sari', time: '10:02', trusted: true, text: 'Overlap check ignores night shifts that cross midnight.' },
      { type: 'system', icon: 'circle-x', text: 'Run interrupted: the daemon restarted at 09:58', failed: true },
      { type: 'turn', text: 'Turn failed', dur: '1m 03s', tok: '9.4k in · 0.8k out', failed: true }
    ];
    var labels = ['type=system', 'from=developer', 'from=agent (markdown)', 'type=turn-finished', 'type=reviewer-comment', 'type=system, failed', 'type=turn-failed'];
    return sec('chat', 'ChatMessage', '', msgs.map(function (m, i) { return spec('ChatMessage / ' + labels[i], '<div class="chat-inner" style="width:620px;padding:0">' + U.chatMessage(m) + '</div>', 'block'); }).join(''));
  }
  function tools() {
    var groups = [
      ['state=ok', { type: 'tools', calls: [{ tool: 'Read', prev: 'src/lib/components/Timeline.svelte', s: 'ok', out: '88 lines' }, { tool: 'Grep', prev: '"formatRupiah" in src/lib', s: 'ok', out: 'src/lib/format.ts:12' }] }],
      ['state=error', { type: 'tools', calls: [{ tool: 'Bash', prev: 'cargo test overtime', s: 'error', out: 'test overtime::tests::half_hour_rounds_up ... FAILED' }] }],
      ['state=denied', { type: 'tools', calls: [{ tool: 'Bash', prev: 'git push origin beni/pgs-1391-partial-payments', s: 'denied', reason: '`git push` is run only by the daemon. Use Push & create PR in Mandor.' }] }],
      ['state=live', { type: 'tools', live: true, calls: [{ tool: 'Edit', prev: 'src/schedule/overlap.rs', s: 'ok', out: '' }] }]
    ];
    return sec('tools', 'ToolCallGroup', 'Consecutive tool calls collapse into one block. Errored and denied calls open by default.',
      groups.map(function (g) { return spec('ToolCallGroup / ' + g[0], '<div style="width:620px">' + U.toolGroup(g[1]) + '</div>', 'block'); }).join(''));
  }
  function diff() {
    var f = WS.diffFor(M.baseTask('PGS-1391'));
    var rows = f[4].rows.map(function (r) { var c = { h: 'hunk', c: 'ctx', a: 'add', d: 'del' }[r[0]]; var s = { a: '+', d: '−', c: ' ', h: '' }[r[0]];
      return r[0] === 'h' ? '<tr class="hunk"><td class="ln"></td><td class="ln"></td><td class="code">' + esc(r[3]) + '</td></tr>' : '<tr class="' + c + '"><td class="ln">' + r[1] + '</td><td class="ln">' + r[2] + '</td><td class="code"><span class="sign">' + s + '</span>' + esc(r[3]) + '</td></tr>'; }).join('');
    return sec('diff', 'DiffViewer', 'File status letters (A, M, D, R, U) are outlined and labelled, not color only.',
      spec('DiffViewer / file list + unified diff', '<div class="frame-box changes" style="width:820px;height:auto"><div class="filelist">' +
        f.map(function (x, i) { return '<button class="file-row" aria-selected="' + (i === 4) + '"><span class="fs" data-s="' + x.s + '">' + x.s + '</span><span class="fp">' + esc(x.path) + '</span><span class="fc">' + (x.sensitive ? ic('shield-alert', 'icon-sm') + ' ' : '') + '<span class="add-c">+' + x.add + '</span> <span class="del-c">−' + x.del + '</span></span></button>'; }).join('') +
        '</div><div class="diff"><div class="diff-file-head">' + ic('file-code', 'icon-sm') + '<span>.github/workflows/ci.yml</span></div><table>' + rows + '</table></div></div>', 'block'));
  }
  function composer() {
    function box(ph, actions, queued) {
      return '<div class="composer" style="width:620px;margin:0">' + (queued ? '<div class="queued"><span class="q-chip">' + ic('clock', 'icon-sm') + '<span>Also add a test for the empty state</span><button class="btn btn-ghost btn-xs btn-icon" aria-label="Remove">' + ic('x', 'icon-sm') + '</button></span></div>' : '') +
        '<div class="composer-box"><textarea rows="2" placeholder="' + ph + '"' + (actions ? '' : ' disabled') + '></textarea><div class="composer-bar"><span class="hint"><span class="kbd">Ctrl ↵</span> send</span><span class="right">' + (actions || '<button class="btn btn-secondary btn-sm" disabled>' + ic('send-horizontal') + 'Send</button>') + '</span></div></div></div>';
    }
    return sec('composer', 'Composer', 'Send becomes Queue while the agent works. Send now stops the turn first.',
      spec('Composer / state=idle (awaiting_dev)', box('Reply to the agent. Drafts are kept per task.', '<button class="btn btn-secondary btn-sm">' + ic('send-horizontal') + 'Send</button>'), 'block') +
      spec('Composer / state=working, queued', box('Queue a message for when this turn ends', '<button class="btn btn-ghost btn-sm">' + ic('square') + 'Stop</button><button class="btn btn-outline btn-sm">' + ic('zap') + 'Send now</button><button class="btn btn-secondary btn-sm">' + ic('list-plus') + 'Queue</button>', true), 'block') +
      spec('Composer / state=disabled (with reason)', box('The agent is idle while PR #412 waits for review. Feedback goes through Start fix.', null), 'block'));
  }
  function steps() {
    function st(s, label, err) { var i = { todo: 'circle', active: 'loader-circle', done: 'circle-check', error: 'circle-x' }[s]; return '<div class="step" data-s="' + s + '">' + ic(i, s === 'active' ? 'spin' : '') + '<span>' + label + (err ? '<span class="err">' + err + '</span>' : '') + '</span></div>'; }
    return sec('steps', 'StepProgress', 'Inline progress for Start and Take over, with the error on the step that failed.',
      spec('StepProgress / running', '<div class="steps" style="width:360px">' + st('done', 'Claiming the task in Lark') + st('active', 'Creating worktree') + st('todo', 'Starting the agent') + '</div>') +
      spec('StepProgress / error', '<div class="steps" style="width:360px">' + st('error', 'Claiming the task in Lark', 'Lost the claim: another developer started this task at the same time.') + st('todo', 'Creating worktree') + '</div>'));
  }
  function comment() {
    var cs = [[{ author: 'sari', time: '09:41', trusted: true, sent: true, text: 'Searching "Budi" finds nobody whose nickname is Budi.' }, 'trusted, sent'],
      [{ author: 'sari', time: '09:42', trusted: true, sent: false, text: 'Results should highlight which field matched.' }, 'trusted, not sent yet'],
      [{ author: 'rizky', time: '08:55', trusted: false, sent: false, text: 'Maybe also round to the nearest thousand before comparing?' }, 'not trusted']];
    return sec('comment', 'CommentItem', 'Lark record comments. Untrusted ones are only shown to the developer.',
      cs.map(function (c) { var x = c[0], p = M.people[x.author];
        return spec('CommentItem / ' + c[1], '<article class="comment" style="width:560px"><div class="head">' + U.avatar(x.author) + '<b>' + esc(p.name) + '</b><span class="time">' + x.time + '</span><span class="right"><span class="badge trust" data-trusted="' + x.trusted + '">' + ic(x.trusted ? 'shield-check' : 'shield-question') + (x.trusted ? 'Trusted' : 'Not trusted') + '</span><span class="badge badge-secondary">' + (x.sent ? ic('send-horizontal') + 'Sent to agent' : ic('clock') + (x.trusted ? 'Not sent yet' : 'Only shown to you')) + '</span></span></div><div>' + esc(x.text) + '</div></article>', 'block');
      }).join('') +
      spec('QuietPeriod', '<div class="quiet" style="width:560px">' + ic('timer') + '<div style="flex:1"><b>Quiet period</b><div class="muted" style="font-size:12px">Waiting for more comments before forwarding trusted ones.</div><div class="bar"><i style="width:14%"></i></div></div><span class="time">1:44</span></div>', 'block'));
  }
  function empty() {
    return sec('empty', 'Empty state', 'Icon, one-line title, one sentence, one action.',
      spec('EmptyState / filter hides everything', '<div class="frame-box" style="width:420px"><div class="empty" style="padding:28px 20px"><span class="ico">' + ic('search-x') + '</span><h3>No cards match "overtime pdf"</h3><p>The search and filters are hiding every card on the board.</p><button class="btn btn-outline">Clear filters</button></div></div>'));
  }

  /* ---------- reserved slots (prompt section 8: reserve space, not designed in detail) ---------- */
  function reserved() {
    var cardHtml = U.taskCard(M.baseTask('PGS-1402')).replace('<div class="tc-foot">',
      '<div class="tc-meta"><span class="badge" data-ui="Badge / custom=OvertimeQueue">' + ic('moon-star') + 'Overtime queue · #2</span></div><div class="tc-foot">');
    var tabsHtml = '<div class="tabs-list" role="tablist" style="padding:0;width:640px">' +
      [['messages-square', 'Chat'], ['file-diff', 'Changes'], ['list-checks', 'Task'], ['message-square-text', 'Review comments']].map(function (x, i) { return '<button class="tabs-trigger" role="tab" aria-selected="' + (i === 0) + '">' + ic(x[0]) + x[1] + '</button>'; }).join('') +
      '<button class="tabs-trigger" role="tab" aria-selected="false" aria-disabled="true" style="opacity:.55;cursor:default">' + ic('clipboard-check') + 'AC review <span class="count">R2</span></button></div>';
    var sidebarHtml = '<div class="sidebar" style="width:256px;border:1px solid var(--sidebar-border);border-radius:var(--radius);padding:0 0 8px">' +
      '<nav class="sidebar-nav"><a class="nav-item">' + ic('square-kanban') + '<span class="label-text">Board</span></a><a class="nav-item" aria-disabled="true" style="opacity:.55">' + ic('moon-star') + '<span class="label-text">Overtime queue</span></a><a class="nav-item" aria-disabled="true" style="opacity:.55">' + ic('sunrise') + '<span class="label-text">Morning report</span></a><a class="nav-item">' + ic('settings-2') + '<span class="label-text">Settings</span></a></nav>' +
      '<hr class="separator"><div class="sidebar-group-label caps"><span class="label-text">Overtime tonight</span><span class="count mono">3</span></div>' +
      '<div style="padding:0 16px"><div class="skeleton" style="height:28px;animation:none"></div></div></div>';
    var acHtml = '<div class="ac-list" style="width:560px">' +
      [['AC-5521', 'pass', 'Timeline lists payments newest first.'], ['AC-5522', 'pass', 'Each row shows the remaining balance.'], ['AC-5525', 'fail', 'Amounts use id-ID formatting.']].map(function (a) {
        return '<div class="ac" style="grid-template-columns:64px 1fr auto"><span class="id">' + a[0] + '</span><span class="txt">' + a[2] + '</span><span class="run" data-tone="' + (a[1] === 'pass' ? 'ok' : 'failed') + '">' + ic(a[1] === 'pass' ? 'circle-check' : 'circle-x') + '<span class="lbl">' + (a[1] === 'pass' ? 'Pass' : 'Fail') + '</span></span></div>';
      }).join('') + '</div>';
    return sec('reserved', 'Slots for later releases', 'Space kept free for AI Lembur overtime agents (Release 3). The Release 2 slots below are now designed: see the Release 2 group and the AC review tab. Not shown in the Phase 1 screens.',
      spec('TaskCard / tc-meta slot · Badge / custom=OvertimeQueue (R3)', cardHtml, 'card-w') +
      spec('Sidebar / nav slots: Overtime queue, Morning report · group "Overtime tonight" (R3)', sidebarHtml) +
      spec('Tabs / 5th tab: AC review (R2)', tabsHtml, 'block') +
      spec('AC report row: AC id + pass/fail with icon and label (R2)', acHtml, 'block'));
  }

  /* ---------- Phase 2 components ---------- */
  function stepper() {
    function steps(cur) {
      var names = ['Welcome', 'Prerequisites', 'Lark app', 'Log in', 'Repo', 'Done'];
      return '<ol class="stepper" style="width:640px;margin:0">' + names.map(function (n, i) {
        var st = i + 1 < cur ? 'done' : i + 1 === cur ? 'current' : 'todo';
        return (i ? '<li aria-hidden="true" class="sep"></li>' : '') + '<li data-s="' + st + '"' + (st === 'current' ? ' aria-current="step"' : '') + '><span class="num">' + (st === 'done' ? ic('check', 'icon-sm') : i + 1) + '</span><span class="lbl">' + n + '</span></li>';
      }).join('') + '</ol>';
    }
    return sec('stepper', 'Stepper', 'Onboarding progress built from Badge and Separator. Done steps show a check; the current step has aria-current="step". Below 1100px only the current label is shown.',
      spec('Stepper / current=1', steps(1), 'block') + spec('Stepper / current=4', steps(4), 'block'));
  }
  function notif() {
    var list = M.notificationsRaw.map(function (n, i) { return Object.assign({ read: i > 3 }, n); });
    return sec('notif', 'NotificationCenter', 'Bell in the title bar with an unread count, and a Popover list. Unread rows are tinted, bold, and carry a dot; the icon tile shows the kind. Same renderer as the app.',
      spec('Button / custom=Bell · unread=4', '<span class="titlebar" style="border:1px solid var(--sidebar-border);border-radius:var(--radius);padding:0 8px;height:40px"><button class="btn btn-ghost btn-sm btn-icon bell" aria-label="Notifications, 4 unread">' + ic('bell') + '<span class="bell-count mono" aria-hidden="true">4</span></button><button class="btn btn-ghost btn-sm btn-icon bell" aria-label="Notifications">' + ic('bell') + '</button></span>') +
      spec('Popover / NotificationCenter · mixed read', '<div class="popover notif popover-static" style="width:380px">' + U.notifCenterHtml(list) + '</div>') +
      spec('Popover / NotificationCenter · empty', '<div class="popover notif popover-static" style="width:380px">' + U.notifCenterHtml([]) + '</div>'));
  }
  function tray() {
    var mark = '<span class="brand-mark" aria-hidden="true"><i></i><i></i><i></i></span>';
    var waiting = ['PGS-1391', 'PGS-1388'].map(function (k) { return M.baseTask(k); });
    function menu(items) {
      return '<div class="popover tray-menu popover-static" role="menu" style="width:300px">' +
        '<button class="menu-item" role="menuitem">' + ic('app-window') + 'Open Mandor</button><div class="menu-sep"></div>' + items +
        '<div class="menu-sep"></div><button class="menu-item" role="menuitem">' + ic('power') + 'Quit Mandor</button></div>';
    }
    return sec('tray', 'TrayMenu, tray icon', 'Native menu drawn by the OS; Mandor supplies the items. Full behaviour in tray.html.',
      spec('TrayMenu / waiting=2', menu('<div class="hdr caps muted">Waiting for you (2)</div>' + waiting.map(function (t) { return '<button class="menu-item" role="menuitem"><span class="k">' + t.key + '</span><span class="t">' + esc(t.title) + '</span></button>'; }).join(''))) +
      spec('TrayMenu / waiting=0', menu('<div class="menu-item" role="menuitem" aria-disabled="true">' + ic('check') + 'No agents waiting for you</div>')) +
      spec('TrayIcon / idle, attention', '<span class="taskbar-mini" style="display:flex;gap:10px;padding:6px 10px;border-radius:var(--radius);background:var(--sidebar);border:1px solid var(--sidebar-border)"><span class="tray-icon" style="cursor:default">' + mark + '</span><span class="tray-icon" data-attention="true" style="cursor:default">' + mark + '</span></span>'));
  }

  /* ---------- Release 2 (AC review) ---------- */
  var A = U.ac, L = window.AC_DATA.scenario('current');
  function acRow(r, o) { return '<div class="ac-ledger" role="grid" style="width:100%">' + A.row(r, o || {}) + '</div>'; }
  function acchips() {
    var d = Object.keys(A.DESIGN).map(function (k) { return A.chip('design', k); }).join(' ');
    var v = Object.keys(A.VERDICT).map(function (k) { return A.chip('verdict', k); }).join(' ');
    return sec('acchips', 'DesignChip, VerdictChip', 'Icon plus label for every value, never colour alone. Stale keeps the old verdict, dashed, with a Stale marker. Without a design source the Design column shows No design source once, in the column header.',
      spec('Badge / custom=DesignChip · all values', '<div style="display:flex;gap:6px;flex-wrap:wrap;max-width:720px">' + d + ' <span class="chip" data-tone="muted">' + ic('frame') + 'No design source</span></div>', 'block') +
      spec('Badge / custom=VerdictChip · all values', '<div style="display:flex;gap:6px;flex-wrap:wrap;max-width:720px">' + v + '</div>', 'block') +
      spec('VerdictChip / stale, reviewing', '<div style="display:flex;gap:6px;flex-wrap:wrap">' + A.chip('verdict', 'pass', { stale: true }) + A.chip('verdict', 'fail', { stale: true }) + A.chip('verdict', 'not_reviewed', { pending: true }) + '</div>') +
      spec('Tabs / AC review count', '<div class="tabs-list" style="padding:0;border:0"><button class="tabs-trigger" aria-selected="true">' + ic('clipboard-check') + 'AC review <span class="count">' + A.tabCount(L) + '</span></button><button class="tabs-trigger">' + ic('clipboard-check') + 'AC review <span class="count">' + A.tabCount(window.AC_DATA.scenario('allpass')) + '</span></button><button class="tabs-trigger">' + ic('clipboard-check') + 'AC review <span class="count">' + A.tabCount(window.AC_DATA.scenario('reviewing')) + '</span></button></div>'));
  }
  function acrow() {
    var r = L.rows, ch = window.AC_DATA.scenario('changed').rows, st = window.AC_DATA.scenario('stale').rows, pl = window.AC_DATA.scenario('plan').rows;
    var fix = JSON.parse(JSON.stringify(r[2])); fix.fixing = true;
    var sk = JSON.parse(JSON.stringify(r[2])); sk.verdict = { v: 'skipped', reason: 'Handled in PGS-1431', why: 'Skipped with Push anyway: handled in PGS-1431, agreed with the PM.' };
    var mn = JSON.parse(JSON.stringify(r[3])); mn.verdict = { v: 'manual', why: 'Checked manually by Beni Handoko.' };
    return sec('acrow', 'ACLedgerRow', 'One row per AC: label, two-line text, Design, Verdict. Arrow keys move between rows, Space selects a Fail row, Enter expands. Only Fail rows have a checkbox; Follows design and Pass rows cannot be sent to the agent.',
      spec('ACLedgerRow / Pass · expanded (evidence: files, test, browser steps, design location)', acRow(r[0], { expanded: true }), 'block') +
      spec('ACLedgerRow / Fail · selected', acRow(r[2], { selected: true }), 'block') +
      spec('ACLedgerRow / Fail · expanded', acRow(r[2], { expanded: true }), 'block') +
      spec('ACLedgerRow / Fix in progress', acRow(fix), 'block') +
      spec("ACLedgerRow / Can't verify", acRow(r[3], { expanded: true }), 'block') +
      spec('ACLedgerRow / Checked manually, Skipped', acRow(mn) + acRow(sk), 'block') +
      spec('ACLedgerRow / Conflicts with design (plan)', acRow(pl[4], { expanded: true }), 'block') +
      spec('ACLedgerRow / Follow design · Follows design', acRow(r[4]), 'block') +
      spec('ACLedgerRow / Stale', acRow(st[0]), 'block') +
      spec('ACLedgerRow / Changed, New', acRow(ch[2]) + acRow(ch[7]), 'block') +
      spec('ACLedgerRow / no design source', acRow(Object.assign({}, r[1], { design: null }), { noSource: true }), 'block') +
      spec('ACLedgerRow / Removed in Lark', '<div class="ac-ledger" style="width:100%">' + A.removedRow(L.removed[0]) + '</div>', 'block'));
  }
  function acsummary() {
    var sc = window.AC_DATA.scenario;
    return sec('acsummary', 'ACReviewSummary', 'Counts per verdict, the gate state, last review and trigger, the design source, and Run review. Run review is disabled with the reason while the agent works.',
      ['current', 'design', 'reviewing', 'stale', 'allpass', 'warn', 'nodesign'].map(function (k) { return spec('ACReviewSummary / ' + k, '<div style="width:100%">' + A.summary(sc(k)) + '</div>', 'block'); }).join('') +
      spec('ACReviewSummary / agent working', '<div style="width:100%">' + A.summary(sc('design'), { busy: true }) + '</div>', 'block'));
  }
  function gatebtn() {
    var sc = window.AC_DATA.scenario;
    var list = [['ready', 'allpass'], ['review needed', 'design'], ['reviewing', 'reviewing'], ['blocked', 'current'], ['warn', 'warn']];
    var nc = sc('current'); nc.rows[2].verdict = window.AC_DATA.fixedAc3;
    return sec('gatebtn', 'GateButton', 'Push & create PR follows the AC ledger. Review needed starts a review and stops there: the push does not continue by itself. Blocked opens a popover with the failing AC. Warn mode keeps the button enabled with a warning count.',
      list.map(function (x) { return spec('Button / custom=GateButton / state=' + x[0], A.gateButton(sc(x[1]))); }).join('') +
      spec('Button / custom=GateButton / state=needs confirmation', A.gateButton(nc)) +
      spec('Popover / GateBlocked', '<div class="popover gate-pop popover-static" style="width:360px"><div class="gp-head">' + ic('lock') + '<b>Push blocked by 1 AC</b></div><div class="gp-row"><span class="mono">AC-3</span><span>' + esc(L.rows[2].verdict.why) + '</span></div><div class="gp-act"><button class="btn btn-ghost btn-sm">View in AC review</button><button class="btn btn-outline btn-sm">Push anyway…</button><button class="btn btn-default btn-sm">' + ic('send-horizontal') + 'Send to agent</button></div></div>'));
  }
  function runapp() {
    return sec('runapp', 'RunAppControl', 'Action bar control for the local app. Only one run’s app runs at a time; the owner is named when another run takes over.',
      Object.keys(A.APP).map(function (k) { return spec('RunAppControl / ' + k, A.runApp(k)); }).join(''));
  }
  function readiness() {
    return sec('readiness', 'ReadinessItem', 'Checklist row in the readiness dialog shown before a browser review. Mandor cannot see the database or the login, so the developer confirms them.',
      spec('ReadinessItem / ok, confirm, warn, missing', '<ul class="ready-list" style="width:520px">' +
        A.readyItem({ state: 'ok', title: 'App is running from the latest code', help: '<span class="mono">http://localhost:5173</span>' }) +
        A.readyItem({ state: 'confirm', title: 'Logged in at <span class="mono">http://localhost:5173</span>', help: 'Log in with your own account in Chrome. Mandor never receives your password.', action: '<label class="ri-check"><input type="checkbox" class="checkbox"> I\'m logged in</label>' }) +
        A.readyItem({ state: 'warn', title: 'App is running older code', help: 'The code changed after the app started.', action: '<button class="btn btn-outline btn-xs">' + ic('rotate-ccw') + 'Restart app</button>' }) +
        A.readyItem({ state: 'missing', title: 'Claude in Chrome extension not connected', help: 'Open Chrome and enable the extension, then check again.', action: '<button class="btn btn-outline btn-xs">' + ic('refresh-cw') + 'Check again</button>' }) + '</ul>'));
  }
  function sources() {
    return sec('sources', 'DesignSourceItem', 'One concept, several kinds. Code repo or folder ships in 2.0. Figma, deployed URL, and images or PDFs are 2.1: dashed border and a 2.1 tag so they stay separable.',
      spec('DesignSourceItem / kind=repo (2.0)', '<div style="width:560px;display:grid;gap:6px">' + A.sourceItem({ kind: 'repo', name: 'kelola-design', location: '/home/beni/work/kelola-design' }) + A.sourceItem({ kind: 'repo', name: 'pegasus-mock', location: '/home/beni/work/pegasus-mock', status: 'error', error: 'Path missing' }) + '</div>', 'block') +
      spec('DesignSourceItem / kinds for 2.1', '<div style="width:560px;display:grid;gap:6px">' + A.sourceItem({ kind: 'figma', name: 'Kelola v2 screens', location: 'https://www.figma.com/design/…/Kelola-v2', status: 'untested' }) + A.sourceItem({ kind: 'url', name: 'Kelola prototype', location: 'https://proto.kelola.example.com', status: 'error', error: 'URL unreachable' }) + A.sourceItem({ kind: 'images', name: 'PO flow exports', location: '/home/beni/designs/po-flow', status: 'ok' }) + '</div>', 'block'));
  }

  function plangaps() {
    var l = window.AC_DATA.scenario('plan');
    var gp = function (r, followed, approved) {
      var orig = r.design.v;
      return '<div class="plan-gaps" style="width:100%"><div class="pg-row"' + (followed ? ' data-follow="true"' : '') + '><span class="mono pg-id">' + esc(r.id) + '</span><div class="pg-txt"><div>' + esc(r.text) + '</div><div class="pg-meta">' + A.chip('design', orig) + (r.design.note ? '<span class="muted">' + esc(r.design.note) + '</span>' : '') + '</div>' +
        (followed ? '<div class="pg-follow">' + ic('signpost-big', 'icon-sm') + '<span>The agent follows the design for ' + esc(r.id) + '. This AC leaves the gate, and the PM is asked to update it in Lark.</span></div>' : '') + '</div><div class="pg-act">' +
        (approved ? (followed ? A.chip('design', 'follow') : '<span class="muted pg-default">Build from the AC</span>') : followed ? '<button class="btn btn-ghost btn-sm">' + ic('undo-2') + 'Undo</button>' : '<button class="btn btn-outline btn-sm">' + ic('signpost-big') + 'Follow design</button>') + '</div></div></div>';
    };
    return sec('plangaps', 'PlanTab design gaps', 'Rows the design is missing, shows partly, or contradicts. Follow design is offered on each and can be undone until the plan is approved.',
      spec('DesignGapRow / Missing', gp(l.rows[2]), 'block') + spec('DesignGapRow / Conflicts', gp(l.rows[4]), 'block') +
      spec('DesignGapRow / Follow design (before approval)', gp(l.rows[4], true), 'block') + spec('DesignGapRow / approved', gp(l.rows[4], true, true) + gp(l.rows[2], false, true), 'block'));
  }
  function r2board() {
    var k = ['PGS-1429', 'PGS-1431', 'PGS-1433'].map(function (x) { return M.baseTask(x); });
    var stale = JSON.parse(JSON.stringify(k[2])); stale.acs = { ok: 4, total: 4, stale: true };
    var rev = JSON.parse(JSON.stringify(k[2])); rev.acs = { ok: 2, total: 4, reviewing: true }; rev.run.state = 'reviewing';
    var plan = M.baseTask('PGS-1429'); plan = JSON.parse(JSON.stringify(plan)); plan.run.state = 'plan_ready'; plan.acs = { gaps: 3, reviewed: false };
    var rows = [k[0], k[1], k[2], rev, plan].map(function (t) { return U.agentRow(t); }).join('');
    var owner = function (st, own) { var a = { running: ['ok', 'running'], starting: ['working', 'starting'], older: ['warning', 'older code'], failed: ['failed', 'failed to start'] }[st];
      return '<a class="app-owner" data-tone="' + a[0] + '">' + ic('app-window', 'icon-sm') + '<span class="conn-text">App · </span><span class="mono">' + own + '</span><span class="conn-text">' + a[1] + '</span></a>'; };
    return sec('r2board', 'Board, sidebar, title bar', 'AC summary chip on cards with a run (icon + text), design-gap marker on In Progress cards, AC labels in the sidebar, and the owner of the one running app in the title bar.',
      [[k[0], '2 failed + design gaps'], [k[1], 'Push held'], [k[2], "Can't verify"], [stale, 'Review stale'], [rev, 'Reviewing'], [plan, 'Plan ready + gaps']].map(function (v) { return spec('TaskCard / ' + v[1], U.taskCard(v[0]), 'card-w'); }).join('') +
      spec('AgentRow / Release 2 labels', '<div class="sidebar" style="width:256px;border:1px solid var(--sidebar-border);border-radius:var(--radius);padding:6px">' + rows + '</div>') +
      spec('Badge / custom=AppOwner', '<div style="display:flex;gap:6px;flex-wrap:wrap">' + owner('running', 'PGS-1426') + owner('starting', 'PGS-1426') + owner('older', 'PGS-1426') + owner('failed', 'PGS-1426') + '</div>'));
  }
  function logs() {
    return sec('logs', 'AppLogs drawer', 'Bottom Sheet in the workspace with one tab per repo; opens on its own after a failed start. Plain text.',
      spec('Sheet / side=bottom / AppLogs · failed', '<section class="logs-drawer" style="width:720px;border:1px solid var(--border);border-radius:var(--radius)"><div class="ld-head"><b>' + ic('scroll-text', 'icon-sm') + 'App logs</b><div class="toggle-group"><button aria-pressed="false">kelola-api</button><button aria-pressed="true">kelola-web ' + ic('circle-x', 'icon-sm') + '</button></div><span class="muted ld-meta">Failed to start · kelola-web</span><button class="btn btn-ghost btn-xs">' + ic('copy') + 'Copy</button><button class="btn btn-ghost btn-xs btn-icon" aria-label="Close logs">' + ic('x') + '</button></div><pre class="ld-body">$ bun run dev\nerror: Cannot find module "./lib/po/RejectDialog.svelte" from "src/routes/po/[id]/+page.svelte"\nerror: script "dev" exited with code 1</pre></section>', 'block'));
  }

  /* ---------- plan by day, queue for VM (plan 2026-10-30-001) ---------- */
  function planvm() {
    var both = '<div class="alert alert-info plan-ready" style="width:640px">' + ic('file-check') + '<div><div class="alert-title">Plan ready</div><div class="muted">Review the gaps and the plan, then approve to start implementing.</div></div><div class="alert-actions">' +
      '<button class="btn btn-outline btn-sm">' + ic('play') + 'Approve plan &amp; implement</button>' +
      '<button class="btn btn-outline btn-sm">' + ic('moon-star') + 'Approve &amp; queue for VM</button></div></div>';
    function pop(opts) {
      var sources = opts.sources.length ? opts.sources.map(function (n, i) {
        return '<label style="display:flex;gap:8px;align-items:center;padding:4px 0"><input type="checkbox" class="checkbox"' + (i === 0 && opts.picked ? ' checked' : '') + '><span>' + esc(n) + '</span></label>';
      }).join('') : '<p class="muted" style="margin:4px 0">No design sources on the VM. <a href="#">Add one in Settings › VM</a>.</p>';
      return '<div class="popover popover-static" style="width:340px;padding:14px">' +
        '<div class="field"><label class="label">Repo on the VM</label><select class="input select"><option>kelola-api</option></select></div>' +
        '<div class="field"><label class="label">Design sources on the VM</label>' + sources +
        '<div class="field-help">On the laptop the plan used: <b>kelola-mock</b></div></div>' +
        (opts.warn ? '<div class="alert alert-warning" style="margin-top:10px">' + ic('triangle-alert') + '<div class="muted">AC-4 follows the design. Pick a design source on the VM.</div></div>' : '') +
        '<div style="display:flex;justify-content:flex-end;gap:8px;margin-top:12px"><button class="btn btn-ghost btn-sm">Cancel</button><button class="btn btn-sm"' + (opts.warn ? ' disabled' : '') + '>' + ic('moon-star') + 'Queue</button></div></div>';
    }
    var steps = ['Pushing the branch…', 'Adding to the VM queue…', 'Handing over the card…'].map(function (t, i) {
      return '<div style="display:flex;gap:8px;align-items:center">' + ic(i < 2 ? 'circle-check' : 'loader-circle', 'icon-sm') + '<span' + (i < 2 ? ' class="muted"' : '') + '>' + t + '</span></div>';
    }).join('');
    function row(key, title, badge, extra, actions) {
      return '<div class="card" style="width:640px;padding:10px 12px;display:flex;gap:10px;align-items:center"><span class="mono">' + key + '</span>' + badge +
        '<div style="flex:1;min-width:0"><div class="truncate">' + esc(title) + '</div>' + (extra ? '<div class="muted" style="font-size:12px">' + extra + '</div>' : '') + '</div>' + actions + '</div>';
    }
    var planned = '<span class="badge">' + ic('list-checks') + 'Planned</span>';
    var noPlan = '<span class="badge badge-outline">No plan</span>';
    var moves = '<button class="btn btn-ghost btn-xs btn-icon" aria-label="Move up">' + ic('arrow-up') + '</button><button class="btn btn-ghost btn-xs btn-icon" aria-label="Move down">' + ic('arrow-down') + '</button><button class="btn btn-outline btn-xs">Start now</button>';
    var queue = '<div style="display:grid;gap:6px">' +
      row('PGS-1426', 'PO approval with reasons', planned, 'Design: kelola-mock', moves + '<button class="btn btn-outline btn-xs">' + ic('laptop') + 'Take over to laptop</button>') +
      row('PGS-1427', 'Export approved POs', noPlan, null, moves + '<button class="btn btn-ghost btn-xs btn-icon" aria-label="Remove">' + ic('x') + '</button>') +
      row('PGS-1428', 'Reject with a reason', planned, '<span style="color:var(--status-warning)">Claim not moved</span>: the laptop did not hand the card over.', '<button class="btn btn-outline btn-xs">' + ic('refresh-cw') + 'Retry handing over</button><button class="btn btn-ghost btn-xs">Remove from VM queue</button>') + '</div>';
    var held = row('PGS-1429', 'Audit entries for approvals', planned, 'Held: Design source "kelola-mock" is not available: Path missing. The plan is kept on the VM.', '<button class="btn btn-outline btn-xs">' + ic('laptop') + 'Take over to laptop</button>');
    var vmOff = '<div class="card" style="width:560px;padding:14px"><div style="display:flex;justify-content:space-between;align-items:center"><b>Design sources on the VM</b><button class="btn btn-outline btn-sm" disabled>' + ic('plus') + 'Add</button></div><p class="muted" style="margin:8px 0 0">VM not connected. Connect the VM in Settings › VM to manage its design sources.</p></div>';
    var vmEmpty = '<div class="card" style="width:560px;padding:14px"><div style="display:flex;justify-content:space-between;align-items:center"><b>Design sources on the VM</b><button class="btn btn-outline btn-sm">' + ic('plus') + 'Add</button></div><p class="muted" style="margin:8px 0 0">Folders on the VM, not on this laptop. A folder that is a git repo is updated (fast-forward only) before each card starts.</p></div>';
    return sec('planvm', 'Plan by day, queue for VM', 'Approve a plan on the laptop and queue it for the VM, or queue a card without a plan. The VM keeps its own design sources.',
      spec('PlanReady / two approve actions', both, 'block') +
      spec('QueueForVmPopover / sources', pop({ sources: ['kelola-mock', 'pegasus-mock'], picked: true })) +
      spec('QueueForVmPopover / no VM sources', pop({ sources: [] })) +
      spec('QueueForVmPopover / follow design needs a source', pop({ sources: ['kelola-mock'], warn: true })) +
      spec('QueueForVm / progress, step 3', '<div class="card" style="width:340px;padding:12px;display:grid;gap:6px">' + steps + '</div>') +
      spec('VM queue rows / Planned, No plan, Claim not moved', queue, 'block') +
      spec('Morning report / planned card held', held, 'block') +
      spec('Settings › VM / design sources, VM offline', vmOff, 'block') +
      spec('Settings › VM / design sources, empty', vmEmpty, 'block'));
  }

  /* ---------- grouped cards on the VM (plan 2026-10-23-004, U16) ---------- */
  function groupvm() {
    function pick(key, title, opts) {
      return '<label style="display:flex;gap:8px;align-items:center;padding:3px 0' + (opts.off ? ';opacity:.55' : '') + '"><input type="checkbox" class="checkbox"' + (opts.on ? ' checked' : '') + (opts.off ? ' disabled' : '') + '><span class="mono">' + key + '</span><span class="truncate" style="flex:1;min-width:0">' + esc(title) + '</span>' + (opts.off ? '<span class="muted" style="font-size:12px">' + opts.off + '</span>' : '') + '</label>';
    }
    var together = '<div class="card" style="width:640px;padding:12px;display:grid;gap:8px">' +
      '<div style="display:flex;gap:8px;align-items:center;flex-wrap:wrap"><select class="input select" style="width:260px"><option>PGS-1401 · Job list</option></select><select class="input select" style="width:150px"><option>kelola-api</option></select><button class="btn btn-outline btn-sm">' + ic('palette') + 'Design: kelola-mock</button></div>' +
      '<div><b style="font-size:13px">Queue together</b> <span class="muted">(optional)</span><div class="field-help">Other cards in this epic assigned to you. Checked cards run one after another on the VM in one branch and one PR.</div></div>' +
      pick('1. PGS-1401', 'Job list', { on: true }) + pick('2. PGS-1402', 'Create a job', { on: true }) + pick('PGS-1403', 'Edit a job', {}) + pick('PGS-1404', 'Job detail', { off: 'Queued for VM' }) +
      '<div style="display:flex;justify-content:flex-end"><button class="btn btn-sm">' + ic('list-plus') + 'Queue 2 cards together</button></div></div>';
    function qrow(key, badge, extra, actions) {
      return '<div class="card" style="width:640px;padding:10px 12px;display:flex;gap:10px;align-items:center"><span class="mono">' + key + '</span>' + badge +
        '<div style="flex:1;min-width:0"><div class="truncate">' + extra + '</div></div>' + actions + '</div>';
    }
    var moves = '<button class="btn btn-ghost btn-xs btn-icon" aria-label="Move up">' + ic('arrow-up') + '</button><button class="btn btn-ghost btn-xs btn-icon" aria-label="Move down">' + ic('arrow-down') + '</button><button class="btn btn-outline btn-xs">Start now</button>';
    var groupBadge = '<span class="badge badge-outline">' + ic('layers') + 'Group of 3</span>';
    var queue = '<div style="display:grid;gap:6px">' +
      qrow('PGS-1401', groupBadge + '<span class="badge">' + ic('list-checks') + 'Planned</span>', 'Job list <span class="muted">· then PGS-1402, PGS-1403</span>', moves + '<button class="btn btn-outline btn-xs">' + ic('laptop') + 'Take over to laptop</button>') +
      qrow('PGS-1402', '<span class="badge badge-outline">' + ic('layers') + 'Group of 2</span><span class="badge badge-warning">Continues</span>', 'Create a job <span class="muted">· then PGS-1403 · paused at the end of the window</span>', moves + '<button class="btn btn-ghost btn-xs btn-icon" aria-label="Remove">' + ic('x') + '</button>') + '</div>';
    function rrow(key, state, tone, note, actions) {
      return '<div style="display:flex;gap:10px;align-items:center;padding:6px 0;border-bottom:1px solid var(--border)"><span class="mono" style="width:76px">' + key + '</span><span class="badge"' + (tone ? ' data-tone="' + tone + '"' : '') + '>' + state + '</span><span class="muted" style="flex:1;min-width:0;font-size:12px">' + note + '</span>' + (actions || '') + '</div>';
    }
    var report = '<div class="card" style="width:640px;padding:12px"><div style="display:flex;gap:8px;align-items:center;margin-bottom:6px"><b>Group PGS-1401</b><span class="muted">· 2 of 3 committed · draft PR #41</span></div>' +
      rrow('PGS-1401', 'Committed', 'ok', 'AC 5 pass') +
      rrow('PGS-1402', 'Committed', 'warning', 'Changed in Lark after it was committed', '<input class="input" style="width:160px;height:26px" placeholder="Reason to keep"><button class="btn btn-outline btn-xs">Keep</button><button class="btn btn-ghost btn-xs">' + ic('undo-2') + 'Revert</button>') +
      rrow('PGS-1403', 'Not started', null, 'Waits for the decision on PGS-1402') + '</div>';
    var stuck = '<div class="card" style="width:640px;padding:12px">' + rrow('PGS-1402', 'Stuck', 'warning', 'The turn ended without code changes: "Which endpoint lists archived jobs?"', '<button class="btn btn-outline btn-xs">' + ic('laptop') + 'Take over to laptop</button>') + '</div>';
    var onePlan = '<div class="card" style="width:520px;padding:12px;display:grid;gap:6px">' +
      '<div class="field-help">Planning: <b>Plan</b> · Start together: PGS-1401, PGS-1402, PGS-1403</div>' +
      '<label style="display:flex;gap:8px;align-items:flex-start"><input type="checkbox" class="checkbox" checked><span><b>One plan for all cards</b><span class="muted" style="display:block;font-size:12px">The agent writes one plan covering the AC of every card and you approve it once. Cards are still built and committed one by one. If the AC of a card not built yet changes, the plan needs approval again.</span></span></label></div>';
    var planBar = '<div class="card" style="width:640px;padding:10px 12px;display:flex;gap:8px;align-items:center"><span class="muted" style="flex:1">Group plan ready for all 3 cards. Read it in the Plan tab, reply in chat to revise it, or approve to build the cards one by one.</span><button class="btn btn-outline btn-sm">' + ic('moon-star') + 'Approve &amp; queue for VM</button><button class="btn btn-sm">' + ic('list-checks') + 'Approve plan &amp; implement</button></div>';
    var outdated = '<div class="card" style="width:640px;padding:12px">' + rrow('PGS-1401', 'Committed', 'ok', 'Group plan outdated: the AC of PGS-1403 changed after the group plan was approved.', '<button class="btn btn-outline btn-xs">' + ic('laptop') + 'Take over to laptop</button>') + '</div>';
    return sec('groupvm', 'Grouped cards on the VM', 'Queue cards of one epic together; the VM builds them one after another in one branch and one PR, pauses between cards at the end of the window, and reports each card in the morning.',
      spec('QueueSection / Queue together', together, 'block') +
      spec('VM queue rows / group, Planned and continuing', queue, 'block') +
      spec('Morning report / group per card, changed in Lark', report, 'block') +
      spec('Morning report / group card stuck', stuck, 'block') +
      spec('StartTaskDialog / One plan for all cards', onePlan, 'block') +
      spec('Workspace action bar / group plan ready', planBar, 'block') +
      spec('Morning report / group plan outdated', outdated, 'block'));
  }

  var BUILDERS = { groupvm: groupvm, planvm: planvm, plangaps: plangaps, r2board: r2board, logs: logs, acchips: acchips, acrow: acrow, acsummary: acsummary, gatebtn: gatebtn, runapp: runapp, readiness: readiness, sources: sources, theme: theme, type: type, button: button, badge: badge, form: form, tabs: tabs, alert: alert, card: card, overlay: overlay, menu: menu, feedback: feedback,
    runstatus: runstatus, flags: flags, taskcard: taskcard, agentrow: agentrow, chat: chat, tools: tools, diff: diff, composer: composer, steps: steps, comment: comment, empty: empty, reserved: reserved, stepper: stepper, notif: notif, tray: tray };

  function sheet(p) {
    var html = '';
    SECTIONS.forEach(function (g) { g[1].forEach(function (s) { html += BUILDERS[s[0]](); }); });
    return html.replace(/\{\{p\}\}/g, p);
  }

  /* ---------- nav + panes ---------- */
  $('#sheet-nav').innerHTML = SECTIONS.map(function (g) {
    return '<div class="grp caps muted">' + g[0] + '</div>' + g[1].map(function (s) { return '<a class="nav-item" href="#' + s[0] + '" data-go="' + s[0] + '">' + s[1] + '</a>'; }).join('');
  }).join('');
  $('#sheet-nav').addEventListener('click', function (e) {
    var a = e.target.closest('[data-go]'); if (!a) return; e.preventDefault();
    var el = $('.pane [data-sec="' + a.dataset.go + '"]'); if (el) window.scrollTo({ top: el.getBoundingClientRect().top + window.scrollY - 56, behavior: 'smooth' });
  });

  function render(mode) {
    localStorage.setItem('mandor:sheet', mode);
    document.documentElement.setAttribute('data-theme', mode === 'light' ? 'light' : 'dark');
    $$('[data-mode]').forEach(function (b) { if (b.tagName === 'BUTTON') b.setAttribute('aria-pressed', String(b.dataset.mode === mode)); });
    var themes = mode === 'both' ? ['dark', 'light'] : [mode];
    var panes = $('#panes'); panes.dataset.mode = mode;
    panes.innerHTML = themes.map(function (th) { return '<div class="pane" data-theme="' + th + '">' + (mode === 'both' ? '<div class="pane-label caps">' + ic(th === 'dark' ? 'moon' : 'sun', 'icon-sm') + th + '</div>' : '') + sheet(th) + '</div>'; }).join('');
    $$('.pane').forEach(function (pane) {
      var cs = getComputedStyle(pane);
      $$('[data-var]', pane).forEach(function (td) { td.textContent = cs.getPropertyValue(td.dataset.var).trim(); });
    });
    $$('.sheet-tabs').forEach(function (list) {
      list.addEventListener('click', function (e) { var b = e.target.closest('[role="tab"]'); if (!b) return; $$('[role="tab"]', list).forEach(function (x) { x.setAttribute('aria-selected', String(x === b)); }); });
    });
    U.icons();
  }
  $$('button[data-mode]').forEach(function (b) { b.onclick = function () { render(b.dataset.mode); }; });
  render(localStorage.getItem('mandor:sheet') || 'dark');
})();
