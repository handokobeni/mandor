/* Mandor app shell: TitleBar, Sidebar (Active agents), Toaster (Sonner),
   Command palette, theme, shared render helpers. Vanilla JS so the
   markup maps cleanly onto shadcn-svelte components. */
(function () {
  var M = window.MANDOR;
  var params = new URLSearchParams(location.search);

  /* ---------- theme ---------- */
  var TKEY = 'mandor:theme';
  /* ?frame=1 (frames.html gallery): URL params apply to this view only and are not persisted */
  var FRAME = params.get('frame') === '1';
  if (params.get('theme') && !FRAME) localStorage.setItem(TKEY, params.get('theme'));
  function applyTheme() {
    var pref = (FRAME && params.get('theme')) || localStorage.getItem(TKEY) || 'dark';
    var dark = pref === 'dark' || (pref === 'system' && matchMedia('(prefers-color-scheme: dark)').matches);
    document.documentElement.setAttribute('data-theme', dark ? 'dark' : 'light');
    return pref;
  }
  applyTheme();
  matchMedia('(prefers-color-scheme: dark)').addEventListener('change', applyTheme);

  /* ---------- session flags ---------- */
  if (params.get('lark') && !FRAME) localStorage.setItem('mandor:lark', params.get('lark'));
  function larkState() { return (FRAME && params.get('lark')) || (FRAME ? 'ok' : localStorage.getItem('mandor:lark') || 'ok'); }

  /* ---------- helpers ---------- */
  function esc(s) { return String(s == null ? '' : s).replace(/[&<>"']/g, function (c) { return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]; }); }
  function ic(name, cls) { return '<i data-lucide="' + name + '"' + (cls ? ' class="' + cls + '"' : '') + ' aria-hidden="true"></i>'; }
  function icons() { if (window.lucide) window.lucide.createIcons({ attrs: { 'stroke-width': 1.75 } }); }
  function $(sel, root) { return (root || document).querySelector(sel); }
  function $$(sel, root) { return Array.prototype.slice.call((root || document).querySelectorAll(sel)); }

  /* RunStatusBadge */
  function runBadge(state, opts) {
    var s = M.runStates[state];
    if (!s || state === 'stopped') return '';
    opts = opts || {};
    var dot = s.tone === 'attention' ? '' : '';
    return '<span class="run" data-ui="RunStatusBadge / state=' + state + '" data-tone="' + s.tone + '">' +
      ic(s.icon, s.spin ? 'spin' : '') + dot + '<span class="lbl">' + esc(opts.label || s.label) + '</span></span>';
  }
  function dotFor(state) {
    var s = M.runStates[state] || { tone: 'muted' };
    var tone = { working: 'var(--status-working)', attention: 'var(--status-attention)', waiting: 'var(--status-waiting)', failed: 'var(--status-failed)', warning: 'var(--status-warning)', ok: 'var(--status-ok)' }[s.tone] || 'var(--muted-foreground)';
    return '<span class="dot' + (state === 'awaiting_dev' ? ' pulse' : '') + '" style="--run-c:' + tone + '"></span>';
  }
  /* CardFlagBadge */
  function flagBadge(flag, withAction) {
    var f = M.flags[flag.type];
    return '<div class="flag" data-ui="CardFlagBadge / flag=' + flag.type + '" data-tone="' + f.tone + '">' + ic(f.icon) +
      '<div><b>' + esc(f.label) + '</b>' + (flag.reason ? '<div class="reason">' + esc(flag.reason) + '</div>' : '') + '</div></div>';
  }
  function avatar(pid, md) { var p = M.people[pid]; return p ? '<span class="avatar' + (md ? ' avatar-md' : '') + '" title="' + esc(p.name) + '">' + p.initials + '</span>' : ''; }

  /* ---------- toasts (Sonner) ---------- */
  function toast(title, desc, icon) {
    var host = $('#toaster');
    if (!host) { host = document.createElement('div'); host.id = 'toaster'; host.className = 'toaster'; host.setAttribute('role', 'status'); host.setAttribute('aria-live', 'polite'); document.body.appendChild(host); }
    var el = document.createElement('div');
    el.className = 'toast'; el.setAttribute('data-ui', 'Sonner / toast');
    el.innerHTML = ic(icon || 'circle-check') + '<div><div>' + esc(title) + '</div>' + (desc ? '<div class="t-desc">' + esc(desc) + '</div>' : '') + '</div>';
    host.appendChild(el); icons();
    setTimeout(function () { el.style.transition = 'opacity 200ms'; el.style.opacity = '0'; setTimeout(function () { el.remove(); }, 220); }, 3600);
  }

  /* ---------- dialogs ---------- */
  function openDialog(id) { var d = document.getElementById(id); if (d && !d.open) { d.showModal(); icons(); } return d; }
  function closeDialog(id) { var d = document.getElementById(id); if (d && d.open) d.close(); }
  document.addEventListener('click', function (e) {
    var c = e.target.closest('[data-close]');
    if (c) { var d = c.closest('dialog'); if (d) d.close(); }
    if (e.target.tagName === 'DIALOG' && e.target.open) {
      var r = e.target.getBoundingClientRect();
      if (e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom) { if (!e.target.hasAttribute('data-static')) e.target.close(); }
    }
  });

  /* ---------- title bar ---------- */
  /* Owner indicator for the one local app (Release 2) */
  function appIndicator() {
    var a = M.getApp && M.getApp();
    if (!a || !a.owner || a.state === 'off' || a.state === 'stopped') return '';
    var tone = { running: 'ok', older: 'warning', failed: 'failed' }[a.state] || 'working';
    var lbl = { running: 'running', older: 'older code', failed: 'failed to start' }[a.state] || 'starting';
    return '<a class="app-owner" href="workspace.html?task=' + a.owner + '" data-tone="' + tone + '" data-ui="Badge / custom=AppOwner" title="Local app owned by ' + a.owner + '. Open its workspace.">' + ic('app-window', 'icon-sm') + '<span class="conn-text">App · </span><span class="mono">' + a.owner + '</span><span class="conn-text">' + lbl + '</span></a>';
  }
  window.addEventListener('mandor:app', function () { renderTitlebar(); icons(); });

  function renderTitlebar() {
    var tb = $('#titlebar'); if (!tb) return;
    var lark = larkState();
    var waiting = M.allTasks().filter(function (t) { return t.run && ['awaiting_dev', 'bs_question', 'plan_ready', 'push_held'].indexOf(t.run.state) > -1; });
    var daemon = params.get('daemon') === 'reconnecting' ? 'reconnecting' : 'connected';
    tb.setAttribute('data-ui', 'TitleBar');
    tb.innerHTML =
      '<a class="brand" href="board.html" aria-label="Mandor board"><span class="brand-mark" aria-hidden="true"><i></i><i></i><i></i></span>Mandor</a>' +
      '<span class="separator-v" style="height:16px;align-self:center"></span>' +
      '<span class="conn" data-state="' + daemon + '" title="' + (daemon === 'connected' ? 'mandord 0.4.2 on WSL Ubuntu-24.04' : 'Lost the daemon, reconnecting') + '"><span class="dot"></span><span class="conn-text">' + (daemon === 'connected' ? 'Daemon connected' : 'Reconnecting…') + '</span></span>' +
      (lark === 'expired'
        ? '<button class="conn" data-state="expired" id="tb-lark">' + ic('triangle-alert', 'icon-sm') + '<span class="conn-text">Lark session expired</span></button>'
        : lark === 'loggedout'
        ? '<a class="conn" data-state="expired" href="settings.html#lark">' + ic('log-in', 'icon-sm') + '<span class="conn-text">Not logged in to Lark</span></a>'
        : '<span class="conn" title="Lark session valid">' + ic('user-round-check', 'icon-sm') + '<span class="conn-text">Lark · ' + M.people.me.name + '</span></span>') +
      '<span class="spacer"></span>' +
      appIndicator() +
      '<button class="tally" id="tb-tally" data-ui="Button / custom=AttentionTally"' + (waiting.length ? '' : ' hidden') + ' title="Jump to the agent waiting for you">' + ic('hand', 'icon-sm') + waiting.length + ' waiting for you</button>' +
      '<button class="cmd-trigger" id="tb-cmd" data-ui="Command / trigger">' + ic('search', 'icon-sm') + '<span class="cmd-text">Open task, start, settings…</span><span class="kbd">' + (navigator.platform.indexOf('Mac') > -1 ? '⌘K' : 'Ctrl K') + '</span></button>' +
      '<button class="btn btn-ghost btn-sm btn-icon" id="tb-refresh" data-ui="Button / variant=ghost / size=sm" aria-label="Refresh board" title="Refresh board">' + ic('refresh-cw') + '</button>' +
      (function () { var u = M.getNotifications().filter(function (n) { return !n.read; }).length; return '<button class="btn btn-ghost btn-sm btn-icon bell" id="tb-bell" data-ui="Popover / NotificationCenter trigger" aria-haspopup="dialog" aria-expanded="false" aria-label="Notifications' + (u ? ', ' + u + ' unread' : '') + '" title="Notifications">' + ic('bell') + (u ? '<span class="bell-count mono" aria-hidden="true">' + (u > 99 ? '99+' : u) + '</span>' : '') + '</button>'; })() +
      '<a class="btn btn-ghost btn-sm btn-icon" href="settings.html" data-ui="Button / variant=ghost / size=sm" aria-label="Settings" title="Settings">' + ic('settings') + '</a>' +
      '<div class="win-ctl" aria-hidden="true"><button tabindex="-1">' + ic('minus', 'icon-sm') + '</button><button tabindex="-1">' + ic('square', 'icon-sm') + '</button><button tabindex="-1">' + ic('x', 'icon-sm') + '</button></div>';
    var tally = $('#tb-tally');
    if (tally) tally.onclick = function () { if (waiting[0]) location.href = 'workspace.html?task=' + waiting[0].key; };
    $('#tb-cmd').onclick = openCommand;
    $('#tb-bell').onclick = function (e) { e.stopPropagation(); toggleNotif(); };
    $('#tb-refresh').onclick = function () { window.dispatchEvent(new Event('mandor:refresh')); if (!document.body.dataset.page || document.body.dataset.page !== 'board') location.href = 'board.html'; };
    var lb = $('#tb-lark'); if (lb) lb.onclick = function () { location.href = 'settings.html#lark'; };
  }

  /* ---------- notification center (Popover + list) ---------- */
  function notifCenterHtml(list) {
    var unread = list.filter(function (n) { return !n.read; }).length;
    return '<div class="notif-head"><b>Notifications</b><span class="muted">' + (unread ? unread + ' unread' : 'All caught up') + '</span>' +
      (unread ? '<button class="btn btn-ghost btn-xs" id="notif-all">Mark all as read</button>' : '') + '</div>' +
      '<div class="notif-list scroll-area">' + (list.length ? list.map(function (n) {
        return '<a class="notif-item" href="' + (n.href || 'workspace.html?task=' + n.task) + '" data-n="' + n.id + '"' + (n.read ? '' : ' data-unread="true"') + ' data-ui="NotificationItem / kind=' + n.kind + (n.read ? '' : ' / unread') + '">' +
          '<span class="ni-ico" data-kind="' + n.kind + '">' + ic(n.icon, 'icon-sm') + '</span>' +
          '<span class="ni-txt"><span class="ni-title">' + esc(n.title) + '</span><span class="ni-body">' + esc(n.body) + '</span></span>' +
          '<span class="ni-meta"><span class="mono">' + n.ago + '</span>' + (n.read ? '' : '<span class="ni-dot" aria-label="Unread"></span>') + '</span></a>';
      }).join('') : '<div class="empty" style="padding:24px"><span class="ico">' + ic('bell-off') + '</span><p>No notifications yet.</p></div>') + '</div>' +
      '<div class="notif-foot"><a class="btn btn-link btn-sm" href="settings.html#general">Notification settings</a></div>';
  }
  function toggleNotif(force) {
    var pop = $('#notif-pop');
    if (!pop) { pop = document.createElement('div'); pop.id = 'notif-pop'; pop.className = 'popover notif'; pop.setAttribute('role', 'dialog'); pop.setAttribute('aria-label', 'Notifications'); pop.setAttribute('data-ui', 'Popover / NotificationCenter'); pop.hidden = true; document.body.appendChild(pop); }
    var open = force != null ? force : pop.hidden;
    var bell = $('#tb-bell');
    if (!open) { pop.hidden = true; if (bell) bell.setAttribute('aria-expanded', 'false'); return; }
    var list = M.getNotifications();
    pop.innerHTML = notifCenterHtml(list);
    var r = bell.getBoundingClientRect();
    pop.style.top = (r.bottom + 6) + 'px'; pop.style.left = Math.max(8, r.right - 380) + 'px';
    pop.hidden = false; bell.setAttribute('aria-expanded', 'true'); icons();
    var all = $('#notif-all'); if (all) all.onclick = function () { M.markRead(list.map(function (n) { return n.id; })); renderTitlebar(); icons(); toggleNotif(true); };
    $$('.notif-item', pop).forEach(function (a) { a.onclick = function () { M.markRead([a.dataset.n]); }; });
    var first = pop.querySelector('.notif-item, button'); if (first) first.focus();
  }
  document.addEventListener('click', function (e) { var p = $('#notif-pop'); if (p && !p.hidden && !e.target.closest('#notif-pop') && !e.target.closest('#tb-bell')) toggleNotif(false); });
  document.addEventListener('keydown', function (e) { var p = $('#notif-pop'); if (e.key === 'Escape' && p && !p.hidden) { toggleNotif(false); var b = $('#tb-bell'); if (b) b.focus(); } });

  /* ---------- sidebar: Active agents ---------- */
  var GROUPS = [
    { id: 'you', label: 'Waiting for you', match: function (t) { return ['awaiting_dev', 'bs_question', 'plan_ready', 'push_held'].indexOf(t.run.state) > -1; } },
    { id: 'attn', label: 'Needs attention', match: function (t) { return t.flag || ['failed', 'orphaned'].indexOf(t.run.state) > -1; } },
    { id: 'work', label: 'Working', match: function (t) { return ['running', 'preparing', 'publishing', 'wf_running', 'brainstorming', 'planning', 'reviewing'].indexOf(t.run.state) > -1; } },
    { id: 'wait', label: 'Waiting', match: function () { return true; } }
  ];
  function agentRow(t, current) {
    var s = M.runStates[t.run.state];
    var a = M.acSummary ? M.acSummary(t) : null;
    var acLabel = !t.flag && a && t.run.state === 'awaiting_dev' ? (a.fail ? 'Review: ' + a.fail + ' failed' : a.cant ? 'Needs your check' : null) : null;
    var label = t.flag ? M.flags[t.flag.type].label : acLabel || s.label;
    var state = t.flag ? null : t.run.state;
    var att = ['awaiting_dev', 'bs_question', 'plan_ready', 'push_held'].indexOf(t.run.state) > -1;
    return '<a class="agent-row" data-ui="AgentRow' + (att ? ' / variant=attention' : '') + '" href="workspace.html?task=' + t.key + '"' +
      (att ? ' data-attention="true"' : '') + (current ? ' aria-current="page"' : '') + ' title="' + esc(t.key + ' · ' + t.title + ' · ' + label) + '">' +
      (t.flag ? '<span class="dot" style="--run-c:var(--status-' + M.flags[t.flag.type].tone + ')"></span>' : dotFor(t.run.state)) +
      '<span class="ar-short">' + t.key.replace('PGS-', '') + '</span>' +
      '<span class="ar-line"><span class="ar-key">' + t.key + '</span><span class="ar-title">' + esc(t.title) + '</span></span>' +
      (state ? runBadge(state, acLabel ? { label: acLabel } : null) : '<span class="run" data-tone="' + M.flags[t.flag.type].tone + '">' + ic(M.flags[t.flag.type].icon) + '<span class="lbl">' + label + '</span></span>') +
      '</a>';
  }
  function renderSidebar(currentKey) {
    var sb = $('#sidebar'); if (!sb) return;
    var page = document.body.dataset.page;
    var runs = M.allTasks().filter(function (t) { return t.run && t.owner === 'me' || (t.run && t.run.state === 'orphaned'); });
    var used = {};
    var groupsHtml = GROUPS.map(function (g) {
      var rows = runs.filter(function (t) { if (used[t.key]) return false; if (g.match(t)) { used[t.key] = 1; return true; } return false; });
      if (!rows.length) return '';
      return '<div class="sidebar-group-label caps"><span class="label-text">' + g.label + '</span><span class="count mono">' + rows.length + '</span></div>' +
        '<div style="display:grid;gap:1px;padding:0 8px">' + rows.map(function (t) { return agentRow(t, t.key === currentKey); }).join('') + '</div>';
    }).join('');
    sb.setAttribute('data-ui', 'Sidebar / collapsible=icon');
    sb.innerHTML =
      '<nav class="sidebar-nav" aria-label="Main">' +
        '<a class="nav-item" href="board.html"' + (page === 'board' ? ' aria-current="page"' : '') + ' title="Board">' + ic('square-kanban') + '<span class="label-text">Board</span></a>' +
        '<a class="nav-item" href="settings.html"' + (page === 'settings' ? ' aria-current="page"' : '') + ' title="Settings">' + ic('settings-2') + '<span class="label-text">Settings</span></a>' +
      '</nav><hr class="separator">' +
      '<div class="sidebar-group-label caps" style="padding-top:12px"><span class="label-text">Active agents</span><span class="count mono">' + runs.length + '</span></div>' +
      '<div class="sidebar-list scroll-area" data-ui="ScrollArea">' + (runs.length ? groupsHtml : '<p class="muted label-text" style="padding:8px 16px;font-size:12px">No agents running. Start a task from the board.</p>') + '</div>' +
      '<div class="sidebar-footer">' + avatar('me', true) + '<div class="who">' + M.people.me.name + '<span>Pegasus · Sprint Tasks</span></div></div>';
  }

  /* ---------- command palette ---------- */
  var cmdItems = [], cmdSel = 0;
  function buildCommandDialog() {
    if ($('#cmdk')) return;
    var d = document.createElement('dialog');
    d.className = 'dialog command'; d.id = 'cmdk'; d.setAttribute('data-ui', 'Command / dialog'); d.setAttribute('aria-label', 'Command palette');
    d.innerHTML = '<div class="command-input">' + ic('search') + '<input id="cmdk-input" placeholder="Type a task key, a title, or a command" aria-label="Search commands" autocomplete="off"><span class="kbd">Esc</span></div>' +
      '<div class="command-list scroll-area" id="cmdk-list" role="listbox"></div>';
    document.body.appendChild(d);
    $('#cmdk-input').addEventListener('input', renderCommand);
    $('#cmdk-input').addEventListener('keydown', function (e) {
      if (e.key === 'ArrowDown') { e.preventDefault(); cmdSel = Math.min(cmdItems.length - 1, cmdSel + 1); paintSel(); }
      else if (e.key === 'ArrowUp') { e.preventDefault(); cmdSel = Math.max(0, cmdSel - 1); paintSel(); }
      else if (e.key === 'Enter') { e.preventDefault(); if (cmdItems[cmdSel]) cmdItems[cmdSel].go(); }
    });
  }
  function commandSource() {
    var list = [];
    M.allTasks().forEach(function (t) {
      list.push({ group: 'Tasks', icon: t.run ? 'bot' : 'square-dashed', text: t.key + '  ' + t.title, hint: t.run ? M.runStates[t.run.state].label : t.status,
        go: function () { location.href = t.run ? 'workspace.html?task=' + t.key : 'board.html?focus=' + t.key; } });
      if (!M.startGate(t)) list.push({ group: 'Actions', icon: 'play', text: 'Start ' + t.key, hint: t.title, go: function () { location.href = 'board.html?start=' + t.key; } });
    });
    list.push({ group: 'Actions', icon: 'square-kanban', text: 'Go to board', hint: '', go: function () { location.href = 'board.html'; } });
    list.push({ group: 'Actions', icon: 'refresh-cw', text: 'Refresh board', hint: '', go: function () { location.href = 'board.html?refresh=1'; } });
    list.push({ group: 'Actions', icon: 'settings', text: 'Go to settings', hint: '', go: function () { location.href = 'settings.html'; } });
    list.push({ group: 'Actions', icon: 'folder-plus', text: 'Add a repo', hint: 'Settings › Repos', go: function () { location.href = 'settings.html#repos'; } });
    return list;
  }
  function renderCommand() {
    var q = $('#cmdk-input').value.trim().toLowerCase();
    cmdItems = commandSource().filter(function (i) { return !q || (i.text + ' ' + i.hint).toLowerCase().indexOf(q) > -1; }).slice(0, 40);
    cmdSel = 0;
    var html = '', last = '';
    cmdItems.forEach(function (i, n) {
      if (i.group !== last) { html += '<div class="command-group caps muted">' + i.group + '</div>'; last = i.group; }
      html += '<div class="command-item" role="option" data-i="' + n + '">' + ic(i.icon) + '<span>' + esc(i.text.slice(0, 64)) + '</span><span class="hint">' + esc(i.hint) + '</span></div>';
    });
    $('#cmdk-list').innerHTML = html || '<div class="command-empty">No task or command matches "' + esc(q) + '".</div>';
    $$('.command-item').forEach(function (el) {
      el.onclick = function () { cmdItems[+el.dataset.i].go(); };
      el.onmousemove = function () { cmdSel = +el.dataset.i; paintSel(); };
    });
    paintSel(); icons();
  }
  function paintSel() { $$('.command-item').forEach(function (el) { el.setAttribute('aria-selected', String(+el.dataset.i === cmdSel)); }); }
  function openCommand() { buildCommandDialog(); $('#cmdk-input').value = ''; renderCommand(); openDialog('cmdk'); $('#cmdk-input').focus(); }
  document.addEventListener('keydown', function (e) {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') { e.preventDefault(); openCommand(); }
  });

  function relTime(sec) { if (sec < 60) return sec + 's ago'; return Math.floor(sec / 60) + 'm ago'; }

  function initShell(currentKey) {
    renderTitlebar(); renderSidebar(currentKey); icons();
    if (params.get('notif') === 'open') setTimeout(function () { toggleNotif(true); }, 0);
    window.addEventListener('mandor:change', function () { renderTitlebar(); renderSidebar(currentKey); icons(); });
  }

  window.UI = { esc: esc, ic: ic, icons: icons, $: $, $$: $$, runBadge: runBadge, dotFor: dotFor, flagBadge: flagBadge, avatar: avatar,
    toast: toast, agentRow: agentRow, notifCenterHtml: notifCenterHtml, openDialog: openDialog, closeDialog: closeDialog, initShell: initShell, applyTheme: applyTheme, larkState: larkState,
    params: params, relTime: relTime, openCommand: openCommand };
})();
