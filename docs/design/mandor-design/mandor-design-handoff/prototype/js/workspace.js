/* Task workspace: header, action bar, callouts, tabs (Chat, Changes,
   Task, Review comments), details panel, Composer, Start fix.
   Run state changes go through MANDOR.update so the sidebar and board
   reflect them. */
(function () {
  var M = window.MANDOR, U = window.UI, esc = U.esc, ic = U.ic, $ = U.$, $$ = U.$$;
  var key = U.params.get('task') || 'PGS-1391';
  var t = M.getTask(key);
  U.initShell(key);
  $('#send-kbd').textContent = navigator.platform.indexOf('Mac') > -1 ? '⌘ ↵' : 'Ctrl ↵';

  if (!t || !t.run) {
    $('#main').innerHTML = '<div class="empty">' + '<span class="ico">' + ic('bot-off') + '</span><h3>' + (t ? 'No run for ' + esc(key) : 'Task ' + esc(key) + ' not found') + '</h3><p>' +
      (t ? 'Start the task from the board to open a workspace.' : 'It may have been deleted in Lark or filtered out of your sprint.') + '</p><a class="btn btn-outline" href="board.html">' + ic('arrow-left') + 'Back to board</a></div>';
    U.icons(); return;
  }

  var chat = WS.chatFor(t), diff = WS.diffFor(t), comments = JSON.parse(JSON.stringify(M.comments[key] || []));
  var queue = [], timers = [], held = false, selFile = 0, panelOpen = innerWidth > 1100;
  var DKEY = 'mandor:draft:' + key;

  function refreshTask() { t = M.getTask(key); }
  function setRun(patch, extra) { M.update(key, Object.assign({ run: patch }, extra || {})); refreshTask(); renderChrome(); }
  function later(fn, ms) { var id = setTimeout(fn, ms); timers.push(id); return id; }
  function clearTimers() { timers.forEach(clearTimeout); timers = []; }
  function push(msg) { chat.push(msg); renderChat(); }
  function fmt(sec) { return Math.floor(sec / 60) + ':' + String(sec % 60).padStart(2, '0'); }
  var WORKING = ['running', 'preparing', 'wf_running', 'brainstorming', 'planning'];
  var PLANNING = ['brainstorming', 'bs_question', 'planning', 'plan_ready'];
  /* Release 2: tasks with an AC ledger get the AC review tab and the gate */
  var AC = null, RA = null, PL = null;
  if (t.ac && window.ACTAB) $('#tab-ac').hidden = false;
  if (t.run && t.run.planned && window.PLAN) $('#tab-plan').hidden = false;

  /* ---------------- header ---------------- */
  function statusIcon(s) { return { 'To Do': 'circle', 'In Progress': 'circle-dot', 'Waiting Fix': 'circle-alert', 'Waiting Review': 'eye', 'In Review': 'eye' }[s] || 'circle-help'; }
  function renderHeader() {
    var r = t.run;
    $('#ws-header').innerHTML =
      '<a class="btn btn-ghost btn-sm btn-icon" href="board.html" aria-label="Back to board" title="Back to board">' + ic('arrow-left') + '</a>' +
      '<div class="ws-title"><span class="key">' + t.key + '</span><h1 title="' + esc(t.title) + '">' + esc(t.title) + '</h1></div>' +
      '<div class="meta">' +
        '<span class="badge" title="Task Status in Lark" data-ui="Badge / variant=outline">' + ic(statusIcon(t.status)) + 'Lark: ' + esc(t.status) + '</span>' +
        U.runBadge(r.state) +
        '<span class="branch-chip" data-ui="BranchChip">' + ic('git-branch', 'icon-sm') + '<span>' + esc(r.branch) + '</span><button class="btn btn-ghost btn-xs btn-icon" id="copy-branch" aria-label="Copy branch name">' + ic('copy', 'icon-sm') + '</button></span>' +
        (t.repos ? '<span class="ws-chip" title="Workspace: ' + esc(t.repos.join(' + ')) + '" data-ui="Badge / custom=WorkspaceChip">' + ic('folders', 'icon-sm') + esc(t.repo) + ' · ' + t.repos.length + ' repos</span>' : '') +
        (r.prs ? '<span class="pr-links">' + r.prs.map(function (p) { return '<a class="link mono" href="#" onclick="return false" title="Open PR on GitHub">' + ic('git-pull-request', 'icon-sm') + esc(p[0]) + '#' + p[1] + '</a>'; }).join('') + '</span>'
          : r.pr ? '<a class="link mono" href="#" onclick="return false" title="Open PR on GitHub">' + ic('git-pull-request', 'icon-sm') + '#' + r.pr + '</a>' : '') +
        '<button class="btn btn-ghost btn-sm btn-icon" id="toggle-panel" aria-pressed="' + panelOpen + '" aria-label="Task details" title="Task details">' + ic('panel-right') + '</button>' +
      '</div>';
    $('#copy-branch').onclick = function () { navigator.clipboard && navigator.clipboard.writeText(r.branch); U.toast('Copied branch name', r.branch, 'copy'); };
    $('#toggle-panel').onclick = function () { panelOpen = !panelOpen; renderPanel(); renderHeader(); U.icons(); };
  }

  /* ---------------- action bar ---------------- */
  var quietTimer = null;
  function renderActions() {
    var s = t.run.state, left = '', right = '';
    var sensitiveFile = diff.find(function (f) { return f.sensitive; });
    if (WORKING.indexOf(s) > -1) left = '<span class="muted">' + (s === 'wf_running' ? 'Applying review feedback. Mandor pushes and moves Lark back to Waiting Review when done.' : 'Agent is working. Messages you send now are queued.') + '</span>';
    if (s === 'awaiting_dev') {
      left = '<span class="muted">Turn finished. Reply, or push when the changes look right.</span>';
      right = (diff.length ? '' : '<span class="muted" style="font-size:12px">No changes to push yet</span>') +
        (AC ? AC.gateButton({ disabled: !diff.length }) : '<button class="btn btn-default btn-sm" id="act-push" data-ui="Button / variant=default / size=sm"' + (diff.length ? '' : ' disabled') + '>' + ic('git-pull-request-arrow') + 'Push &amp; create PR</button>');
      if (AC) { var g = AC.gate(); left = '<span class="muted">' + { review_needed: 'Turn finished. Push starts an AC review first; push again when it finishes.', blocked: 'Reviewer found failing AC. Send them to the agent, or push anyway with a reason.', needs_confirmation: "Some AC can't be verified by the reviewer. Confirm them before pushing.", warn: 'Warn mode: failing AC do not block the push.', ready: 'Every AC passed or was confirmed. Ready to push.', no_ac: 'Turn finished. Reply, or push when the changes look right.' }[g.state] + '</span>'; }
    }
    if (s === 'push_held') left = '<span class="muted">Automatic fix done. The push waits for your check.</span>';
    if (s === 'brainstorming') left = '<span class="muted">Brainstorming with ' + esc(t.run.skill || 'Mandor built-in') + '. The agent may ask you questions in Chat.</span>';
    if (s === 'bs_question') {
      left = '<span class="muted">The agent asked a question in Chat. Answer it, or continue without answering.</span>';
      right = '<button class="btn btn-default btn-sm" id="act-continue">' + ic('arrow-right') + 'Continue to plan</button>';
    }
    if (s === 'planning') left = '<span class="muted">Writing the plan and comparing each AC with ' + esc((AC && AC.ledger().designSource) ? AC.ledger().designSource.name : 'the design') + '.</span>';
    if (s === 'plan_ready') {
      var ng = PL ? PL.gaps().length : 0;
      left = '<span class="muted">Plan ready' + (ng ? ' with ' + ng + ' design gap' + (ng > 1 ? 's' : '') : '') + '. Review it in the Plan tab. Push &amp; create PR is available after you approve.</span>';
      right = '<button class="btn btn-ghost btn-sm" id="act-plan">' + ic('notebook-pen') + 'Open plan</button><button class="btn btn-default btn-sm" id="act-approve">' + ic('play') + 'Approve plan &amp; implement</button>';
    }
    if (s === 'reviewing' && AC) {
      left = '<span class="run" data-tone="working">' + ic('scan-search') + '<span class="lbl">Reviewer is checking each AC. Results fill in on the AC review tab.</span></span>';
      right = AC.gateButton();
    }
    if (s === 'publishing') left = '<span class="run" data-tone="working">' + ic('loader-circle', 'spin') + '<span class="lbl">Committing, pushing, and opening the PR…</span></span>';
    if (s === 'in_review_loop') left = '<span class="muted">PR #' + t.run.pr + ' is open. Nothing to do until a reviewer moves the task in Lark.</span>';
    if (s === 'wf_awaiting_comment') {
      left = '<span class="muted">Task is in Waiting Fix. No trusted reviewer comment yet.</span>';
      right = '<button class="btn btn-default btn-sm" id="act-fix">' + ic('wrench') + 'Start fix</button>';
    }
    if (s === 'wf_quiet') {
      left = '<span class="run" data-tone="waiting">' + ic('timer') + '<span class="lbl">Quiet period: forwarding in <span class="mono" id="quiet-left">' + fmt(t.run.quietLeft) + '</span></span></span>';
      right = '<button class="btn btn-secondary btn-sm" id="act-fix">' + ic('wrench') + 'Start fix now</button>';
    }
    if (s === 'failed') right = '<button class="btn btn-default btn-sm" id="act-resume">' + ic('rotate-ccw') + 'Resume</button>';
    if (s === 'orphaned' || s === 'stopped') right = '<button class="btn btn-outline btn-sm" id="act-close">Close run</button>';
    if (WORKING.indexOf(s) > -1) right = '<button class="btn btn-outline btn-sm" id="act-stop">' + ic('square') + 'Stop</button>';
    if (RA && s !== 'publishing' && PLANNING.indexOf(s) === -1) right = RA.html() + right;
    right += '<button class="btn btn-ghost btn-sm btn-icon" id="act-more" aria-label="More run actions" aria-haspopup="menu">' + ic('ellipsis') + '</button>';
    $('#ws-actions').innerHTML = left + '<span class="right">' + right + '</span>';
    bind('#act-push', function (e) { if (sensitiveFile && !t.run.sensitiveConfirmed) { held = true; renderCallout(); } else if (AC) AC.onPush(e); else publish(); });
    if (RA) RA.bind();
    bind('#act-continue', continueToPlan);
    bind('#act-plan', function () { selectTab('plan'); });
    bind('#act-approve', openApprove);
    bind('#act-fix', openFix);
    bind('#act-resume', function () { push({ type: 'system', icon: 'rotate-ccw', text: 'Session resumed by developer' }); agentTurn(); });
    bind('#act-close', confirmCloseRun);
    bind('#act-stop', stopTurn);
    bind('#act-more', openMore);
  }
  function bind(sel, fn) { var el = $(sel); if (el) el.onclick = fn; }

  function openMore(e) {
    var menu = $('#more-menu'), btn = e.currentTarget;
    menu.innerHTML = '<button class="menu-item" data-m="lark">' + ic('external-link') + 'Open in Lark</button>' +
      '<button class="menu-item" data-m="editor">' + ic('code') + 'Open worktree in editor</button>' +
      '<button class="menu-item" data-m="branch">' + ic('copy') + 'Copy branch name</button>' +
      '<div class="menu-sep"></div><button class="menu-item danger" data-m="close">' + ic('circle-x') + 'Close run</button>';
    var r = btn.getBoundingClientRect();
    menu.style.top = (r.bottom + 4) + 'px'; menu.style.left = (r.right - 220) + 'px'; menu.hidden = false; U.icons();
    menu.onclick = function (ev) {
      var it = ev.target.closest('[data-m]'); if (!it) return; menu.hidden = true;
      if (it.dataset.m === 'close') confirmCloseRun();
      if (it.dataset.m === 'branch') U.toast('Copied branch name', t.run.branch, 'copy');
      if (it.dataset.m === 'lark') U.toast('Opening ' + key + ' in Lark', null, 'external-link');
      if (it.dataset.m === 'editor') U.toast('Opened in VS Code', '~/.local/share/mandor/workspaces/' + key.toLowerCase(), 'code');
    };
  }
  document.addEventListener('click', function (e) { if (!e.target.closest('#more-menu') && !e.target.closest('#act-more')) $('#more-menu').hidden = true; });

  /* ---------------- callouts ---------------- */
  function alertHtml(kind, icon, title, body, actions) {
    return '<div class="alert alert-' + kind + '" role="' + (kind === 'failed' ? 'alert' : 'status') + '" data-ui="Alert / variant=' + kind + '">' + ic(icon) +
      '<div><div class="alert-title">' + title + '</div>' + (body ? '<div class="muted">' + body + '</div>' : '') + '</div>' +
      (actions ? '<div class="alert-actions">' + actions + '</div>' : '') + '</div>';
  }
  function renderCallout() {
    var s = t.run.state, html = '';
    if (s === 'failed') html = alertHtml('failed', 'circle-x', 'Run failed', esc(t.run.error));
    if (s === 'orphaned') html = alertHtml('warning', 'square-parking', 'Parked: claim lost', esc(t.run.error));
    if (t.flag) { var f = M.flags[t.flag.type]; html = alertHtml(f.tone === 'failed' ? 'failed' : 'warning', f.icon, f.label, esc(t.flag.reason), t.flag.type === 'sync_failed' ? '<button class="btn btn-outline btn-sm" id="co-retry">' + ic('refresh-cw') + 'Retry</button>' : ''); }
    if (s === 'wf_awaiting_comment' && t.run.untrustedNotes) html = alertHtml('info', 'shield-question', 'Fix Notes are from an untrusted author; confirm via Start fix', 'Rizky Pratama is not an assignee, reporter, or trusted reviewer for pegasus-web, so the comment is not forwarded automatically.');
    if (held) {
      var f2 = diff.filter(function (x) { return x.sensitive; }).map(function (x) { return '<span class="mono">' + esc(x.path) + '</span>'; }).join(', ');
      html = alertHtml('warning', 'shield-alert', 'Changes touch CI, agent config, or git hooks; push needs developer confirmation', 'Held file: ' + f2 + '. Review it in Changes before pushing.',
        '<button class="btn btn-ghost btn-sm" id="co-discard">Discard</button><button class="btn btn-outline btn-sm" id="co-review">Review</button><button class="btn btn-secondary btn-sm" id="co-confirm">Confirm push</button>');
    }
    if (t.run.note) html = alertHtml('info', 'info', esc(t.run.note), '');
    if (s === 'push_held' && t.acs) html = alertHtml('warning', 'shield-alert', 'Automatic fix finished; push held', esc(t.acs.held) + ' Check it yourself in the app, then push the fix to ' + (t.run.prs ? t.run.prs.map(function (p) { return p[0] + '#' + p[1]; }).join(', ') : 'the PR') + '.',
      '<button class="btn btn-secondary btn-sm" id="co-held">' + ic('user-round-check') + 'I checked it, push fix</button>');
    $('#ws-callout').innerHTML = html;
    bind('#co-review', function () { selectTab('changes'); selFile = diff.findIndex(function (x) { return x.sensitive; }); renderChanges(); });
    bind('#co-confirm', confirmSensitivePush);
    bind('#co-held', function () { push({ type: 'system', icon: 'upload', text: 'AC-2 checked manually by Beni Handoko · fix pushed to kelola-web#41 · Lark moved to Waiting Review' }); setRun({ state: 'in_review_loop' }, { status: 'Waiting Review' }); U.toast('Fix pushed to kelola-web#41', key + ' is back in Waiting Review', 'git-pull-request'); });
    bind('#co-discard', discardSensitive);
    bind('#co-retry', function () { M.update(key, { flag: null, status: 'Waiting Review' }); refreshTask(); renderChrome(); U.toast('Synced ' + key, 'Lark moved to Waiting Review'); });
    U.icons();
  }

  /* ---------------- chat ---------------- */
  /* ChatMessage / ToolCallGroup renderers live in js/components.js */
  var msgHtml = UI.chatMessage;
  function renderChat() {
    $('#chat-inner').innerHTML = chat.map(msgHtml).join('');
    U.icons();
    var c = $('#chat'); c.scrollTop = c.scrollHeight;
  }

  /* agent turn simulation */
  function agentTurn(reply) {
    setRun({ state: 'running' });
    var group = { type: 'tools', live: true, calls: [{ tool: 'Read', prev: diff[0] ? diff[0].path : 'src/', s: 'ok', out: '' }] };
    later(function () { push(group); }, 500);
    later(function () { group.calls[0].out = 'read'; group.calls.push({ tool: 'Edit', prev: diff[0] ? diff[0].path : 'src/lib', s: 'ok', out: '+4 −1' }); renderChat(); }, 1600);
    later(function () { group.calls.push({ tool: 'Bash', prev: t.repo === 'pegasus-api' ? 'cargo test' : 'pnpm test', s: 'ok', out: 'all tests passed' }); renderChat(); }, 2600);
    later(function () {
      group.live = false;
      if (!diff.length && key === 'PGS-1402') {
        diff.push({ s: 'M', path: 'src/routes/invoices/+page.svelte', add: 22, del: 4, rows: [
          ['h', '', '', '@@ -12,8 +12,26 @@ <script lang="ts">'],
          ['d', 14, '', '  const invoices = await listInvoices();'],
          ['a', '', 14, '  const status = $page.url.searchParams.get("status") ?? "all";'],
          ['a', '', 15, '  const range = parseRange($page.url.searchParams);'],
          ['a', '', 16, '  const invoices = await listInvoices({ status, from: range.from, to: range.to });'],
          ['c', 15, 17, '</script>']
        ] });
        diff.push({ s: 'M', path: 'src/lib/api/invoice.ts', add: 9, del: 2, rows: [
          ['h', '', '', '@@ -20,4 +20,11 @@ export type InvoiceFilter'],
          ['d', 21, '', 'export async function listInvoices() {'],
          ['a', '', 21, 'export async function listInvoices(filter: InvoiceFilter = {}) {'],
          ['a', '', 22, '  const q = new URLSearchParams(clean(filter));'],
          ['c', 22, 23, '  const res = await api(`/invoices?${q}`);']
        ] });
        renderChanges();
      }
      push({ type: 'agent', html: reply || '<p>Done. I applied your note, kept the rest of the change as it was, and re-ran the tests: all passing.</p>' });
      push({ type: 'turn', text: 'Turn finished', dur: '0m 38s', tok: '12.6k in · 1.4k out' });
      setRun({ state: 'awaiting_dev', fresh: false });
      if (AC) AC.codeChanged('2 files in kelola-web and kelola-api');
      if (RA) RA.codeChanged();
      if (queue.length) { var next = queue.shift(); renderQueue(); later(function () { sendNow(next, true); }, 400); }
    }, 3800);
  }
  function stopTurn() {
    clearTimers();
    chat.forEach(function (m) { if (m.live) m.live = false; });
    push({ type: 'system', icon: 'square', text: 'Turn stopped by developer' });
    setRun({ state: 'awaiting_dev' });
  }

  /* ---------------- composer ---------------- */
  var input = $('#composer-input');
  input.value = localStorage.getItem(DKEY) || '';
  input.addEventListener('input', function () { localStorage.setItem(DKEY, input.value); renderComposer(); autosize(); });
  input.addEventListener('keydown', function (e) { if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') { e.preventDefault(); primarySend(); } });
  function autosize() { input.style.height = 'auto'; input.style.height = Math.min(200, input.scrollHeight) + 'px'; }
  function canChat() { return WORKING.indexOf(t.run.state) > -1 || ['awaiting_dev', 'bs_question', 'plan_ready'].indexOf(t.run.state) > -1; }
  function renderComposer() {
    var s = t.run.state, working = WORKING.indexOf(s) > -1, has = input.value.trim().length > 0;
    var reasons = { in_review_loop: 'The agent is idle while PR #' + t.run.pr + ' waits for review. Feedback goes through Start fix.', wf_awaiting_comment: 'Use Start fix to send your instruction with the reviewer comments.', wf_quiet: 'Trusted reviewer comments are forwarded when the quiet period ends.', publishing: 'Creating the PR. You can reply after it is opened.', failed: 'Resume the run to continue the session.', orphaned: 'This run is parked. Close it, or ask the new owner to hand it back.', stopped: 'The agent is stopped. Close the run to clean up the worktree.', reviewing: 'The reviewer is checking each AC. You can reply when it finishes.', push_held: 'The automatic fix is done. Confirm the held AC above to push it.' };
    input.disabled = !canChat();
    input.placeholder = canChat() ? (working ? 'Queue a message for when this turn ends' : 'Reply to the agent. Drafts are kept per task.') : (reasons[s] || '');
    $('#composer-actions').innerHTML = working
      ? '<button class="btn btn-ghost btn-sm" id="c-stop">' + ic('square') + 'Stop</button><button class="btn btn-outline btn-sm" id="c-now"' + (has ? '' : ' disabled') + ' title="Stop the current turn, then send">' + ic('zap') + 'Send now</button><button class="btn btn-secondary btn-sm" id="c-send"' + (has ? '' : ' disabled') + '>' + ic('list-plus') + 'Queue</button>'
      : '<button class="btn btn-secondary btn-sm" id="c-send"' + (has && canChat() ? '' : ' disabled') + '>' + ic('send-horizontal') + 'Send</button>';
    bind('#c-stop', stopTurn);
    bind('#c-now', function () { var v = input.value.trim(); if (!v) return; clearInput(); stopTurn(); sendNow(v); });
    bind('#c-send', primarySend);
    U.icons();
  }
  function clearInput() { input.value = ''; localStorage.removeItem(DKEY); autosize(); }
  function primarySend() {
    var v = input.value.trim(); if (!v || !canChat()) return;
    clearInput();
    if (WORKING.indexOf(t.run.state) > -1) { queue.push(v); renderQueue(); renderComposer(); return; }
    sendNow(v);
  }
  function sendNow(v) {
    push({ type: 'dev', text: v });
    if (t.run.state === 'bs_question') return continueToPlan(true);
    agentTurn();
  }
  function renderQueue() {
    $('#queued').innerHTML = queue.map(function (q, i) { return '<span class="q-chip" data-ui="Badge / custom=QueuedMessage">' + ic('clock', 'icon-sm') + '<span title="' + esc(q) + '">' + esc(q) + '</span><button class="btn btn-ghost btn-xs btn-icon" data-rm="' + i + '" aria-label="Remove queued message">' + ic('x', 'icon-sm') + '</button></span>'; }).join('');
    $$('[data-rm]').forEach(function (b) { b.onclick = function () { queue.splice(+b.dataset.rm, 1); renderQueue(); }; });
    U.icons();
  }

  /* ---------------- publish (Push & create PR) ---------------- */
  function confirmSensitivePush() {
    $('#cf-title').textContent = 'Push changes to CI configuration?';
    $('#cf-desc').textContent = 'These files change how CI, the agent, or git hooks run for everyone on the repo. Mandor pushes them only with your confirmation.';
    $('#cf-body').innerHTML = diff.filter(function (x) { return x.sensitive; }).map(function (x) { return '<div class="list-editor"><div class="item">' + ic('shield-alert', 'icon-sm') + '<span>' + esc(x.path) + '</span><span class="add-c">+' + x.add + '</span><span class="del-c">−' + x.del + '</span></div></div>'; }).join('');
    $('#cf-go').className = 'btn btn-default'; $('#cf-go').textContent = 'Confirm push';
    $('#cf-go').onclick = function () { U.closeDialog('dlg-confirm'); held = false; setRun({ sensitiveConfirmed: true }); publish(); };
    U.openDialog('dlg-confirm');
  }
  function discardSensitive() {
    var f = diff.find(function (x) { return x.sensitive; });
    $('#cf-title').textContent = 'Discard changes to ' + f.path + '?';
    $('#cf-desc').textContent = 'Mandor restores the file from origin/main in the worktree. Other changes stay.';
    $('#cf-body').innerHTML = '';
    $('#cf-go').className = 'btn btn-destructive'; $('#cf-go').textContent = 'Discard changes';
    $('#cf-go').onclick = function () {
      U.closeDialog('dlg-confirm'); diff = diff.filter(function (x) { return !x.sensitive; }); held = false; selFile = 0;
      push({ type: 'system', icon: 'undo-2', text: 'Discarded changes to ' + f.path });
      renderChanges(); renderChrome(); U.toast('Changes discarded', f.path, 'undo-2');
    };
    U.openDialog('dlg-confirm');
  }
  function publish() {
    if (t.repos) return publishMulti();
    var pr = 418;
    held = false;
    setRun({ state: 'publishing' });
    var n = diff.length;
    later(function () { push({ type: 'system', icon: 'git-commit-horizontal', text: 'Commit 3f9c2ab: feat: ' + t.title.split(': ').pop().toLowerCase() + ' (' + key + '), ' + n + ' files' }); }, 700);
    later(function () { push({ type: 'system', icon: 'upload', text: 'Pushed ' + t.run.branch + ' to origin' }); }, 1500);
    later(function () {
      push({ type: 'system', icon: 'git-pull-request', text: 'PR #' + pr + ' opened · Lark moved to Waiting Review' });
      setRun({ state: 'in_review_loop', pr: pr }, { status: 'Waiting Review' });
      U.toast('PR #' + pr + ' created', key + ' moved to Waiting Review in Lark', 'git-pull-request');
    }, 2400);
  }

  /* ---------------- Before coding: brainstorm, plan, approve ---------------- */
  function brainstormTurn() {
    later(function () { push({ type: 'tools', calls: [
      { tool: 'Read', prev: 'kelola-api/src/po/mod.rs', s: 'ok', out: '186 lines' },
      { tool: 'Read', prev: 'kelola-design/src/routes/po/[id]/+page.svelte', s: 'ok', out: '204 lines' },
      { tool: 'Grep', prev: '"approve" in kelola-design/src', s: 'ok', out: 'src/routes/approvals/+page.svelte:14\nsrc/routes/po/[id]/+page.svelte:88' }
    ] }); }, 900);
    later(function () {
      push({ type: 'agent', html: '<p>One question before I plan: the design puts bulk approve on a separate <code>Approvals</code> page, but AC-5 says the PO list page. Which one should I build?</p>' });
      setRun({ state: 'bs_question' });
      U.toast(key + ' has a question for you', 'Brainstorming: AC-5 and the design disagree', 'message-circle-question');
    }, 2600);
  }
  function continueToPlan(answered) {
    if (answered !== true) push({ type: 'system', icon: 'arrow-right', text: 'Continued to planning without an answer' });
    setRun({ state: 'planning' });
    if (PL) PL.render();
    later(function () { push({ type: 'tools', calls: [
      { tool: 'Read', prev: 'kelola-design/src/routes/po/+page.svelte', s: 'ok', out: '161 lines' },
      { tool: 'Write', prev: 'docs/plans/pgs-1426-po-approval.md', s: 'ok', out: '38 lines' }
    ] }); }, 900);
    later(planReady, 2800);
  }
  function planReady() {
    if (AC) AC.load('plan');
    push({ type: 'agent', html: '<p>The plan is ready. Compared with <code>kelola-design</code>: AC-1 and AC-6 are in the design, AC-2 partly, AC-3, AC-4 and AC-7 are missing (I will build them from the AC), and AC-5 conflicts with the design. Details are in the Plan tab.</p>' });
    push({ type: 'turn', text: 'Plan ready', dur: '3m 05s', tok: '41.2k in · 4.4k out' });
    setRun({ state: 'plan_ready' });
    if (PL) PL.render();
    U.toast(key + ': plan ready', 'Review the design gaps, then approve', 'file-check');
  }
  function openApprove() {
    var g = PL ? PL.gaps() : [], follow = g.filter(function (r) { return r.design.v === 'follow'; }), build = g.filter(function (r) { return r.design.v !== 'follow'; });
    $('#cf-title').textContent = 'Approve the plan and start implementing?';
    $('#cf-desc').textContent = 'The agent implements the plan. After approval the design gaps are written to the task in Lark, so the designer can update the design.';
    $('#cf-body').innerHTML = (g.length ? '<div class="gate-list">' +
      build.map(function (r) { return '<div class="gate-row">' + ic('hammer', 'gate-q') + '<span><span class="mono">' + esc(r.id) + '</span> ' + U.ac.DESIGN[r.design.v].label + '<span class="muted gate-why">Built from the AC</span></span></div>'; }).join('') +
      follow.map(function (r) { return '<div class="gate-row">' + ic('signpost-big', 'gate-q') + '<span><span class="mono">' + esc(r.id) + '</span> Follow design<span class="muted gate-why">Left out of the gate; the PM is asked to update the AC</span></span></div>'; }).join('') +
      '</div>' : '<p class="muted">No design gaps.</p>') + '<p class="field-help" style="margin-top:10px">Follow design can no longer be undone after this.</p>';
    $('#cf-go').className = 'btn btn-default'; $('#cf-go').innerHTML = ic('play') + 'Approve &amp; implement';
    $('#cf-go').onclick = function () { U.closeDialog('dlg-confirm'); approvePlan(follow); };
    U.openDialog('dlg-confirm');
  }
  function approvePlan(follow) {
    var at = new Date().toTimeString().slice(0, 5);
    setRun({ approved: true, approvedAt: 'at ' + at });
    push({ type: 'system', icon: 'circle-check', text: 'Plan approved' + (follow.length ? ' · ' + follow.map(function (r) { return r.id; }).join(', ') + ' marked Follow design, PM asked to update the AC' : '') });
    push({ type: 'system', icon: 'frame', text: 'Design gaps written to the task in Lark' });
    U.toast('Design gaps written to Lark', key + ' · field "Design gaps"', 'frame');
    renderTask(); if (PL) PL.render();
    implementTurn();
  }
  function implementTurn() {
    var reply = WS.chatFor(t).filter(function (m) { return m.type === 'agent'; }).pop();
    agentTurn(reply ? reply.html : null);
  }

  /* One branch, one PR per changed repo; each PR gets the AC summary */
  function publishMulti() {
    var prs = [['kelola-api', 12], ['kelola-web', 34]];
    held = false;
    setRun({ state: 'publishing' });
    later(function () { push({ type: 'system', icon: 'git-commit-horizontal', text: 'Committed in kelola-api (3 files) and kelola-web (3 files)' }); }, 700);
    later(function () { push({ type: 'system', icon: 'upload', text: 'Pushed ' + t.run.branch + ' to origin in both repos' }); }, 1500);
    later(function () {
      push({ type: 'system', icon: 'git-pull-request', text: 'PR kelola-api#12 and kelola-web#34 opened with the AC summary · Lark moved to Waiting Review' });
      setRun({ state: 'in_review_loop', pr: 12, prs: prs }, { status: 'Waiting Review' });
      U.toast('2 PRs created', 'kelola-api#12, kelola-web#34 · ' + key + ' moved to Waiting Review', 'git-pull-request');
    }, 2400);
  }

  /* ---------------- Start fix ---------------- */
  function openFix() {
    $('#fx-key').textContent = key;
    $('#fx-instr').value = ''; $('#fx-instr').removeAttribute('aria-invalid'); $('#fx-err').hidden = true;
    $('#fx-comments').innerHTML = comments.length ? comments.map(function (c, i) {
      var p = M.people[c.author];
      var locked = c.trusted && !c.sent;
      return '<label class="comment" style="display:flex;gap:10px;align-items:flex-start;cursor:' + (locked || c.sent ? 'default' : 'pointer') + '">' +
        '<input type="checkbox" class="checkbox" data-c="' + i + '" style="margin-top:2px"' + (locked ? ' checked disabled' : '') + (c.sent ? ' disabled' : '') + '>' +
        '<div style="flex:1"><div class="head">' + U.avatar(c.author) + '<b>' + esc(p.name) + '</b><span class="time">' + esc(c.time) + '</span><span class="right">' +
          '<span class="badge trust" data-trusted="' + c.trusted + '">' + ic(c.trusted ? 'shield-check' : 'shield-question') + (c.trusted ? 'Trusted' : 'Not trusted') + '</span>' +
          (locked ? '<span class="badge badge-secondary">' + ic('lock') + 'Will be sent</span>' : c.sent ? '<span class="badge badge-secondary">' + ic('check') + 'Already sent</span>' : '<span class="badge">Opt-in</span>') +
        '</span></div><div>' + esc(c.text) + '</div></div></label>';
    }).join('') : '<p class="muted">No comments on this record yet. The agent gets your instruction only.</p>';
    $('#fx-go').onclick = function () {
      var v = $('#fx-instr').value.trim();
      if (!v) { $('#fx-instr').setAttribute('aria-invalid', 'true'); $('#fx-err').hidden = false; $('#fx-instr').focus(); return; }
      var picked = comments.filter(function (c, i) { var cb = $('[data-c="' + i + '"]'); return !c.sent && (c.trusted || (cb && cb.checked)); });
      U.closeDialog('dlg-fix');
      startFixRun(v, picked);
    };
    U.openDialog('dlg-fix');
  }
  function startFixRun(instr, picked) {
    clearInterval(quietTimer);
    if (instr) push({ type: 'dev', text: instr });
    picked.forEach(function (c) { c.sent = true; push({ type: 'review', author: c.author, time: c.time, trusted: c.trusted, text: c.text }); });
    push({ type: 'system', icon: 'wrench', text: 'Fix started · Lark moved from Waiting Fix to In Progress' });
    setRun({ state: 'wf_running', untrustedNotes: false }, { status: 'In Progress' });
    renderComments();
    fixTurn();
  }
  function fixTurn() {
    var g = { type: 'tools', live: true, calls: [{ tool: 'Read', prev: diff[0] ? diff[0].path : 'src/', s: 'ok', out: '' }] };
    later(function () { push(g); }, 400);
    later(function () { g.calls.push({ tool: 'Edit', prev: diff[0] ? diff[0].path : 'src/', s: 'ok', out: '+9 −3' }); renderChat(); }, 1800);
    later(function () { g.calls.push({ tool: 'Bash', prev: t.repo === 'pegasus-api' ? 'cargo test' : 'pnpm test', s: 'ok', out: 'all tests passed' }); renderChat(); }, 3200);
    later(function () {
      g.live = false;
      push({ type: 'agent', html: '<p>Addressed the review feedback and re-ran the tests. Pushing the fix to the same PR.</p>' });
      push({ type: 'system', icon: 'upload', text: 'Pushed fix to PR #' + t.run.pr + ' · Lark moved to Waiting Review' });
      setRun({ state: 'in_review_loop' }, { status: 'Waiting Review' });
      U.toast('Fix pushed to PR #' + t.run.pr, key + ' is back in Waiting Review', 'git-pull-request');
    }, 4800);
  }
  function startQuiet() {
    if (t.run.state !== 'wf_quiet') return;
    var left = t.run.quietLeft;
    quietTimer = setInterval(function () {
      left--; t.run.quietLeft = left;
      var el = $('#quiet-left'); if (el) el.textContent = fmt(left);
      var q = $('#quiet-time'); if (q) q.textContent = fmt(left);
      var bar = $('#quiet-bar'); if (bar) bar.style.width = (100 - left / 120 * 100) + '%';
      if (left <= 0) {
        clearInterval(quietTimer);
        var picked = comments.filter(function (c) { return c.trusted && !c.sent; });
        picked.forEach(function (c) { c.sent = true; push({ type: 'review', author: c.author, time: c.time, trusted: true, text: c.text }); });
        push({ type: 'system', icon: 'wrench', text: 'Quiet period ended. Forwarded ' + picked.length + ' trusted comments · Lark moved to In Progress' });
        setRun({ state: 'wf_running', quietLeft: 0 }, { status: 'In Progress' });
        renderComments(); fixTurn();
      }
    }, 1000);
  }

  function confirmCloseRun() {
    $('#cf-title').textContent = 'Close the run for ' + key + '?';
    $('#cf-desc').textContent = 'Mandor stops the agent and removes the worktree. The branch ' + t.run.branch + ' stays on origin, and Lark is not changed.';
    $('#cf-body').innerHTML = '';
    $('#cf-go').className = 'btn btn-destructive'; $('#cf-go').textContent = 'Close run';
    $('#cf-go').onclick = function () { M.update(key, { closed: true }); location.href = 'board.html'; };
    U.openDialog('dlg-confirm');
  }

  /* ---------------- tabs ---------------- */
  function selectTab(id) {
    $$('[role="tab"]').forEach(function (b) { b.setAttribute('aria-selected', String(b.dataset.tab === id)); });
    $$('[role="tabpanel"]').forEach(function (p) { p.hidden = p.id !== 'p-' + id; });
  }
  $$('[role="tab"]:not([hidden])').forEach(function (b, i, all) {
    b.onclick = function () { selectTab(b.dataset.tab); };
    b.onkeydown = function (e) { if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') { var n = all[(i + (e.key === 'ArrowRight' ? 1 : all.length - 1)) % all.length]; n.focus(); n.click(); } };
  });

  /* ---------------- changes ---------------- */
  function renderChanges() {
    $('#cnt-changes').textContent = diff.length || '';
    var el = $('#changes');
    if (!diff.length) { el.style.display = 'flex'; el.innerHTML = '<div class="empty">' + '<span class="ico">' + ic('git-compare') + '</span><h3>No changes against the base branch</h3><p>The agent has not changed any file on ' + esc(t.run.branch) + ' yet.</p></div>'; U.icons(); return; }
    el.style.display = '';
    var f = diff[selFile] || diff[0];
    el.innerHTML = '<div class="filelist scroll-area" role="listbox" aria-label="Changed files">' +
      '<div class="caps muted" style="padding:6px 8px 8px">' + diff.length + ' files · <span class="add-c">+' + diff.reduce(function (a, x) { return a + x.add; }, 0) + '</span> <span class="del-c">−' + diff.reduce(function (a, x) { return a + x.del; }, 0) + '</span></div>' +
      diff.map(function (x, i) { return (x.repo && (i === 0 || diff[i - 1].repo !== x.repo) ? '<div class="file-group" role="presentation">' + ic('folder-git-2', 'icon-sm') + esc(x.repo) + '</div>' : '') + '<button class="file-row" role="option" data-f="' + i + '" aria-selected="' + (x === f) + '" title="' + esc(x.path) + '"><span class="fs" data-s="' + x.s + '" aria-label="' + { A: 'added', M: 'modified', D: 'deleted', R: 'renamed', U: 'untracked' }[x.s] + '">' + x.s + '</span><span class="fp">' + esc(x.path) + '</span><span class="fc">' + (x.sensitive ? ic('shield-alert', 'icon-sm') + ' ' : '') + '<span class="add-c">+' + x.add + '</span> <span class="del-c">−' + x.del + '</span></span></button>'; }).join('') +
      '</div><div class="diff scroll-area"><div class="diff-file-head">' + ic('file-code', 'icon-sm') + '<span>' + (f.repo ? '<span class="muted">' + esc(f.repo) + ' / </span>' : '') + esc(f.path) + '</span>' + (f.sensitive ? '<span class="badge" style="color:var(--status-warning)">' + ic('shield-alert') + 'Sensitive: needs confirmation to push</span>' : '') + '</div><table>' +
      f.rows.map(function (r) { var cls = { h: 'hunk', c: 'ctx', a: 'add', d: 'del' }[r[0]]; var sign = { a: '+', d: '−', c: ' ', h: '' }[r[0]];
        return r[0] === 'h' ? '<tr class="hunk"><td class="ln"></td><td class="ln"></td><td class="code">' + esc(r[3]) + '</td></tr>' : '<tr class="' + cls + '"><td class="ln">' + r[1] + '</td><td class="ln">' + r[2] + '</td><td class="code"><span class="sign">' + sign + '</span>' + esc(r[3]) + '</td></tr>'; }).join('') +
      '</table></div>';
    $$('[data-f]').forEach(function (b) { b.onclick = function () { selFile = +b.dataset.f; renderChanges(); }; });
    U.icons();
  }

  /* ---------------- task tab ---------------- */
  function renderTask() {
    var d = WS.detailFor(t);
    $('#p-task').innerHTML = '<div class="task-doc" data-ui="Workspace / TaskTab">' +
      '<div class="alert alert-info">' + ic('lock') + '<div class="muted">Read-only. Lark owns the title, description, acceptance criteria, and assignees. Edit them in Lark.</div><div class="alert-actions"><button class="btn btn-outline btn-sm" onclick="UI.toast(\'Opening ' + key + ' in Lark\', null, \'external-link\')">' + ic('external-link') + 'Open in Lark</button></div></div>' +
      '<section><h2>Description</h2><div class="md">' + (d ? d.description.split('\n\n').map(function (p) { return '<p>' + esc(p) + '</p>'; }).join('') : '<p class="muted">No description in Lark.</p>') + '</div></section>' +
      '<section><h2>Acceptance criteria</h2>' + (d ? '<div class="ac-list">' + d.ac.map(function (a) { return '<div class="ac" data-dropped="' + !!a.dropped + '"><span class="id">' + a.id + '</span><span class="txt">' + esc(a.text) + (a.dropped ? ' <span class="badge" style="margin-left:6px;text-decoration:none">Dropped</span>' : '') + '</span></div>'; }).join('') + '</div>' : '<p class="muted">No acceptance criteria linked in the AC table.</p>') + '</section>' +
      designGapsField() +
      '<section><h2>Fields</h2><dl class="kv">' + kvRows() + '</dl></section></div>';
    U.icons();
  }
  /* Release 2: the gaps written to Lark after plan approval */
  function designGapsField() {
    if (!AC || !t.run || !t.run.approved) return '';
    var g = PLAN.gaps(AC.ledger());
    if (!g.length) return '';
    var lines = g.map(function (r) { var v = r.design.v === 'follow' ? 'Follow design (AC outdated, please update)' : U.ac.DESIGN[r.design.v].label; return r.id + ': ' + v + (r.design.note ? '. ' + r.design.note : ''); });
    return '<section data-ui="TaskTab / DesignGapsField"><h2>Design gaps <span class="muted" style="font-weight:400">written to Lark ' + esc(t.run.approvedAt || '') + '</span></h2><pre class="ev-pre" style="font-family:var(--font-sans);font-size:var(--text-sm)">' + esc(lines.join('\n')) + '</pre></section>';
  }
  function kvRows() {
    var d = WS.detailFor(t) || {};
    var sp = M.sprints.find(function (s) { return s.id === t.sprint; });
    return '<dt>Lark status</dt><dd>' + esc(t.status) + '</dd>' +
      '<dt>Readiness</dt><dd>' + esc(t.readiness) + '</dd>' +
      '<dt>Sprint</dt><dd>' + (sp ? esc(sp.name + ' · ' + sp.range) : '—') + '</dd>' +
      '<dt>Assignees</dt><dd>' + (d.assignees || ['me']).map(function (a) { return U.avatar(a) + ' ' + esc(M.people[a].name); }).join(', ') + '</dd>' +
      '<dt>Reporter</dt><dd>' + (d.reporter ? U.avatar(d.reporter) + ' ' + esc(M.people[d.reporter].name) : '—') + '</dd>' +
      '<dt>Repo</dt><dd class="mono">' + esc(t.repo) + '</dd>';
  }

  /* ---------------- review comments ---------------- */
  function renderComments() {
    $('#cnt-comments').textContent = comments.length || '';
    var s = t.run.state, inFix = t.status === 'Waiting Fix' || ['wf_awaiting_comment', 'wf_quiet', 'wf_running'].indexOf(s) > -1;
    var head = '';
    if (s === 'wf_quiet') head = '<div class="quiet" data-ui="QuietPeriod"><span>' + ic('timer') + '</span><div style="flex:1"><b>Quiet period</b><div class="muted" style="font-size:12px">Waiting for more comments before forwarding trusted ones to the agent.</div><div class="bar"><i id="quiet-bar" style="width:' + (100 - t.run.quietLeft / 120 * 100) + '%"></i></div></div><span class="time" id="quiet-time">' + fmt(t.run.quietLeft) + '</span></div>';
    $('#p-comments').innerHTML = !comments.length
      ? '<div class="empty"><span class="ico">' + ic('message-square-dashed') + '</span><h3>' + (inFix ? 'No reviewer comments yet' : 'No review comments') + '</h3><p>' + (inFix ? 'Mandor checks the Lark record for new comments on every sync.' : 'Comments from the Lark record show up here when a reviewer moves the task to Waiting Fix.') + '</p></div>'
      : '<div class="comments" data-ui="Workspace / ReviewComments">' + head + comments.slice().reverse().map(function (c) {
          var p = M.people[c.author];
          return '<article class="comment" data-ui="CommentItem"><div class="head">' + U.avatar(c.author) + '<b>' + esc(p.name) + '</b><span class="time">' + esc(c.time) + '</span><span class="right">' +
            '<span class="badge trust" data-trusted="' + c.trusted + '">' + ic(c.trusted ? 'shield-check' : 'shield-question') + (c.trusted ? 'Trusted' : 'Not trusted') + '</span>' +
            '<span class="badge badge-secondary">' + (c.sent ? ic('send-horizontal') + 'Sent to agent' : ic('clock') + (c.trusted ? 'Not sent yet' : 'Only shown to you')) + '</span></span></div><div>' + esc(c.text) + '</div></article>';
        }).join('') + '</div>';
    U.icons();
  }

  /* ---------------- details panel ---------------- */
  function renderPanel() {
    var p = $('#ws-panel'); p.hidden = !panelOpen;
    p.innerHTML = '<div style="display:flex;align-items:center"><h2 style="font-size:13px;font-weight:600">Details</h2><button class="btn btn-ghost btn-xs btn-icon" style="margin-left:auto" id="panel-close" aria-label="Close details">' + ic('x', 'icon-sm') + '</button></div>' +
      '<dl class="kv">' + kvRows() + '</dl><hr class="separator">' +
      '<dl class="kv"><dt>Run</dt><dd>' + U.runBadge(t.run.state) + '</dd><dt>Started</dt><dd>' + esc(t.run.since === 'now' ? 'just now' : t.run.since + ' ago') + '</dd>' +
      '<dt>Worktree</dt><dd class="mono" style="font-size:11px;word-break:break-all">~/.local/share/mandor/workspaces/' + key.toLowerCase() + '</dd>' +
      '<dt>Model</dt><dd>Claude Code · subscription</dd></dl>';
    bind('#panel-close', function () { panelOpen = false; renderPanel(); renderHeader(); U.icons(); });
    U.icons();
  }

  function renderChrome() { renderHeader(); renderActions(); renderCallout(); renderComposer(); renderPanel(); U.icons(); }

  /* ---------------- boot ----------------
     Loading state first (Skeleton), then the run snapshot from the daemon. */
  function renderLoading() {
    var bar = function (w, h) { return '<span class="skeleton" style="display:block;height:' + (h || 12) + 'px;width:' + w + '"></span>'; };
    $('#main').setAttribute('aria-busy', 'true');
    $('#ws-header').innerHTML = '<a class="btn btn-ghost btn-sm btn-icon" href="board.html" aria-label="Back to board">' + ic('arrow-left') + '</a><span class="mono muted">' + esc(key) + '</span>' + bar('320px', 14) + '<span style="margin-left:auto;display:flex;gap:8px">' + bar('110px', 20) + bar('90px', 20) + '</span>';
    $('#ws-actions').innerHTML = bar('280px') + '<span class="right">' + bar('120px', 28) + '</span>';
    $('#chat-inner').innerHTML = '<span class="sr-only">Loading the run from the daemon…</span>' +
      '<div style="display:grid;gap:8px">' + bar('46%', 10) + '</div>' +
      '<div style="justify-self:end;width:52%;display:grid;gap:6px">' + bar('100%') + bar('70%') + '</div>' +
      '<div class="toolgroup" style="height:32px;display:flex;align-items:center;padding:0 10px">' + bar('38%', 10) + '</div>' +
      '<div style="display:grid;gap:6px;width:72%">' + bar('100%') + bar('94%') + bar('60%') + '</div>';
    $('#composer-input').disabled = true;
    U.icons();
  }
  function boot() {
    $('#main').removeAttribute('aria-busy');
    if (t.repos && window.RUNAPP) RA = RUNAPP.mount({
      key: key, repos: t.repos, mountIn: $('.ws-main'),
      diff: function () { return diff; },
      openFile: function (repo, path) { var i = diff.findIndex(function (x) { return x.path === path && (!x.repo || x.repo === repo); }); if (i > -1) selFile = i; selectTab('changes'); renderChanges(); },
      onChange: function () { if ($('#ws-actions').innerHTML) renderActions(); U.icons(); }
    });
    var PREV = { brainstorming: 'notchecked', question: 'notchecked', planning: 'notchecked', plan: 'plan', implementing: 'design' };
    if (t.ac && window.ACTAB) AC = ACTAB.mount({
      previewScenario: t.preview && !U.params.get('ac') ? (PREV[U.params.get('phase')] || 'design') : null,
      defaultScenario: ['brainstorming', 'bs_question', 'planning'].indexOf(t.run.state) > -1 ? 'notchecked' : t.run.state === 'plan_ready' ? 'plan' : 'design',
      appState: RA ? RA.state : null,
      appStart: RA ? RA.requestStart : null,
      key: key, panel: $('#p-ac'), countEl: $('#cnt-ac'),
      isBusy: function () { return WORKING.indexOf(t.run.state) > -1 || t.run.state === 'publishing'; },
      setRunState: function (st) { setRun({ state: st }); },
      publish: function () { publish(); },
      selectTab: function (id) { selectTab(id); },
      openFile: function (repo, path) { var i = diff.findIndex(function (x) { return x.path === path && (!x.repo || x.repo === repo); }); if (i > -1) selFile = i; selectTab('changes'); renderChanges(); },
      sendToAgent: function (text, rows) { push({ type: 'dev', text: text }); push({ type: 'system', icon: 'send-horizontal', text: 'Sent ' + rows.map(function (r) { return r.id; }).join(', ') + ' to the agent · marked Fix in progress until the next review' }); selectTab('chat'); agentTurn('<p>Fixed ' + rows.map(function (r) { return r.id; }).join(', ') + '. The reject dialog now requires at least 10 characters, and <code>kelola-api</code> rejects shorter reasons with a 422. Added tests on both sides.</p>'); },
      onChange: function () { if ($('#ws-actions').innerHTML) renderActions(); U.icons(); }
    });
    if (t.run.planned && window.PLAN && AC) {
      PL = PLAN.mount({ key: key, panel: $('#p-plan'), ac: AC, runState: function () { return t.run.state; }, approved: function () { return !!t.run.approved; }, approvedAt: function () { return t.run.approvedAt || 'earlier'; }, onApprove: openApprove });
      PL.render(); renderActions(); renderTask();
    }
    if (t.repos && t.run.fresh && !t.preview) {
      setRun({ fresh: false });
      if (t.run.state === 'brainstorming') brainstormTurn();
      if (t.run.state === 'planning') continueToPlan(true);
      if (t.run.state === 'running') implementTurn();
    }
    if (AC && AC.ledger().reviewing && t.run.state !== 'reviewing') setRun({ state: 'reviewing' });
    if (AC && !AC.ledger().reviewing && t.run.state === 'reviewing') setRun({ state: 'awaiting_dev' });
    renderChrome(); renderChat(); renderChanges(); renderTask(); renderComments(); renderQueue(); autosize();
    startQuiet();
    if (t.run.state === 'running' && !t.repos && (t.run.fresh || key === 'PGS-1388')) {
      later(function () {
        chat.forEach(function (m) { m.live = false; });
        push({ type: 'agent', html: t.run.fresh
          ? '<p>I read the task, the acceptance criteria, and <code>CLAUDE.md</code>. Plan: add the filter state to the URL, reuse <code>DateRangePicker</code>, and extend <code>listInvoices()</code> with <code>status</code> and <code>from/to</code>. Shall I go ahead?</p>'
          : '<p>Fixed. Overtime is now computed from minutes and rounded once at the end. <code>cargo test overtime</code>: 14 passed. Existing payslips are untouched (AC-5480).</p>' });
        push({ type: 'turn', text: 'Turn finished', dur: t.run.fresh ? '0m 52s' : '6m 40s', tok: t.run.fresh ? '21.0k in · 1.9k out' : '44.8k in · 5.2k out' });
        setRun({ state: 'awaiting_dev', fresh: false });
        U.toast(key + ' is waiting for you', 'The agent finished its turn', 'hand');
      }, t.run.fresh ? 5000 : 7000);
    }
    if (t.run.state === 'wf_running' && key === 'PGS-1362') {
      later(function () {
        chat.forEach(function (m) { m.live = false; });
        push({ type: 'agent', html: '<p>Night shifts are now normalised to an end time on the next day before the overlap check. Added a test for 22:00 to 06:00 followed by 05:00 to 13:00.</p>' });
        push({ type: 'system', icon: 'upload', text: 'Pushed fix to PR #399 · Lark moved to Waiting Review' });
        setRun({ state: 'in_review_loop' }, { status: 'Waiting Review' });
        U.toast('Fix pushed to PR #399', key + ' is back in Waiting Review', 'git-pull-request');
      }, 6500);
    }
    if (U.params.get('tab')) selectTab(U.params.get('tab'));
    if (U.params.get('open') === 'fix') openFix();
    if (U.params.get('open') === 'push') { held = true; renderCallout(); }
    if (AC && U.params.get('gate')) AC.openDialog(U.params.get('gate'));
    if (RA && U.params.get('open') === 'runapp') RA.requestStart();
  }
  renderLoading();
  setTimeout(boot, U.params.get('loading') === 'hold' ? 600000 : 450);
})();
