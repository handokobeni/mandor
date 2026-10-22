/* Board (kanban). Cards are derived from Lark Task Status; there is no
   drag and drop. Columns move only through actions or Lark changes. */
(function () {
  var M = window.MANDOR, U = window.UI, esc = U.esc, ic = U.ic, $ = U.$, $$ = U.$$;
  var state = { sprint: 's42', q: '', scope: 'all', doneCollapsed: true, loading: true, syncedAt: Date.now() - 12000,
    syncError: U.params.get('sync') === 'error' };

  U.initShell(null);

  /* ---------------- banners ---------------- */
  function renderBanners() {
    var html = '';
    if (U.larkState() === 'expired') {
      html += '<div class="alert alert-warning" role="alert" data-ui="Alert / variant=warning">' + ic('triangle-alert') +
        '<div><div class="alert-title">Your Lark session expired</div><div class="muted">The board is not syncing. Agents keep running, but status changes wait until you log in.</div></div>' +
        '<div class="alert-actions"><button class="btn btn-default btn-sm" id="bn-login">' + ic('log-in') + 'Log in again</button></div></div>';
    }
    if (!localStorage.getItem('mandor:dismiss:restart')) {
      html += '<div class="alert alert-info" data-ui="Alert / variant=default">' + ic('rotate-ccw') +
        '<div><div class="alert-title">The daemon restarted at 09:58</div><div class="muted">1 run was interrupted. Resume it from its workspace: <a class="link mono" href="workspace.html?task=PGS-1379">PGS-1379</a></div></div>' +
        '<div class="alert-actions"><button class="btn btn-ghost btn-sm btn-icon" id="bn-dismiss" aria-label="Dismiss">' + ic('x') + '</button></div></div>';
    }
    $('#banners').innerHTML = html;
    var l = $('#bn-login'); if (l) l.onclick = function () { fakeLogin(); };
    var d = $('#bn-dismiss'); if (d) d.onclick = function () { localStorage.setItem('mandor:dismiss:restart', '1'); renderBanners(); };
    U.icons();
  }
  function fakeLogin() {
    U.toast('Opened Lark in your browser', 'Waiting for the login to finish…', 'external-link');
    setTimeout(function () { localStorage.setItem('mandor:lark', 'ok'); U.toast('Logged in to Lark', 'as ' + M.people.me.name); location.reload(); }, 1800);
  }

  /* ---------------- sync indicator ---------------- */
  function renderSync() {
    var el = $('#sync');
    if (state.syncing) { el.dataset.state = ''; el.innerHTML = ic('loader-circle', 'spin icon-sm') + '<span class="sync-text">Syncing with Lark…</span>'; }
    else if (state.syncError) { el.dataset.state = 'error'; el.innerHTML = ic('refresh-cw-off', 'icon-sm') + '<span class="sync-text">Last sync failed 2m ago: Lark returned 503</span>'; }
    else { el.dataset.state = ''; el.innerHTML = ic('check', 'icon-sm') + '<span class="sync-text">Synced ' + U.relTime(Math.round((Date.now() - state.syncedAt) / 1000)) + '</span>'; }
    U.icons();
  }
  setInterval(function () { if (!state.syncing) renderSync(); }, 5000);

  /* TaskCard renderer lives in js/components.js (shared with components.html) */
  var card = UI.taskCard;

  /* ---------------- board ---------------- */
  function filtered() {
    return M.allTasks().filter(function (t) {
      if (state.sprint !== 'all' && t.sprint !== state.sprint) return false;
      if (state.scope === 'mine' && t.owner && t.owner !== 'me' && !t.run) return false;
      if (state.q) { var q = state.q.toLowerCase(); if ((t.key + ' ' + t.title).toLowerCase().indexOf(q) === -1) return false; }
      return true;
    });
  }
  function order(list) {
    var rank = { awaiting_dev: 0, failed: 1, orphaned: 2, running: 3, preparing: 3, publishing: 3, wf_running: 3, wf_quiet: 4, wf_awaiting_comment: 5, in_review_loop: 6, stopped: 2 };
    return list.sort(function (a, b) {
      var ra = a.flag ? 1.5 : (a.run ? rank[a.run.state] : 7), rb = b.flag ? 1.5 : (b.run ? rank[b.run.state] : 7);
      if (a.column === 'todo') { ra = M.startGate(a) ? 1 : 0; rb = M.startGate(b) ? 1 : 0; }
      return ra - rb;
    });
  }
  function renderBoard() {
    var el = $('#board');
    if (U.larkState() === 'loggedout') {
      el.innerHTML = '<div class="empty">' + '<span class="ico">' + ic('log-in') + '</span><h3>Log in to Lark to load your tasks</h3><p>Mandor reads your Sprint Tasks from Lark Base. Your login opens in the browser and comes back here.</p><button class="btn btn-default" id="em-login">' + ic('log-in') + 'Log in with Lark</button></div>';
      $('#em-login').onclick = fakeLogin; U.icons(); return;
    }
    if (state.loading) {
      el.innerHTML = ['Todo', 'In Progress', 'Review', 'Done'].map(function (n) {
        return '<section class="col"><div class="col-head"><h2>' + n + '</h2></div><div class="col-body">' +
          [1, 2, 3].map(function () { return '<div class="task-card" aria-hidden="true"><div class="skeleton" style="height:10px;width:30%"></div><div class="skeleton" style="height:12px;width:90%"></div><div class="skeleton" style="height:12px;width:60%"></div></div>'; }).join('') + '</div></section>';
      }).join('');
      return;
    }
    var list = filtered();
    if (!list.length) {
      var sprintEmpty = !state.q && state.sprint !== 'all';
      el.innerHTML = '<div class="empty">' + '<span class="ico">' + ic(sprintEmpty ? 'calendar-x-2' : 'search-x') + '</span>' +
        (sprintEmpty ? '<h3>No tasks assigned to you in ' + esc($('#f-sprint').selectedOptions[0].text.split(' · ')[0]) + '</h3><p>Tasks appear here when you are an assignee in Lark Sprint Tasks.</p>'
          : '<h3>No cards match "' + esc(state.q) + '"</h3><p>The search and filters are hiding every card on the board.</p>') +
        '<button class="btn btn-outline" id="em-clear">Clear filters</button></div>';
      $('#em-clear').onclick = function () { state.q = ''; $('#f-search').value = ''; state.sprint = 'all'; $('#f-sprint').value = 'all'; state.scope = 'all'; paintScope(); renderBoard(); };
      U.icons(); return;
    }
    el.innerHTML = M.columns.map(function (c) {
      var cards = order(list.filter(function (t) { return t.column === c.id; }));
      if (c.hideWhenEmpty && !cards.length) return '';
      var collapsed = c.collapsible && state.doneCollapsed;
      return '<section class="col" data-col="' + c.id + '"' + (collapsed ? ' data-collapsed="true"' : '') + ' aria-label="' + c.name + ', ' + cards.length + ' cards">' +
        '<div class="col-head"><h2>' + c.name + '</h2><span class="count">' + cards.length + '</span>' +
        (c.collapsible ? '<button class="btn btn-ghost btn-xs btn-icon" id="toggle-done" aria-expanded="' + !collapsed + '" aria-label="' + (collapsed ? 'Expand' : 'Collapse') + ' Done column">' + ic(collapsed ? 'chevrons-left-right' : 'chevrons-right-left') + '</button>' : '') +
        '</div><div class="col-body scroll-area">' +
        (cards.length ? cards.map(card).join('') : '<div class="col-empty">' + (c.id === 'review' ? 'No PRs waiting for review' : 'Nothing here') + '</div>') +
        '</div></section>';
    }).join('');
    var td = $('#toggle-done'); if (td) td.onclick = function () { state.doneCollapsed = !state.doneCollapsed; renderBoard(); };
    var f = U.params.get('focus');
    if (f) { var fc = $('.task-card[data-key="' + f + '"]'); if (fc) { fc.style.outline = '2px solid var(--ring)'; fc.style.outlineOffset = '2px'; } }
    U.icons();
  }

  /* ---------------- toolbar ---------------- */
  $('#f-sprint').onchange = function (e) { state.sprint = e.target.value; renderBoard(); };
  $('#f-search').oninput = function (e) { state.q = e.target.value.trim(); renderBoard(); };
  function paintScope() { $$('[data-scope]').forEach(function (b) { b.setAttribute('aria-pressed', String(b.dataset.scope === state.scope)); }); }
  $$('[data-scope]').forEach(function (b) { b.onclick = function () { state.scope = b.dataset.scope; paintScope(); renderBoard(); }; });
  function refresh() {
    state.syncing = true; renderSync();
    setTimeout(function () { state.syncing = false; state.syncError = false; state.syncedAt = Date.now(); renderSync(); renderBoard(); U.toast('Board refreshed', 'Read 23 records from Sprint Tasks', 'refresh-cw'); }, 1100);
  }
  $('#btn-refresh').onclick = refresh;
  window.addEventListener('mandor:refresh', refresh);
  window.addEventListener('mandor:change', renderBoard);

  /* ---------------- no-drag hint ---------------- */
  var drag = null, hinted = false;
  $('#board').addEventListener('pointerdown', function (e) {
    var c = e.target.closest('.task-card'); if (!c || e.target.closest('button,a')) return;
    drag = { x: e.clientX, y: e.clientY, card: c };
  });
  window.addEventListener('pointermove', function (e) {
    if (!drag || hinted) return;
    if (Math.abs(e.clientX - drag.x) + Math.abs(e.clientY - drag.y) > 14) {
      hinted = true; drag = null;
      U.toast('Cards can\'t be dragged', 'They move when you Start, create a PR, or when the status changes in Lark.', 'move');
      setTimeout(function () { hinted = false; }, 6000);
    }
  });
  window.addEventListener('pointerup', function () { drag = null; });

  /* ---------------- card actions ---------------- */
  $('#board').addEventListener('click', function (e) {
    var b = e.target.closest('[data-act]'); var m = e.target.closest('[data-menu]');
    if (m) { openMenu(m); return; }
    if (!b) return;
    var key = b.dataset.key, t = M.getTask(key);
    if (b.dataset.act === 'start') openStart(t);
    if (b.dataset.act === 'takeover') openTakeover(t);
    if (b.dataset.act === 'retry') {
      b.disabled = true; b.innerHTML = ic('loader-circle', 'spin icon-sm') + 'Retrying'; U.icons();
      setTimeout(function () { M.update(key, { flag: null, status: 'Waiting Review' }); U.toast('Synced ' + key, 'Lark moved to Waiting Review', 'circle-check'); }, 1000);
    }
    if (b.dataset.act === 'close') confirmClose(t);
  });

  function openMenu(btn) {
    var t = M.getTask(btn.dataset.menu), menu = $('#card-menu');
    menu.innerHTML = '<button class="menu-item" role="menuitem" data-m="lark">' + ic('external-link') + 'Open in Lark</button>' +
      '<button class="menu-item" role="menuitem" data-m="key">' + ic('copy') + 'Copy task key</button>' +
      (t.run && t.run.branch ? '<button class="menu-item" role="menuitem" data-m="branch">' + ic('git-branch') + 'Copy branch name</button>' : '') +
      (t.run ? '<div class="menu-sep"></div><a class="menu-item" role="menuitem" href="workspace.html?task=' + t.key + '">' + ic('panel-right-open') + 'Open workspace</a>' : '');
    var r = btn.getBoundingClientRect();
    menu.style.top = (r.bottom + 4) + 'px'; menu.style.left = Math.min(r.right - 200, innerWidth - 210) + 'px';
    menu.hidden = false; U.icons(); menu.querySelector('.menu-item').focus();
    menu.onclick = function (e) {
      var it = e.target.closest('[data-m]'); if (!it) return;
      if (it.dataset.m === 'key') { navigator.clipboard && navigator.clipboard.writeText(t.key); U.toast('Copied ' + t.key, null, 'copy'); }
      if (it.dataset.m === 'branch') { navigator.clipboard && navigator.clipboard.writeText(t.run.branch); U.toast('Copied branch name', t.run.branch, 'copy'); }
      if (it.dataset.m === 'lark') U.toast('Opening ' + t.key + ' in Lark', 'Sprint Tasks · record view', 'external-link');
      menu.hidden = true;
    };
  }
  document.addEventListener('click', function (e) { if (!e.target.closest('#card-menu') && !e.target.closest('[data-menu]')) $('#card-menu').hidden = true; });
  document.addEventListener('keydown', function (e) { if (e.key === 'Escape') $('#card-menu').hidden = true; });

  function repoOptions(sel, preselect) {
    var repos = M.getRepos();
    sel.innerHTML = repos.map(function (r) { return '<option value="' + r.id + '"' + (r.id === preselect ? ' selected' : '') + '>' + esc(r.id) + '  ·  ' + esc(r.path) + '</option>'; }).join('');
    return repos;
  }

  function stepsHtml(steps) {
    var icon = { todo: 'circle', active: 'loader-circle', done: 'circle-check', error: 'circle-x' };
    return steps.map(function (s) {
      return '<div class="step" data-s="' + s.s + '">' + ic(icon[s.s], s.s === 'active' ? 'spin' : '') + '<span>' + esc(s.label) + (s.err ? '<span class="err">' + esc(s.err) + '</span>' : '') + '</span></div>';
    }).join('');
  }
  function runSteps(listEl, labels, failAt, failMsg, done) {
    var steps = labels.map(function (l) { return { label: l, s: 'todo' }; });
    var i = 0;
    (function tick() {
      if (i > 0) steps[i - 1].s = 'done';
      if (i === failAt) { steps[i].s = 'error'; steps[i].err = failMsg; listEl.innerHTML = stepsHtml(steps); U.icons(); done(false); return; }
      if (i >= steps.length) { listEl.innerHTML = stepsHtml(steps); U.icons(); done(true); return; }
      steps[i].s = 'active'; listEl.innerHTML = stepsHtml(steps); U.icons(); i++;
      setTimeout(tick, 750 + Math.random() * 400);
    })();
  }

  /* ---------------- Start dialog additions ----------------
     Release 1 as built: One repo | Workspace, Before coding with skill pickers.
     Release 2: Reviewer picker, Design sources, (2.1) task design link.
     Choices are remembered in mandor:v1:startprefs. */
  var PKEY = 'mandor:v1:startprefs';
  var SKILLS = { brainstorm: ['Mandor built-in', 'compound-engineering:ce-brainstorm'], plan: ['Mandor built-in', 'compound-engineering:ce-plan'], review: ['Mandor built-in', 'compound-engineering:ce-code-review'] };
  function prefs() { try { return JSON.parse(localStorage.getItem(PKEY)) || {}; } catch (e) { return {}; } }
  function startExtras(t) {
    var p = prefs(), el = $('#st-r2');
    var mode = t.repos ? 'workspace' : 'repo';
    var before = p.before || 'brainstorm';
    var sources = M.getSources();
    var remembered = p.sources || ['kelola-design', 'pegasus-mock'];
    var missing = sources.filter(function (s) { return remembered.indexOf(s.id) > -1 && s.status === 'error'; });
    var sel = remembered.filter(function (id) { return sources.some(function (s) { return s.id === id && s.status !== 'error'; }); });
    function skillSel(id, stage, val) { return '<select class="select input-sm" id="' + id + '" aria-label="' + stage + ' skill">' + SKILLS[stage].map(function (k) { return '<option' + (k === val ? ' selected' : '') + '>' + esc(k) + '</option>'; }).join('') + '</select>'; }
    var detected = 'https://www.figma.com/design/k3LvQ/Kelola-v2?node-id=12-40';
    el.innerHTML =
      '<div class="field"><span class="label">Where the agent works</span>' +
        '<div class="toggle-group" role="radiogroup" aria-label="Repo or workspace" id="st-mode"><button type="button" data-mode="repo" aria-pressed="' + (mode === 'repo') + '">One repo</button><button type="button" data-mode="workspace" aria-pressed="' + (mode === 'workspace') + '">Workspace</button></div>' +
        '<div id="st-ws" style="margin-top:8px"' + (mode === 'workspace' ? '' : ' hidden') + '><select class="select" id="st-ws-sel" aria-label="Workspace"><option>kelola-v2  ·  /home/beni/work/kelola-v2</option></select>' +
          '<div class="st-checks">' + ['kelola-api', 'kelola-web'].map(function (r) { return '<label class="env-key"><input type="checkbox" class="checkbox" checked data-wsrepo="' + r + '"><span class="mono">' + r + '</span></label>'; }).join('') + '</div>' +
          '<p class="field-help">One branch for all repos; one PR per changed repo.</p></div></div>' +
      '<div class="field"><span class="label">Before coding</span><div class="radio-row" role="radiogroup" aria-label="Before coding">' +
        [['brainstorm', 'Brainstorm, then plan'], ['plan', 'Plan only'], ['skip', 'Skip planning']].map(function (o) { return '<label class="radio-card"><input type="radio" name="st-before" value="' + o[0] + '"' + (o[0] === before ? ' checked' : '') + '>' + o[1] + '</label>'; }).join('') + '</div>' +
        '<div class="st-skills" id="st-skills">' +
          '<label class="st-skill" data-stage="brainstorm"><span class="muted">Brainstorm skill</span>' + skillSel('st-sk-bs', 'brainstorm', p.bsSkill || SKILLS.brainstorm[0]) + '</label>' +
          '<label class="st-skill" data-stage="plan"><span class="muted">Plan skill</span>' + skillSel('st-sk-pl', 'plan', p.plSkill || SKILLS.plan[1]) + '</label></div></div>' +
      '<div class="field"><label class="label" for="st-sk-rv">Reviewer <span class="muted" style="font-weight:400">(checks every AC before the PR)</span></label>' + skillSel('st-sk-rv', 'review', p.rvSkill || SKILLS.review[0]) + '</div>' +
      '<div class="field"><span class="label">Design sources</span>' +
        (missing.length ? '<div class="alert alert-warning" style="margin-bottom:8px">' + ic('triangle-alert') + '<div><div class="alert-title">' + missing.map(function (s) { return esc(s.name); }).join(', ') + ' is no longer available</div><div class="muted">' + missing.map(function (s) { return esc(s.error); }).join(', ') + '. It was unselected. Fix it in Settings › Design sources.</div></div></div>' : '') +
        (sources.length ? '<div class="st-sources" role="group" aria-label="Design sources">' + sources.map(function (s) {
          var k = U.ac.SOURCE_KINDS[s.kind], bad = s.status === 'error';
          return '<label class="st-source"' + (k.release === '2.1' ? ' data-release="2.1"' : '') + (bad ? ' data-bad="true"' : '') + '><input type="checkbox" class="checkbox" data-src="' + esc(s.id) + '"' + (sel.indexOf(s.id) > -1 ? ' checked' : '') + (bad || k.release === '2.1' ? ' disabled' : '') + (k.release === '2.1' ? ' title="Available in Release 2.1"' : '') + '>' + ic(k.icon, 'icon-sm') +
            '<span class="ss-name">' + esc(s.name) + '</span>' + (k.release === '2.1' ? '<span class="rel-tag">2.1</span>' : '') + '<span class="ss-loc mono">' + esc(bad ? s.error : s.location) + '</span></label>';
        }).join('') + '</div>'
        : '<div class="st-empty">' + ic('frame', 'icon-sm') + '<span>No design sources yet. Planning skips the design check.</span><a class="link" href="settings.html#design">Add one in Settings</a></div>') + '</div>' +
      '<div class="field rel-block" data-release="2.1"><span class="label">Design link for this task <span class="rel-tag">2.1</span> <span class="muted" style="font-weight:400">(optional)</span></span>' +
        '<input class="input" id="st-link" placeholder="https://… link to the screens for this task" aria-describedby="st-link-help">' +
        '<div class="st-detected"><span class="muted">Found in the description:</span><span class="mono" title="' + esc(detected) + '"><b>www.figma.com</b>/design/k3LvQ/Kelola-v2</span><button type="button" class="btn btn-outline btn-xs" id="st-use-link">Use this link</button></div>' +
        '<p class="field-help" id="st-link-help">Links from the description are only used after you confirm them. The full host is shown before use.</p></div>';
    U.icons();
    function syncMode(m) { mode = m; $$('#st-mode [data-mode]').forEach(function (b) { b.setAttribute('aria-pressed', String(b.dataset.mode === m)); }); $('#st-ws').hidden = m !== 'workspace'; $('#st-repo').parentElement.hidden = m === 'workspace'; }
    $$('#st-mode [data-mode]').forEach(function (b) { b.onclick = function () { syncMode(b.dataset.mode); }; });
    syncMode(mode);
    function syncBefore() { var v = $('input[name="st-before"]:checked').value; $('[data-stage="brainstorm"]').hidden = v !== 'brainstorm'; $('[data-stage="plan"]').hidden = v === 'skip'; }
    $$('input[name="st-before"]').forEach(function (r) { r.onchange = syncBefore; }); syncBefore();
    $('#st-use-link').onclick = function () { $('#st-link').value = detected; U.toast('Using the Figma link from the description', 'Host: www.figma.com', 'link-2'); };
    return function () {
      var v = {
        mode: mode, before: $('input[name="st-before"]:checked').value,
        bsSkill: $('#st-sk-bs').value, plSkill: $('#st-sk-pl').value, rvSkill: $('#st-sk-rv').value,
        sources: $$('[data-src]').filter(function (c) { return c.checked; }).map(function (c) { return c.dataset.src; })
      };
      localStorage.setItem(PKEY, JSON.stringify(Object.assign({}, p, v, { sources: v.sources.length ? v.sources : p.sources })));
      return v;
    };
  }

  /* ---------------- Start dialog ---------------- */
  function openStart(t) {
    var gate = M.startGate(t); if (gate) { U.toast('Can\'t start ' + t.key, gate, 'lock'); return; }
    $('#st-key').textContent = t.key; $('#st-title').textContent = t.title;
    $('#st-ready').innerHTML = '<span class="badge" data-ui="Badge / variant=outline">' + ic('circle-check') + esc(t.readiness) + '</span>';
    $('#st-form').hidden = false; $('#st-steps').hidden = true; $('#st-note').value = '';
    var repos = repoOptions($('#st-repo'), t.repo);
    $('#st-repo-help').textContent = repos.length ? 'Last used for this task: ' + t.repo + '. Worktree is created from origin/' + (repos.find(function (r) { return r.id === t.repo; }) || repos[0]).branch + '.' : '';
    $('#st-repo').parentElement.hidden = !repos.length;
    $('#st-norepo').innerHTML = repos.length ? '' : '<div class="alert alert-warning">' + ic('folder-x') + '<div><div class="alert-title">No repo added yet</div><div class="muted">The agent needs a git repo on the daemon machine.</div></div><div class="alert-actions"><a class="btn btn-outline btn-sm" href="settings.html#repos">Add a repo</a></div></div>';
    var choices = startExtras(t);
    var foot = $('#st-foot');
    foot.innerHTML = '<span class="left"><span class="kbd">Esc</span> to cancel</span><button class="btn btn-ghost" data-close>Cancel</button><button class="btn btn-default" id="st-go"' + (repos.length ? '' : ' disabled') + '>' + ic('play') + 'Start</button>';
    $('#st-go').onclick = function () {
      var c = choices();
      $('#st-form').hidden = true; $('#st-steps').hidden = false;
      foot.innerHTML = '<span class="left">Starting ' + t.key + '…</span><button class="btn btn-default" disabled>' + ic('loader-circle', 'spin') + 'Starting</button>'; U.icons();
      $('#dlg-start').setAttribute('data-static', '');
      var branch = 'beni/' + t.key.toLowerCase() + '-' + t.title.toLowerCase().replace(/[^a-z0-9]+/g, '-').split('-').slice(0, 3).join('-');
      runSteps($('#st-step-list'), ['Claiming the task in Lark', t.repos && c.mode === 'workspace' ? 'Creating worktrees on ' + branch + ' in kelola-api and kelola-web' : 'Creating worktree on ' + branch, c.before === 'skip' ? 'Starting the agent' : 'Starting the agent (' + (c.before === 'brainstorm' ? 'brainstorm, then plan' : 'plan only') + ')', 'Moving Lark status to In Progress'],
        t.simulateClaimLost ? 0 : -1, 'Lost the claim: another developer started this task at the same time. Nothing was created.',
        function (ok) {
          $('#dlg-start').removeAttribute('data-static');
          if (!ok) { foot.innerHTML = '<span class="left"></span><button class="btn btn-ghost" data-close>Close</button><button class="btn btn-secondary" id="st-refresh">' + ic('refresh-cw') + 'Refresh board</button>'; U.icons(); $('#st-refresh').onclick = function () { U.closeDialog('dlg-start'); refresh(); }; return; }
          /* The prototype simulates planning on the kelola-v2 workspace task only */
          var planned = !!t.repos && c.before !== 'skip';
          if (t.repos) localStorage.removeItem('mandor:v1:ac:' + t.key);
          M.update(t.key, { status: 'In Progress', owner: 'me', run: { state: planned ? (c.before === 'brainstorm' ? 'brainstorming' : 'planning') : 'running', branch: branch, since: 'now', fresh: true, planned: planned, approved: !planned, skill: c.bsSkill, reviewer: c.rvSkill, sources: c.sources } });
          foot.innerHTML = '<span class="left">Agent is working on ' + t.key + '</span><button class="btn btn-ghost" data-close>Stay on board</button><a class="btn btn-default" href="workspace.html?task=' + t.key + '">Open workspace' + ic('arrow-right') + '</a>';
          U.icons();
        });
    };
    U.openDialog('dlg-start');
  }

  /* ---------------- Take over dialog ---------------- */
  function openTakeover(t) {
    var p = M.people[t.owner];
    $('#to-key').textContent = t.key; $('#to-title').textContent = 'Take over from ' + p.name;
    $('#to-desc').textContent = p.name + '\'s claim has been inactive for ' + t.ownerSeen.replace(' ago', '') + ' (last seen 09:03). Taking over moves the claim in Lark to you.';
    $('#to-warn').innerHTML = '<div class="alert-title">' + esc(p.name.split(' ')[0]) + '\'s local work is not visible here</div><div class="muted">Mandor restores the branch <span class="mono">dimas/' + t.key.toLowerCase() + '-map-cluster</span> from origin. Unpushed commits on their machine stay there.</div>';
    repoOptions($('#to-repo'), t.repo);
    $('#to-form').hidden = false; $('#to-steps').hidden = true;
    var foot = $('#to-foot');
    foot.innerHTML = '<button class="btn btn-ghost" data-close>Cancel</button><button class="btn btn-default" id="to-go">' + ic('arrow-left-right') + 'Take over</button>';
    $('#to-go').onclick = function () {
      $('#to-form').hidden = true; $('#to-steps').hidden = false;
      foot.innerHTML = '<button class="btn btn-default" disabled>' + ic('loader-circle', 'spin') + 'Taking over</button>'; U.icons();
      runSteps($('#to-step-list'), ['Moving the claim to you in Lark', 'Restoring branch from origin', 'Starting the agent'], -1, '', function () {
        M.update(t.key, { owner: 'me', ownerActive: true, run: { state: 'awaiting_dev', branch: 'dimas/' + t.key.toLowerCase() + '-map-cluster', since: 'now' } });
        foot.innerHTML = '<button class="btn btn-ghost" data-close>Stay on board</button><a class="btn btn-default" href="workspace.html?task=' + t.key + '">Open workspace' + ic('arrow-right') + '</a>'; U.icons();
      });
    };
    U.openDialog('dlg-takeover');
  }

  function confirmClose(t) {
    $('#cf-title').textContent = 'Close the run for ' + t.key + '?';
    $('#cf-desc').textContent = M.flags[t.flag.type].label + '. Mandor stops the agent and removes the worktree. The branch ' + t.run.branch + ' stays on origin.';
    $('#cf-go').textContent = 'Close run';
    $('#cf-go').onclick = function () { U.closeDialog('dlg-confirm'); M.update(t.key, { closed: true }); U.toast('Run closed', t.key + ' · worktree removed', 'circle-check'); };
    U.openDialog('dlg-confirm');
  }

  /* ---------------- boot ---------------- */
  renderBanners(); renderSync(); renderBoard();
  setTimeout(function () {
    state.loading = false; renderBoard();
    var s = U.params.get('start'); if (s) { var t = M.getTask(s); if (t) openStart(t); }
    var to = U.params.get('takeover'); if (to) { var tt = M.getTask(to); if (tt && tt.owner && tt.owner !== 'me' && !tt.ownerActive) openTakeover(tt); }
    if (U.params.get('refresh')) refresh();
  }, 450);
})();
