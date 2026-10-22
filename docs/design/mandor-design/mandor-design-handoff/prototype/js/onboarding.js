/* First run wizard: Welcome, Prerequisites, Connect Lark app, Log in with
   Lark, Add a repo, Done. ?step=1..6 opens a step; on step 4,
   ?login=waiting|ok|port|state|denied|timeout opens that login state. */
(function () {
  var U = window.UI, M = window.MANDOR, esc = U.esc, ic = U.ic, $ = U.$, $$ = U.$$;
  var STEPS = ['Welcome', 'Prerequisites', 'Lark app', 'Log in', 'Repo', 'Done'];
  var step = Math.min(6, Math.max(1, +(U.params.get('step') || 1)));
  var st = {
    prereq: { claude: 'ok', gh: 'ok', larkConfirmed: false },
    lark: { appId: '', secret: '', appToken: '', tasksTable: '', acTable: '', port: '53682' },
    login: U.params.get('login') || 'idle',
    repo: null
  };
  if (step > 3) st.lark = { appId: 'cli_a6f2c81d93b8d00e', secret: 'saved', appToken: 'NkQ2bXr7YcPq1sZt9vLmHg4eAbc', tasksTable: 'tbl8kX2pQm4vR7aW', acTable: 'tblD3nF6hJ9kL2pS', port: '53682' };
  if (step > 4) st.login = 'ok';
  if (step > 5) st.repo = { id: 'pegasus-web', path: '/home/beni/work/pegasus-web', branch: 'main', gh: 'pegasus-bot' };
  if (step > 2) st.prereq.larkConfirmed = true;
  var loginTimer = null;

  function stepper() {
    $('#stepper').innerHTML = STEPS.map(function (s, i) {
      var n = i + 1, state = n < step ? 'done' : n === step ? 'current' : 'todo';
      return (i ? '<li aria-hidden="true" class="sep"></li>' : '') + '<li data-s="' + state + '"' + (state === 'current' ? ' aria-current="step"' : '') + '><span class="num">' + (state === 'done' ? ic('check', 'icon-sm') : n) + '</span><span class="lbl">' + s + '</span></li>';
    }).join('');
  }
  function foot(back, next, extra) {
    $('#ob-foot').innerHTML = '<span>' + (back ? '<button class="btn btn-ghost" id="ob-back">' + ic('arrow-left') + 'Back</button>' : '') + '</span><span style="display:flex;gap:8px">' + (extra || '') + (next || '') + '</span>';
    var b = $('#ob-back'); if (b) b.onclick = function () { go(step - 1); };
  }
  function go(n) { clearTimeout(loginTimer); step = n; render(); var h = $('#ob-body h1'); if (h) { h.setAttribute('tabindex', '-1'); h.focus(); } }
  function field(id, label, value, help, type) {
    return '<div class="field"><label class="label" for="' + id + '">' + label + '</label><input class="input mono" id="' + id + '"' + (type ? ' type="' + type + '"' : '') + ' value="' + esc(value) + '" autocomplete="off" spellcheck="false"><p class="field-help">' + help + '</p><p class="field-error" id="' + id + '-err" hidden></p></div>';
  }
  function setErr(id, msg) {
    var el = $('#' + id), err = $('#' + id + '-err');
    el.setAttribute('aria-invalid', msg ? 'true' : 'false'); err.hidden = !msg;
    err.innerHTML = msg ? ic('circle-alert', 'icon-sm') + esc(msg) : '';
    if (msg) el.setAttribute('aria-describedby', id + '-err');
    return !msg;
  }

  /* ---------- 1. Welcome ---------- */
  function welcome() {
    $('#ob-body').innerHTML = '<h1>Run several agents without losing track of them</h1>' +
      '<p class="lede">Mandor watches the Claude Code agents working on your Lark Sprint Tasks, one worktree each, and keeps Lark in sync.</p>' +
      '<div class="points">' +
        '<div class="point"><span class="ico">' + ic('hand') + '</span><div><b>See who needs you</b><span>Every agent in one sidebar. The ones waiting for your reply are pinned to the top.</span></div></div>' +
        '<div class="point"><span class="ico">' + ic('square-kanban') + '</span><div><b>Start straight from Lark</b><span>Press Start on a ready task. Mandor claims it, creates a worktree, and moves Lark to In Progress.</span></div></div>' +
        '<div class="point"><span class="ico">' + ic('wrench') + '</span><div><b>Review fixes run on their own</b><span>When a reviewer moves a task to Waiting Fix, trusted comments go to the agent after a short quiet period.</span></div></div>' +
      '</div>';
    foot(false, '<button class="btn btn-default" id="ob-next">Get started' + ic('arrow-right') + '</button>');
    $('#ob-next').onclick = function () { go(2); };
  }

  /* ---------- 2. Prerequisites ---------- */
  function prereq() {
    var p = st.prereq;
    function row(s, title, desc, how, extra) {
      var icon = { ok: 'circle-check', missing: 'circle-alert', checking: 'loader-circle' }[s];
      var label = { ok: 'Found', missing: 'Not confirmed', checking: 'Checking' }[s];
      return '<div class="pr-row"><span class="st" data-s="' + s + '" title="' + label + '">' + ic(icon, s === 'checking' ? 'spin' : '') + '<span class="sr-only">' + label + '</span></span>' +
        '<div><div class="t">' + title + '</div><div class="d">' + desc + '</div>' + (extra || '') + '</div>' +
        '<a class="btn btn-link btn-sm" href="#" data-how="' + esc(title) + '">How to</a></div>';
    }
    $('#ob-body').innerHTML = '<h1>Check what Mandor needs</h1><p class="lede">The daemon checks these on the machine where agents run (inside WSL on Windows).</p>' +
      '<div class="prereq" data-ui="PrerequisiteList">' +
        row(p.claude, 'Claude Code is logged in', p.claude === 'ok' ? '<span class="mono">claude 2.0.14</span> · subscription login found' : 'Checking…') +
        row(p.gh, 'GitHub CLI <span class="mono">gh</span> is logged in', p.gh === 'ok' ? '<span class="mono">gh 2.62.0</span> · profiles: default, pegasus-bot' : 'Checking…') +
        row(p.larkConfirmed ? 'ok' : 'missing', 'Lark Custom App is created', 'Mandor can\'t detect this. Create the app in the Lark Developer Console with the Bitable and offline access scopes.',
          '', '<label class="confirm"><input type="checkbox" class="checkbox" id="lark-made"' + (p.larkConfirmed ? ' checked' : '') + '>I created the app and have its App ID and App Secret</label>') +
      '</div>';
    foot(true, '<button class="btn btn-default" id="ob-next"' + (p.larkConfirmed && p.claude === 'ok' && p.gh === 'ok' ? '' : ' disabled') + '>Continue' + ic('arrow-right') + '</button>',
      '<button class="btn btn-ghost" id="recheck">' + ic('refresh-cw') + 'Check again</button>');
    $('#lark-made').onchange = function (e) { p.larkConfirmed = e.target.checked; prereq(); U.icons(); };
    $('#recheck').onclick = function () { p.claude = p.gh = 'checking'; prereq(); U.icons(); setTimeout(function () { p.claude = p.gh = 'ok'; if (step === 2) { prereq(); U.icons(); } }, 1200); };
    $('#ob-next').onclick = function () { go(3); };
    $$('[data-how]').forEach(function (a) { a.onclick = function (e) { e.preventDefault(); U.toast('Opened the setup guide', a.dataset.how.replace(/<[^>]+>/g, ''), 'book-open'); }; });
  }

  /* ---------- 3. Connect Lark app ---------- */
  function larkApp() {
    var l = st.lark;
    $('#ob-body').innerHTML = '<h1>Connect your Lark app</h1><p class="lede">Values from the Lark Developer Console and from the Sprint Tasks URL: <span class="mono">…/base/<b>app_token</b>?table=<b>tbl…</b></span></p>' +
      '<form id="lark-form" novalidate style="margin-top:18px"><div class="grid-2">' +
        field('f-appid', 'App ID', l.appId === 'cli_a6f2c81d93b8d00e' ? l.appId : l.appId, 'Credentials &amp; Basic Info. Starts with <span class="mono">cli_</span>.') +
        (l.secret === 'saved'
          ? '<div class="field"><label class="label" for="f-secret">App Secret</label><input class="input" type="password" id="f-secret" placeholder="•••••••• saved" disabled><p class="field-help">Saved by the daemon. Never shown again.</p><p class="field-error" id="f-secret-err" hidden></p></div>'
          : field('f-secret', 'App Secret', l.secret, 'Credentials &amp; Basic Info. Stored by the daemon, never shown again.', 'password')) +
        field('f-token', 'Base app_token', l.appToken, 'The part after <span class="mono">/base/</span> in the Sprint Tasks URL.') +
        field('f-tasks', 'Sprint Tasks table ID', l.tasksTable, 'The <span class="mono">table=</span> value with Sprint Tasks open.') +
        field('f-ac', 'Acceptance Criteria table ID', l.acTable, 'The <span class="mono">table=</span> value with the AC table open.') +
      '</div><details style="margin-top:14px"' + (U.params.get('advanced') ? ' open' : '') + '><summary class="muted" style="cursor:pointer;font-size:12px">Advanced</summary><div style="margin-top:12px;max-width:240px">' +
        field('f-port', 'Callback port', l.port, 'Must match the redirect URL in the Lark app. Default 53682.') + '</div></details></form>';
    foot(true, '<button class="btn btn-default" id="ob-next">Save and continue' + ic('arrow-right') + '</button>');
    $('#ob-next').onclick = function () {
      var v = function (id) { return $('#' + id).value.trim(); };
      var ok = [
        setErr('f-appid', !v('f-appid') ? 'App ID is required.' : !/^cli_[a-z0-9]{8,}$/.test(v('f-appid')) ? 'App ID should look like cli_a6f2c81d93b8d00e.' : ''),
        setErr('f-secret', l.secret !== 'saved' && !v('f-secret') ? 'App Secret is required.' : ''),
        setErr('f-token', !v('f-token') ? 'Base app_token is required.' : v('f-token').length < 20 ? 'This looks too short for an app_token.' : ''),
        setErr('f-tasks', !v('f-tasks') ? 'Sprint Tasks table ID is required.' : !/^tbl/.test(v('f-tasks')) ? 'Table IDs start with tbl.' : ''),
        setErr('f-ac', !v('f-ac') ? 'Acceptance Criteria table ID is required.' : !/^tbl/.test(v('f-ac')) ? 'Table IDs start with tbl.' : ''),
        setErr('f-port', !/^\d{4,5}$/.test(v('f-port')) || +v('f-port') < 1024 || +v('f-port') > 65535 ? 'Use a port between 1024 and 65535.' : '')
      ].every(Boolean);
      U.icons();
      if (!ok) { var bad = $('[aria-invalid="true"]'); if (bad) bad.focus(); return; }
      st.lark = { appId: v('f-appid'), secret: 'saved', appToken: v('f-token'), tasksTable: v('f-tasks'), acTable: v('f-ac'), port: v('f-port') };
      go(4);
    };
  }

  /* ---------- 4. Log in with Lark ---------- */
  var LOGIN_ERRORS = {
    port: ['Port ' + '53682' + ' is already in use', 'Another app is listening on the callback port. Close it, or pick a different port in Advanced and update the redirect URL in your Lark app.', 'Change port'],
    state: ['Login was cancelled', 'The browser returned a response that does not match this login (state mismatch). Start the login again from here.', null],
    denied: ['You declined access in Lark', 'Mandor needs permission to read Sprint Tasks and update Task Status on your behalf. Log in again and choose Allow.', null],
    timeout: ['No response from the browser', 'Nothing came back after 2 minutes. If the browser tab is still open, finish the login there, or start again.', null]
  };
  function login() {
    var s = st.login, html = '<h1>Log in with Lark</h1><p class="lede">Mandor opens Lark in your browser. Status changes are written as you, so reviewers see who moved a task.</p><div style="margin-top:20px">';
    if (s === 'idle' || s === 'cancelled') html += (s === 'cancelled' ? '<div class="alert" style="margin-bottom:12px">' + ic('info') + '<div class="muted">Login cancelled. Nothing was saved.</div></div>' : '') +
      '<button class="btn btn-default" id="do-login">' + ic('external-link') + 'Log in with Lark</button><p class="field-help" style="margin-top:8px">Opens <span class="mono">open.larksuite.com</span>; Mandor listens on <span class="mono">127.0.0.1:' + esc(st.lark.port) + '</span> for the reply.</p>';
    else if (s === 'waiting') html += '<div class="alert" data-ui="Alert">' + ic('loader-circle', 'spin') + '<div><div class="alert-title">Waiting for the browser…</div><div class="muted">Finish the login in the browser tab that just opened. This times out after 2 minutes.</div></div><div class="alert-actions"><button class="btn btn-ghost btn-sm" id="cancel-login">Cancel</button></div></div>';
    else if (s === 'ok') html += '<div class="alert" data-ui="Alert">' + ic('user-round-check') + '<div><div class="alert-title">Logged in as ' + M.people.me.name + '</div><div class="muted">Token refreshes automatically. You can log out in Settings › Lark.</div></div></div>';
    else { var e = LOGIN_ERRORS[s]; html += '<div class="alert alert-failed" role="alert" data-ui="Alert / variant=failed">' + ic('circle-x') + '<div><div class="alert-title">' + e[0] + '</div><div class="muted">' + e[1] + '</div></div></div>' +
      '<div style="display:flex;gap:8px;margin-top:12px"><button class="btn btn-secondary" id="do-login">' + ic('refresh-cw') + 'Try again</button>' + (e[2] ? '<button class="btn btn-ghost" id="change-port">' + e[2] + '</button>' : '') + '</div>'; }
    $('#ob-body').innerHTML = html + '</div>';
    foot(s !== 'waiting', '<button class="btn btn-default" id="ob-next"' + (s === 'ok' ? '' : ' disabled') + '>Continue' + ic('arrow-right') + '</button>');
    var d = $('#do-login'); if (d) d.onclick = function () { st.login = 'waiting'; render(); loginTimer = setTimeout(function () { st.login = 'ok'; render(); U.toast('Logged in to Lark', 'as ' + M.people.me.name, 'user-round-check'); }, 2600); };
    var c = $('#cancel-login'); if (c) c.onclick = function () { clearTimeout(loginTimer); st.login = 'cancelled'; render(); };
    var cp = $('#change-port'); if (cp) cp.onclick = function () { history.replaceState(null, '', '?step=3&advanced=1'); U.params.set('advanced', '1'); go(3); var f = $('#f-port'); if (f) f.focus(); };
    $('#ob-next').onclick = function () { go(5); };
  }

  /* ---------- 5. Add a repo ---------- */
  function repo() {
    var r = st.repo;
    $('#ob-body').innerHTML = '<h1>Add your first repo</h1><p class="lede">A git clone on the daemon machine. Agents work in separate worktrees under ~/.local/share/mandor/workspaces; your clone is not changed.</p>' +
      '<div style="margin-top:18px"><div class="field"><label class="label" for="r-path">Path</label><input class="input mono" id="r-path" placeholder="/home/beni/work/pegasus-web" value="' + esc(r ? r.path : '') + '" autocomplete="off" spellcheck="false"><div id="r-msg">' +
        (r ? '<p class="field-help" style="color:var(--status-ok);display:flex;gap:5px;align-items:center">' + ic('circle-check', 'icon-sm') + 'Git repo found. Default branch: <span class="mono">' + r.branch + '</span></p>' : '<p class="field-help">Absolute path with an <span class="mono">origin</span> remote.</p>') +
      '</div></div><div class="field" style="max-width:280px"><label class="label" for="r-gh">gh profile <span class="muted" style="font-weight:400">(optional)</span></label><select class="select" id="r-gh"><option>default</option><option' + (r && r.gh === 'pegasus-bot' ? ' selected' : '') + '>pegasus-bot</option></select></div></div>';
    foot(true, '<button class="btn btn-default" id="ob-next"' + (r ? '' : ' disabled') + '>Continue' + ic('arrow-right') + '</button>',
      '<button class="btn btn-outline" id="r-check"' + (r ? ' hidden' : '') + '>Check repo</button><button class="btn btn-ghost" id="r-skip">Add later</button>');
    $('#r-path').oninput = function () { if (st.repo) { st.repo = null; repo(); U.icons(); $('#r-path').focus(); } };
    $('#r-path').onkeydown = function (e) { if (e.key === 'Enter') $('#r-check').click(); };
    $('#r-check').onclick = function () {
      var v = $('#r-path').value.trim(), msg = $('#r-msg'), btn = $('#r-check');
      function fail(h) { $('#r-path').setAttribute('aria-invalid', 'true'); msg.innerHTML = '<div class="field-error" style="display:block">' + h + '</div>'; btn.disabled = false; btn.textContent = 'Check repo'; U.icons(); }
      if (!v || v[0] !== '/') return fail('Use an absolute path, e.g. /home/beni/work/pegasus-web.');
      btn.disabled = true; btn.innerHTML = ic('loader-circle', 'spin') + 'Checking'; U.icons();
      setTimeout(function () {
        if (/tmp|downloads/i.test(v)) return fail('Not a git repository: <span class="mono">' + esc(v) + '</span>');
        if (/fork|mirror/i.test(v)) return fail('<span class="mono">origin/HEAD</span> is missing. Fix it with:<pre class="mono" style="margin:6px 0 0;padding:6px 8px;border-radius:4px;background:var(--muted);color:var(--foreground)">git -C ' + esc(v) + ' remote set-head origin --auto</pre>');
        st.repo = { id: v.split('/').filter(Boolean).pop(), path: v, branch: 'main', gh: $('#r-gh').value };
        repo(); U.icons();
      }, 900);
    };
    $('#r-skip').onclick = function () { st.repo = null; go(6); };
    $('#ob-next').onclick = function () { go(6); };
  }

  /* ---------- 6. Done ---------- */
  function done() {
    var r = st.repo;
    $('#ob-body').innerHTML = '<h1>Mandor is ready</h1><p class="lede">Your Lark tasks are loading on the board. Press Start on a ready task to launch the first agent.</p>' +
      '<div class="summary">' +
        '<div>' + ic('circle-check') + '<span>Lark app <span class="mono">' + esc(st.lark.appId) + '</span> connected to Sprint Tasks</span></div>' +
        '<div>' + ic('circle-check') + '<span>Logged in to Lark as ' + M.people.me.name + '</span></div>' +
        (r ? '<div>' + ic('circle-check') + '<span>Repo <span class="mono">' + esc(r.id) + '</span> · origin/' + esc(r.branch) + '</span></div>'
           : '<div>' + ic('circle-dashed', '') + '<span class="muted">No repo yet. Add one in Settings › Repos before your first Start.</span></div>') +
      '</div>';
    $('#ob-body').querySelectorAll('.summary div:last-child [data-lucide="circle-dashed"]').forEach(function (i) { i.style.color = 'var(--muted-foreground)'; });
    foot(true, '<a class="btn btn-default" href="board.html">Open the board' + ic('arrow-right') + '</a>', '<a class="btn btn-ghost" href="settings.html">Settings</a>');
  }

  function render() {
    stepper();
    [welcome, prereq, larkApp, login, repo, done][step - 1]();
    U.icons();
  }
  render();
})();
