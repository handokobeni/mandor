/* Settings: Lark, Repos, General, About. Values are stored by the daemon
   in production; here they live in localStorage. */
(function () {
  var M = window.MANDOR, U = window.UI, esc = U.esc, ic = U.ic, $ = U.$, $$ = U.$$;
  U.initShell(null);
  var main = $('#settings-main');
  var selRepo = 0;
  var lark = JSON.parse(localStorage.getItem('mandor:larkcfg') || 'null') || {
    appId: 'cli_a6f2c81d93b8d00e', secretSaved: true, appToken: 'NkQ2bXr7YcPq1sZt9vLmHg4eAbc', tasksTable: 'tbl8kX2pQm4vR7aW', acTable: 'tblD3nF6hJ9kL2pS', port: '53682'
  };

  function section() { return (location.hash || '#lark').slice(1); }
  function nav() { $$('[data-sec]').forEach(function (a) { a.setAttribute('aria-current', a.dataset.sec === section() ? 'page' : 'false'); }); }
  window.addEventListener('hashchange', render);

  function field(id, label, value, help, opts) {
    opts = opts || {};
    return '<div class="field"><label class="label" for="' + id + '">' + label + '</label>' +
      (opts.secret
        ? '<div class="input-group"><input class="input mono" type="password" id="' + id + '" placeholder="' + (lark.secretSaved ? '•••••••• saved' : 'Paste the App Secret') + '"' + (lark.secretSaved ? ' disabled' : '') + ' autocomplete="off"><button class="btn btn-outline" type="button" id="replace-secret">' + (lark.secretSaved ? 'Replace' : 'Cancel') + '</button></div>'
        : '<input class="input mono" id="' + id + '" value="' + esc(value) + '" autocomplete="off" spellcheck="false">') +
      '<p class="field-help">' + help + '</p><p class="field-error" id="' + id + '-err" hidden></p></div>';
  }

  /* ---------------- Lark ---------------- */
  function larkSection() {
    var st = U.larkState();
    var status = st === 'expired'
      ? '<div class="alert alert-warning" data-ui="Alert / variant=warning">' + ic('triangle-alert') + '<div><div class="alert-title">Session expired at 08:41</div><div class="muted">Board sync is paused until you log in again. Agents keep running.</div></div><div class="alert-actions"><button class="btn btn-default btn-sm" id="lk-login">' + ic('log-in') + 'Log in again</button></div></div>'
      : st === 'loggedout'
      ? '<div class="alert" data-ui="Alert">' + ic('user-round-x') + '<div><div class="alert-title">Not logged in to Lark</div><div class="muted">Log in to read Sprint Tasks and write status changes as you.</div></div><div class="alert-actions"><button class="btn btn-default btn-sm" id="lk-login">' + ic('log-in') + 'Log in with Lark</button></div></div>'
      : st === 'waiting'
      ? '<div class="alert" data-ui="Alert">' + ic('loader-circle', 'spin') + '<div><div class="alert-title">Waiting for the browser…</div><div class="muted">Finish the Lark login in the browser window. Mandor listens on port ' + esc(lark.port) + '.</div></div><div class="alert-actions"><button class="btn btn-ghost btn-sm" id="lk-cancel">Cancel</button></div></div>'
      : '<div class="alert" data-ui="Alert">' + ic('user-round-check') + '<div><div class="alert-title">Logged in as ' + M.people.me.name + '</div><div class="muted">Token refreshes automatically. Status changes in Lark are written as you.</div></div><div class="alert-actions"><button class="btn btn-outline btn-sm" id="lk-logout">Log out</button></div></div>';
    return '<section><h1>Lark</h1><p class="lede">Connect the Lark Custom App and the Base that holds your Sprint Tasks. Lark stays the source of truth for titles, descriptions, AC, and assignees.</p></section>' +
      '<section>' + status + '</section>' +
      '<section><form class="card" id="lark-form" novalidate data-ui="Card / form=LarkApp"><div class="card-header"><div class="card-title">Custom App</div><div class="card-description">From the Lark Developer Console, under Credentials &amp; Basic Info.</div></div><div class="card-content"><div class="grid-2">' +
        field('f-appid', 'App ID', lark.appId, 'Starts with <span class="mono">cli_</span>.') +
        field('f-secret', 'App Secret', '', 'Stored by the daemon. Never shown again after saving.', { secret: true }) +
      '</div></div><div class="card-header" style="padding-top:0"><div class="card-title">Base</div><div class="card-description">Open Sprint Tasks in the browser: <span class="mono">…/base/<b>app_token</b>?table=<b>tbl…</b></span></div></div><div class="card-content"><div class="grid-2">' +
        field('f-token', 'Base app_token', lark.appToken, 'The part after <span class="mono">/base/</span> in the URL.') +
        field('f-tasks', 'Sprint Tasks table ID', lark.tasksTable, 'The <span class="mono">table=</span> value with Sprint Tasks open.') +
        field('f-ac', 'Acceptance Criteria table ID', lark.acTable, 'The <span class="mono">table=</span> value with the AC table open.') +
      '</div><details style="margin-top:14px" data-ui="Collapsible"><summary class="muted" style="cursor:pointer;font-size:12px">Advanced</summary><div style="margin-top:12px;max-width:240px">' +
        field('f-port', 'Callback port', lark.port, 'Must match the redirect URL in the Lark app. Default 53682.') +
      '</div></details></div><div class="card-footer"><button class="btn btn-ghost" type="reset">Reset</button><button class="btn btn-default" type="submit">Save</button></div></form></section>';
  }
  function bindLark() {
    var lg = $('#lk-login'); if (lg) lg.onclick = function () { localStorage.setItem('mandor:lark', 'waiting'); render(); setTimeout(function () { if (U.larkState() !== 'waiting') return; localStorage.setItem('mandor:lark', 'ok'); render(); U.initShell(null); U.toast('Logged in to Lark', 'as ' + M.people.me.name, 'user-round-check'); }, 2400); };
    var cc = $('#lk-cancel'); if (cc) cc.onclick = function () { localStorage.setItem('mandor:lark', 'loggedout'); render(); U.toast('Login cancelled', null, 'x'); };
    var lo = $('#lk-logout'); if (lo) lo.onclick = function () { localStorage.setItem('mandor:lark', 'loggedout'); render(); U.initShell(null); U.toast('Logged out of Lark', 'Board sync is paused', 'log-out'); };
    var rs = $('#replace-secret'); if (rs) rs.onclick = function () { lark.secretSaved = !lark.secretSaved; render(); if (!lark.secretSaved) $('#f-secret').focus(); };
    $('#lark-form').onsubmit = function (e) {
      e.preventDefault();
      var rules = [
        ['f-appid', function (v) { return !v ? 'App ID is required.' : !/^cli_[a-z0-9]{8,}$/.test(v) ? 'App ID should look like cli_a6f2c81d93b8d00e.' : ''; }],
        ['f-secret', function (v) { return !lark.secretSaved && !v ? 'Paste the App Secret, or cancel to keep the saved one.' : ''; }],
        ['f-token', function (v) { return !v ? 'Base app_token is required.' : v.length < 20 ? 'This looks too short for an app_token.' : ''; }],
        ['f-tasks', function (v) { return !v ? 'Sprint Tasks table ID is required.' : !/^tbl/.test(v) ? 'Table IDs start with tbl.' : ''; }],
        ['f-ac', function (v) { return !v ? 'Acceptance Criteria table ID is required.' : !/^tbl/.test(v) ? 'Table IDs start with tbl.' : ''; }],
        ['f-port', function (v) { return !/^\d{4,5}$/.test(v) || +v < 1024 || +v > 65535 ? 'Use a port between 1024 and 65535.' : ''; }]
      ];
      var bad = null;
      rules.forEach(function (r) {
        var el = $('#' + r[0]); var msg = r[1](el.value.trim()); var err = $('#' + r[0] + '-err');
        el.setAttribute('aria-invalid', msg ? 'true' : 'false'); err.hidden = !msg; err.innerHTML = msg ? ic('circle-alert', 'icon-sm') + esc(msg) : '';
        if (msg) { el.setAttribute('aria-describedby', r[0] + '-err'); if (!bad) bad = el; }
      });
      U.icons();
      if (bad) { bad.focus(); return; }
      lark = { appId: $('#f-appid').value.trim(), secretSaved: true, appToken: $('#f-token').value.trim(), tasksTable: $('#f-tasks').value.trim(), acTable: $('#f-ac').value.trim(), port: $('#f-port').value.trim() };
      localStorage.setItem('mandor:larkcfg', JSON.stringify(lark));
      render(); U.toast('Lark settings saved', 'The daemon reloaded the Base binding');
    };
    $('#lark-form').onreset = function () { setTimeout(render, 0); };
  }

  /* ---------------- Repos ---------------- */
  function listEditor(id, items, placeholder, mono) {
    return '<div class="list-editor" data-ui="ListEditor" id="' + id + '">' + items.map(function (v, i) {
      return '<div class="item"><span' + (mono ? '' : ' style="font-family:var(--font-sans)"') + '>' + esc(v) + '</span><button class="btn btn-ghost btn-xs btn-icon" data-rm="' + id + ':' + i + '" aria-label="Remove ' + esc(v) + '">' + ic('x', 'icon-sm') + '</button></div>';
    }).join('') + '<div class="input-group"><input class="input input-sm mono" id="' + id + '-in" placeholder="' + placeholder + '" aria-label="Add to ' + id + '"><button class="btn btn-outline btn-sm" data-add="' + id + '">' + ic('plus') + 'Add</button></div><p class="field-error" id="' + id + '-err" hidden></p></div>';
  }
  function reposSection() {
    var repos = M.getRepos();
    if (!repos.length) return '<section><h1>Repos</h1></section><section><div class="card"><div class="empty"><span class="ico">' + ic('folder-git-2') + '</span><h3>No repos yet</h3><p>Add a git repo on the daemon machine so agents have somewhere to work.</p><button class="btn btn-default" id="add-repo">' + ic('plus') + 'Add a repo</button></div></div></section>';
    var r = repos[selRepo] || repos[0];
    var names = function (ids) { return ids.map(function (id) { var p = Object.keys(M.people).map(function (k) { return M.people[k]; }).find(function (x) { return x.id === id; }); return p ? id + '  ·  ' + p.name : id; }); };
    return '<section style="display:flex;align-items:flex-end;gap:12px;max-width:none"><div style="flex:1"><h1>Repos</h1><p class="lede">Repos on the daemon machine that agents can work in.</p></div><button class="btn btn-secondary btn-sm" id="add-repo">' + ic('plus') + 'Add repo</button></section>' +
      '<section><div class="card" style="overflow:hidden"><table class="table" data-ui="Table"><thead><tr><th>Repo</th><th>Path</th><th>Default branch</th><th>gh profile</th></tr></thead><tbody>' +
        repos.map(function (x, i) { return '<tr data-repo="' + i + '" aria-selected="' + (i === selRepo) + '" tabindex="0"><td><b>' + esc(x.id) + '</b></td><td class="mono">' + esc(x.path) + '</td><td class="mono">' + esc(x.branch) + '</td><td>' + esc(x.gh) + '</td></tr>'; }).join('') +
      '</tbody></table></div></section>' +
      '<section><div class="card" data-ui="Card / RepoDetail"><div class="card-header"><div class="card-title">' + esc(r.id) + '</div><div class="card-description mono">' + esc(r.path) + ' · origin/' + esc(r.branch) + '</div></div><div class="card-content">' +
        '<div class="field" style="max-width:280px"><label class="label" for="r-gh">gh profile</label><select class="select" id="r-gh">' + ['default', 'pegasus-bot'].map(function (g) { return '<option' + (g === r.gh ? ' selected' : '') + '>' + g + '</option>'; }).join('') + '</select></div>' +
        '<div class="field"><span class="label">Read-only companion paths</span><p class="field-help" style="margin:-2px 0 8px">Extra folders the agent may read but never write, e.g. a shared design-token repo.</p>' + listEditor('companions', r.companions, '/home/beni/work/shared-config', true) + '</div>' +
        '<div class="field"><span class="label">Trusted reviewers and QA</span><p class="field-help" style="margin:-2px 0 8px">Lark open_ids whose record comments are forwarded to the agent automatically. Assignees and the reporter are always trusted.</p>' + listEditor('reviewers', names(r.reviewers), 'ou_51aa6c', true) + '</div>' +
        repoTestHtml(r) +
        '<div class="field"><span class="label">Extra agent allowlist</span><p class="field-help" style="margin:-2px 0 8px">Tool patterns the agent can run without asking. Anything not listed is denied, and <span class="mono">git push</span> is always run by the daemon.</p>' + listEditor('allowlist', r.allowlist, 'Bash(just *)', true) + '</div>' +
      '</div><div class="card-footer" style="justify-content:space-between"><button class="btn btn-ghost" id="rm-repo" style="color:var(--status-failed)">' + ic('trash-2') + 'Remove repo</button><button class="btn btn-default" id="save-repo">Save changes</button></div></div></section>';
  }
  function bindRepos() {
    var repos = M.getRepos();
    $$('[data-repo]').forEach(function (tr) { tr.onclick = function () { selRepo = +tr.dataset.repo; render(); }; tr.onkeydown = function (e) { if (e.key === 'Enter') tr.click(); }; });
    var add = $('#add-repo'); if (add) add.onclick = openAddRepo;
    if (!repos.length) return;
    var r = repos[selRepo];
    function listOf(id) { return id === 'envkeys' ? r.test.env : r[id]; }
    $$('[data-rm]').forEach(function (b) { b.onclick = function () { var p = b.dataset.rm.split(':'); listOf(p[0]).splice(+p[1], 1); M.setRepos(repos); render(); }; });
    bindSegs(function (name, v) { if (name === 'Test mode') { r.test = Object.assign({ env: [] }, r.test || {}, { mode: v }); M.setRepos(repos); render(); } if (name === 'Gate override') { r.gate = v; M.setRepos(repos); } });
    var tb = $('#t-browser'); if (tb) tb.onchange = function () { r.browser = tb.checked; M.setRepos(repos); };
    $$('[data-add]').forEach(function (b) {
      var id = b.dataset.add, inp = $('#' + id + '-in');
      inp.onkeydown = function (e) { if (e.key === 'Enter') { e.preventDefault(); b.click(); } };
      b.onclick = function () {
        var v = inp.value.trim(), err = $('#' + id + '-err'), msg = '';
        if (!v) msg = 'Type a value first.';
        else if (id === 'reviewers' && !/^ou_[a-z0-9]{6,}$/.test(v)) msg = 'Lark open_ids start with ou_, e.g. ou_51aa6c.';
        else if (id === 'companions' && v[0] !== '/') msg = 'Use an absolute path on the daemon machine.';
        else if (id === 'allowlist' && !/^[A-Z][A-Za-z]+(\(.+\))?$/.test(v)) msg = 'Use a tool pattern such as Bash(pnpm test *).';
        else if (id === 'envkeys' && v.indexOf('=') > -1) msg = 'Key names only. Values stay in your .env file.';
        else if (id === 'envkeys' && !/^[A-Z_][A-Z0-9_]*$/.test(v)) msg = 'Use the key name as written in .env, e.g. DATABASE_URL.';
        else if (listOf(id).indexOf(v) > -1) msg = 'Already in the list.';
        err.hidden = !msg; err.innerHTML = msg ? ic('circle-alert', 'icon-sm') + esc(msg) : ''; inp.setAttribute('aria-invalid', msg ? 'true' : 'false'); U.icons();
        if (msg) return;
        listOf(id).push(v); M.setRepos(repos); render(); $('#' + id + '-in').focus();
      };
    });
    $('#save-repo').onclick = function () { r.gh = $('#r-gh').value; M.setRepos(repos); U.toast('Repo saved', r.id + ' · used on the next agent start'); };
    $('#rm-repo').onclick = function () {
      $('#rr-title').textContent = 'Remove ' + r.id + '?';
      $('#rr-desc').textContent = 'Mandor forgets this repo and its settings. The folder at ' + r.path + ' is not deleted. Runs already working in it keep running.';
      $('#rr-go').onclick = function () { repos.splice(selRepo, 1); selRepo = 0; M.setRepos(repos); U.closeDialog('dlg-rmrepo'); render(); U.toast('Repo removed', r.id, 'trash-2'); };
      U.openDialog('dlg-rmrepo');
    };
  }
  function openAddRepo() {
    $('#ar-path').value = ''; $('#ar-path').removeAttribute('aria-invalid');
    $('#ar-msg').innerHTML = '<p class="field-help">Absolute path to a git clone with an <span class="mono">origin</span> remote.</p>';
    $('#ar-go').disabled = false; $('#ar-go').textContent = 'Add repo';
    $('#ar-go').onclick = function () {
      var v = $('#ar-path').value.trim(), msg = $('#ar-msg');
      function fail(html) { $('#ar-path').setAttribute('aria-invalid', 'true'); msg.innerHTML = '<div class="field-error" style="display:block">' + html + '</div>'; $('#ar-go').disabled = false; $('#ar-go').textContent = 'Add repo'; U.icons(); }
      if (!v || v[0] !== '/') return fail('Use an absolute path, e.g. /home/beni/work/pegasus-mobile.');
      $('#ar-go').disabled = true; $('#ar-go').innerHTML = ic('loader-circle', 'spin') + 'Checking'; U.icons();
      setTimeout(function () {
        if (/tmp|downloads/i.test(v)) return fail('Not a git repository: <span class="mono">' + esc(v) + '</span>');
        if (/fork|mirror/i.test(v)) return fail('<span class="mono">origin/HEAD</span> is missing. Fix it with:<pre class="mono" style="margin:6px 0 0;padding:6px 8px;border-radius:4px;background:var(--muted);color:var(--foreground)">git -C ' + esc(v) + ' remote set-head origin --auto</pre>');
        var id = v.split('/').filter(Boolean).pop();
        msg.innerHTML = '<p class="field-help" style="color:var(--status-ok);display:flex;gap:4px;align-items:center">' + ic('circle-check', 'icon-sm') + 'Git repo found. Default branch: <span class="mono">main</span></p>'; U.icons();
        var repos = M.getRepos(); repos.push({ id: id, path: v, branch: 'main', gh: $('#ar-gh').value, companions: [], reviewers: [], allowlist: [] }); M.setRepos(repos);
        setTimeout(function () { U.closeDialog('dlg-addrepo'); selRepo = repos.length - 1; location.hash = '#repos'; render(); U.toast('Repo added', id + ' · default branch main'); }, 700);
      }, 900);
    };
    U.openDialog('dlg-addrepo'); $('#ar-path').focus();
  }

  /* ---------------- Release 2: repo Test mode, gate override, browser testing ---------------- */
  function seg(name, opts, val) {
    return '<div class="toggle-group" role="radiogroup" aria-label="' + name + '" data-seg="' + name + '">' + opts.map(function (o) { return '<button type="button" role="radio" aria-checked="' + (o[0] === val) + '" aria-pressed="' + (o[0] === val) + '" data-v="' + o[0] + '">' + o[1] + '</button>'; }).join('') + '</div>';
  }
  function repoTestHtml(r) {
    var tst = r.test || { mode: 'none' }, ws = M.workspaces.filter(function (w) { return w.repos.indexOf(r.id) > -1; })[0];
    var web = tst.mode === 'web';
    return '<hr class="separator" style="margin:18px 0 4px"><div class="r2-head"><h3>AC review</h3><span class="muted">How the reviewer tests this repo.</span></div>' +
      '<div class="field"><span class="label">Test mode</span>' + seg('Test mode', [['web', 'Web'], ['none', 'No app']], tst.mode) +
        '<p class="field-help">' + (web ? 'The reviewer can run the app and check UI AC in Chrome.' : 'The reviewer checks code and tests only. Use this for APIs without a UI, libraries, and mobile apps.') + '</p></div>' +
      (web ? '<div class="grid-2" data-ui="RepoDetail / WebTest">' +
        '<div class="field"><label class="label" for="t-setup">Setup command <span class="muted" style="font-weight:400">(optional)</span></label><input class="input mono" id="t-setup" value="' + esc(tst.setup || '') + '"><p class="field-help">Runs when the lockfile changed.</p></div>' +
        '<div class="field"><label class="label" for="t-start">Start command</label><input class="input mono" id="t-start" value="' + esc(tst.start || '') + '"></div>' +
        '<div class="field"><label class="label" for="t-port">Port</label><input class="input mono" id="t-port" value="' + esc(tst.port || '') + '"></div>' +
        '<div class="field"><label class="label" for="t-url">App URL</label><input class="input mono" id="t-url" value="' + esc(tst.url || '') + '"><p class="field-help">One address per repo. Login cookies are tied to the host, so log in here in Chrome.</p></div></div>' +
        '<div class="field"><span class="label">.env keys passed to the app</span><p class="field-help" style="margin:-2px 0 8px">Key names only. Values stay in your .env and are never shown or sent to the agent. Use local or dev credentials.</p>' + listEditor('envkeys', tst.env || [], 'DATABASE_URL', true) + '</div>' : '') +
      '<div class="field"><span class="label">Gate override</span>' + seg('Gate override', [['workspace', 'Use workspace'], ['block', 'Block'], ['warn', 'Warn']], r.gate || 'workspace') +
        '<p class="field-help">' + (ws ? 'Workspace ' + esc(ws.id) + ' uses ' + (ws.gate === 'block' ? 'Block' : 'Warn') + '. Repo override wins; a multi-repo run uses the strictest member.' : 'Not in a workspace: Use workspace means the default, Block.') + '</p></div>' +
      '<label class="row-setting"><span class="txt">Browser testing<span>Used for single-repo runs. Multi-repo runs use the workspace setting' + (ws ? ' (' + esc(ws.id) + ': ' + (ws.browser ? 'on' : 'off') + ')' : '') + '. Page content is read by the AI model.</span></span><input type="checkbox" class="switch" role="switch" id="t-browser"' + (r.browser ? ' checked' : '') + (web ? '' : ' disabled') + '></label>';
  }
  function bindSegs(onChange) {
    $$('[data-seg]').forEach(function (g) { $$('button', g).forEach(function (b) { b.onclick = function () { $$('button', g).forEach(function (x) { x.setAttribute('aria-pressed', String(x === b)); x.setAttribute('aria-checked', String(x === b)); }); onChange(g.dataset.seg, b.dataset.v); }; }); });
  }

  /* ---------------- Workspaces ---------------- */
  function workspacesSection() {
    var w = M.workspaces[0];
    return '<section style="display:flex;align-items:flex-end;gap:12px;max-width:none"><div style="flex:1"><h1>Workspaces</h1><p class="lede">Repos grouped under one parent folder. A run on a workspace has one branch and one PR per changed repo.</p></div><button class="btn btn-secondary btn-sm">' + ic('plus') + 'Add workspace</button></section>' +
      '<section><div class="card" style="overflow:hidden"><table class="table"><thead><tr><th>Workspace</th><th>Folder</th><th>Repos</th><th>Gate</th></tr></thead><tbody><tr aria-selected="true"><td><b>' + esc(w.id) + '</b></td><td class="mono">' + esc(w.path) + '</td><td class="mono">' + esc(w.repos.join(', ')) + '</td><td>' + (w.gate === 'block' ? 'Block' : 'Warn') + '</td></tr></tbody></table></div></section>' +
      '<section><div class="card" data-ui="Card / WorkspaceDetail"><div class="card-header"><div class="card-title">' + esc(w.id) + '</div><div class="card-description mono">' + esc(w.path) + '</div></div><div class="card-content">' +
        '<div class="field"><span class="label">Repos</span><div class="list-editor">' + w.repos.map(function (r) { var rr = M.getRepos().filter(function (x) { return x.id === r; })[0] || {}; return '<div class="item">' + ic('folder-git-2', 'icon-sm') + '<span>' + esc(r) + '</span><span class="muted" style="flex:none;font-family:var(--font-sans)">' + ((rr.test || {}).mode === 'web' ? 'Web' : 'No app') + (rr.gate && rr.gate !== 'workspace' ? ' · gate override: ' + rr.gate : '') + '</span></div>'; }).join('') + '</div></div>' +
        '<label class="row-setting"><span class="txt">Browser testing<span>The reviewer opens the app in its own Chrome tab, only at each repo\'s app URL.</span></span><input type="checkbox" class="switch" role="switch" id="w-browser"' + (w.browser ? ' checked' : '') + '></label>' +
        '<div class="alert alert-warning" id="w-browser-note"' + (w.browser ? '' : ' hidden') + '>' + ic('eye') + '<div><div class="alert-title">Page content is read by the AI model</div><div class="muted">Whatever the app shows while logged in (names, amounts, records) is sent to the model during a review. Use a dev database.</div></div></div>' +
        '<div class="field" style="margin-top:14px"><span class="label">Gate</span>' + seg('Gate', [['block', 'Block'], ['warn', 'Warn']], w.gate) +
          '<p class="field-help" id="w-gate-help">' + gateHelp(w.gate) + '</p></div>' +
      '</div></div></section>' + precedenceNote();
  }
  function gateHelp(g) { return g === 'block' ? 'Failing AC lock Push &amp; create PR; you can push anyway with a written reason. AC the reviewer can\'t verify need your confirmation.' : 'Failing and unverified AC are listed as warnings; Push &amp; create PR stays enabled.'; }
  function precedenceNote() {
    return '<section><div class="precedence" data-ui="Settings / Precedence"><h2>' + ic('info', 'icon-sm') + 'Where each setting lives</h2><ul>' +
      '<li><b>Global</b> (this app): design sources, reviewer skill and model, cost limit.</li>' +
      '<li><b>Per repo</b>: Test mode, start command, app URL, .env keys.</li>' +
      '<li><b>Per workspace</b>: browser testing and the gate. A repo can override the gate: repo override › workspace › default (Block). A multi-repo run uses the strictest member.</li>' +
      '<li><b>Single-repo runs</b> use the repo\'s own browser testing setting.</li></ul></div></section>';
  }
  function bindWorkspaces() {
    var w = M.workspaces[0];
    $('#w-browser').onchange = function (e) { w.browser = e.target.checked; $('#w-browser-note').hidden = !w.browser; U.toast('Browser testing ' + (w.browser ? 'on' : 'off') + ' for ' + w.id, w.browser ? null : 'UI AC will be Can\'t verify in reviews', 'globe'); };
    bindSegs(function (name, v) { if (name === 'Gate') { w.gate = v; $('#w-gate-help').innerHTML = gateHelp(v); U.toast('Gate for ' + w.id + ': ' + (v === 'block' ? 'Block' : 'Warn')); } });
  }

  /* ---------------- Design sources ---------------- */
  var dsEdit = null;
  function designSection() {
    var list = M.getSources(), A = U.ac;
    var head = '<section style="display:flex;align-items:flex-end;gap:12px;max-width:none"><div style="flex:1"><h1>Design sources</h1><p class="lede">Where planning looks for the design. Each run picks one or more in the Start dialog. The agent never receives design tokens.</p></div><button class="btn btn-secondary btn-sm" id="ds-add">' + ic('plus') + 'Add design source</button></section>';
    var body = !list.length ? '<section><div class="card"><div class="empty"><span class="ico">' + ic('frame') + '</span><h3>No design sources yet</h3><p>Add the designer\'s prototype repo or folder. Planning then marks which AC the design is missing.</p><button class="btn btn-default" id="ds-add2">' + ic('plus') + 'Add design source</button></div></div></section>'
      : '<section><div class="ds-list" data-ui="DesignSourceList">' + list.map(function (src, i) { return '<div data-ds="' + i + '">' + A.sourceItem(src) + '</div>'; }).join('') + '</div></section>';
    return head + (dsEdit ? dsForm(dsEdit) : '') + body;
  }
  function dsForm(e) {
    var kinds = U.ac.SOURCE_KINDS;
    var k = e.kind;
    var fields = k === 'repo' ? '<div class="field"><label class="label" for="ds-path">Path on the daemon machine</label><div class="input-group"><input class="input mono" id="ds-path" placeholder="/home/beni/work/kelola-design" value="' + esc(e.location || '') + '" aria-describedby="ds-err"><button class="btn btn-outline" type="button" id="ds-browse">' + ic('folder-open') + 'Browse</button></div></div>'
      : k === 'figma' ? '<div class="field"><label class="label" for="ds-link">Figma file link</label><input class="input mono" id="ds-link" placeholder="https://www.figma.com/design/…" value="' + esc(e.location || '') + '"></div>' +
        '<div class="field"><label class="label" for="ds-token">Figma token</label>' + (e.tokenSaved ? '<div class="input-group"><input class="input mono" value="Saved token" disabled aria-label="Figma token saved, hidden"><button class="btn btn-outline" type="button" id="ds-replace">Replace</button></div>' : '<input class="input mono" id="ds-token" type="password" autocomplete="off" placeholder="figd_…">') + '<p class="field-help">Use a read-only token. It is stored on the daemon machine, never shown again, and never given to the agent.</p></div>'
      : k === 'url' ? '<div class="field"><label class="label" for="ds-url">Deployed prototype URL</label><input class="input mono" id="ds-url" placeholder="https://proto.example.com" value="' + esc(e.location || '') + '" aria-describedby="ds-host"><p class="field-help" id="ds-host">https only. Host: <b class="mono" id="ds-hostname">' + esc(hostOf(e.location)) + '</b></p></div>'
      : '<div class="field"><label class="label" for="ds-path">Folder of images or PDFs</label><div class="input-group"><input class="input mono" id="ds-path" placeholder="/home/beni/designs/po-flow" value="' + esc(e.location || '') + '"><button class="btn btn-outline" type="button">' + ic('folder-open') + 'Browse</button></div></div>';
    return '<section><div class="card ds-form" data-ui="Card / DesignSourceForm"' + (kinds[k].release === '2.1' ? ' data-release="2.1"' : '') + '><div class="card-header"><div class="card-title">' + (e.index != null ? 'Edit ' + esc(e.name) : 'Add design source') + '</div></div><div class="card-content">' +
      '<div class="field"><span class="label">Kind</span><div class="radio-row ds-kinds" role="radiogroup" aria-label="Kind">' + Object.keys(kinds).map(function (kk) { return '<label class="radio-card"><input type="radio" name="ds-kind" value="' + kk + '"' + (kk === k ? ' checked' : '') + '>' + ic(kinds[kk].icon) + kinds[kk].label + (kinds[kk].release === '2.1' ? '<span class="rel-tag">2.1</span>' : '') + '</label>'; }).join('') + '</div></div>' +
      '<div class="field"><label class="label" for="ds-name">Name</label><input class="input" id="ds-name" value="' + esc(e.name || '') + '" placeholder="kelola-design"></div>' + fields +
      '<div id="ds-msg" aria-live="polite">' + (e.msg || '') + '</div>' +
      '</div><div class="card-footer"><button class="btn btn-ghost" id="ds-cancel">Cancel</button><button class="btn btn-outline" id="ds-test">' + ic('plug-zap') + 'Test</button><button class="btn btn-default" id="ds-save">Save</button></div></div></section>';
  }
  function hostOf(u) { try { return new URL(u).host; } catch (e) { return 'not set'; } }
  function bindDesign() {
    var list = M.getSources();
    function open(e) { dsEdit = e; render(); }
    [$('#ds-add'), $('#ds-add2')].forEach(function (b) { if (b) b.onclick = function () { open({ kind: 'repo' }); }; });
    $$('[data-ds]').forEach(function (row) {
      var i = +row.dataset.ds, src = list[i];
      $$('.si-act button', row).forEach(function (b, j) { b.onclick = function () {
        if (j === 0) { b.disabled = true; b.textContent = 'Testing…'; setTimeout(function () { U.toast(src.status === 'error' ? 'Test failed: ' + src.name : src.name + ' is reachable', src.status === 'error' ? src.error : src.location, src.status === 'error' ? 'circle-x' : 'circle-check'); render(); }, 900); }
        else open(Object.assign({ index: i, tokenSaved: src.kind === 'figma' }, src));
      }; });
    });
    if (!dsEdit) return;
    $$('input[name="ds-kind"]').forEach(function (r) { r.onchange = function () { dsEdit = { kind: r.value, name: $('#ds-name').value, index: dsEdit.index }; render(); }; });
    var url = $('#ds-url'); if (url) url.oninput = function () { $('#ds-hostname').textContent = hostOf(url.value); };
    var rp = $('#ds-replace'); if (rp) rp.onclick = function () { dsEdit.tokenSaved = false; render(); $('#ds-token').focus(); };
    var br = $('#ds-browse'); if (br) br.onclick = function () { $('#ds-path').value = '/home/beni/work/kelola-design'; if (!$('#ds-name').value) $('#ds-name').value = 'kelola-design'; };
    $('#ds-cancel').onclick = function () { dsEdit = null; render(); };
    function check() {
      var k = dsEdit.kind, err = '';
      var loc = ($('#ds-path') || $('#ds-link') || $('#ds-url')).value.trim();
      if (!$('#ds-name').value.trim()) err = 'Give the source a name.';
      else if (!loc) err = 'Fill in the location.';
      else if (k === 'url' && !/^https:\/\//.test(loc)) err = 'Use an https:// address.';
      else if ((k === 'repo' || k === 'images') && /missing|^\/tmp\/nope/.test(loc)) err = 'Path missing: ' + loc + ' does not exist on the daemon machine.';
      else if (k === 'figma' && !dsEdit.tokenSaved && !($('#ds-token') || {}).value) err = 'Paste a Figma token.';
      else if (k === 'figma' && /^figd_bad/.test(($('#ds-token') || {}).value || '')) err = 'Token rejected by Figma. Check that it is valid and has read access to the file.';
      return { err: err, loc: loc };
    }
    function msg(html, ok) { $('#ds-msg').innerHTML = '<p class="' + (ok ? 'field-help' : 'field-error') + '" style="display:flex;gap:6px;align-items:center">' + ic(ok ? 'circle-check' : 'circle-alert', 'icon-sm') + html + '</p>'; U.icons(); }
    $('#ds-test').onclick = function () { var c = check(); if (c.err) return msg(esc(c.err)); msg('Reachable. ' + (dsEdit.kind === 'repo' ? 'Found a git repo with 42 Svelte routes.' : 'Ready to use.'), true); };
    $('#ds-save').onclick = function () {
      var c = check(); if (c.err) return msg(esc(c.err));
      var item = { id: dsEdit.id || $('#ds-name').value.trim().toLowerCase().replace(/\s+/g, '-'), kind: dsEdit.kind, name: $('#ds-name').value.trim(), location: c.loc, status: 'ok' };
      if (dsEdit.index != null) list[dsEdit.index] = item; else list.push(item);
      M.setSources(list); dsEdit = null; render(); U.toast('Design source saved', item.name, 'frame');
    };
  }

  /* ---------------- General ---------------- */
  function generalSection() {
    var theme = localStorage.getItem('mandor:theme') || 'dark';
    var notif = JSON.parse(localStorage.getItem('mandor:notif') || '{"waiting":true,"failed":true,"fix":true,"pr":false,"attention":true,"review":true,"app":true}');
    var kinds = [['waiting', 'Agent finished and is waiting for you'], ['failed', 'Run failed'], ['fix', 'Reviewer requested fixes (Waiting Fix)'], ['pr', 'PR created'], ['attention', 'Needs attention: claim lost, sensitive changes held, automatic fix push held'], ['review', 'AC review finished (with counts)'], ['app', 'Local app failed to start']];
    return '<section><h1>General</h1></section>' +
      '<section><h2>Theme</h2><div class="radio-row" role="radiogroup" aria-label="Theme" style="margin-top:10px">' +
        [['system', 'monitor', 'System'], ['dark', 'moon', 'Dark'], ['light', 'sun', 'Light']].map(function (o) { return '<label class="radio-card"><input type="radio" name="theme" value="' + o[0] + '"' + (theme === o[0] ? ' checked' : '') + '>' + ic(o[1]) + o[2] + '</label>'; }).join('') +
      '</div></section>' +
      '<section><h2>Desktop notifications</h2><div style="margin-top:4px">' + kinds.map(function (k) { return '<label class="row-setting"><span class="txt">' + k[1] + '</span><input type="checkbox" class="switch" role="switch" data-n="' + k[0] + '"' + (notif[k[0]] ? ' checked' : '') + ' data-ui="Switch"></label>'; }).join('') + '</div></section>' +
      '<section><h2>Waiting Fix</h2><div class="row-setting"><span class="txt"><label for="g-quiet">Quiet period</label><span>Seconds to wait after a trusted comment before forwarding, so a reviewer can finish writing.</span></span><div class="input-group" style="width:150px"><input class="input mono" id="g-quiet" type="number" min="30" max="900" value="' + (localStorage.getItem('mandor:quiet') || 120) + '"><span class="muted" style="align-self:center">sec</span></div></div></section>' +
      '<section data-ui="Settings / Reviewer"><h2>AC review</h2><div style="margin-top:4px">' +
        '<div class="row-setting"><span class="txt"><label for="g-rmodel">Reviewer model</label><span>The reviewer runs in a separate, read-only Claude Code session.</span></span><select class="select" id="g-rmodel" style="width:260px"><option selected>Same as the implementing agent</option><option>Claude Opus</option><option>Claude Sonnet</option></select></div>' +
        '<div class="row-setting"><span class="txt"><label for="g-rcost">Cost limit per review</label><span>The review stops when it reaches this estimate; unreported AC stay Not reviewed. Default 2 USD; leave empty for no limit.</span></span><div class="input-group" style="width:150px"><span class="muted" style="align-self:center">USD</span><input class="input mono" id="g-rcost" type="number" min="0" step="0.5" value="2" placeholder="No limit"></div></div>' +
        '<div class="row-setting"><span class="txt"><label for="g-rtime">Time limit per review</label><span>Same rule as the cost limit.</span></span><div class="input-group" style="width:150px"><input class="input mono" id="g-rtime" type="number" min="5" max="60" value="15"><span class="muted" style="align-self:center">min</span></div></div>' +
        '<p class="field-help">Reviewer skill (Mandor built-in or a plugin skill) is chosen in the Start dialog and remembered.</p></div></section>' +
      '<section><h2>Window</h2><label class="row-setting"><span class="txt">Keep running in the tray when closed<span>Agents keep working; Mandor reopens from the tray icon.</span></span><input type="checkbox" class="switch" role="switch" id="g-tray" checked></label></section>';
  }
  function bindGeneral() {
    $$('input[name="theme"]').forEach(function (r) { r.onchange = function () { localStorage.setItem('mandor:theme', r.value); U.applyTheme(); U.toast('Theme: ' + r.value, null, r.value === 'light' ? 'sun' : r.value === 'dark' ? 'moon' : 'monitor'); }; });
    $$('[data-n]').forEach(function (s) { s.onchange = function () { var n = JSON.parse(localStorage.getItem('mandor:notif') || '{"waiting":true,"failed":true,"fix":true,"pr":false,"attention":true,"review":true,"app":true}'); n[s.dataset.n] = s.checked; localStorage.setItem('mandor:notif', JSON.stringify(n)); }; });
    $('#g-quiet').onchange = function (e) { var v = Math.max(30, Math.min(900, +e.target.value || 120)); e.target.value = v; localStorage.setItem('mandor:quiet', v); U.toast('Quiet period set to ' + v + ' seconds'); };
  }

  /* ---------------- About ---------------- */
  function aboutSection() {
    return '<section><h1>About</h1></section><section><div class="card"><div class="card-content"><dl class="versions">' +
      '<dt class="muted">Mandor app</dt><dd class="mono" style="margin:0">' + M.version.app + '</dd>' +
      '<dt class="muted">mandord daemon</dt><dd class="mono" style="margin:0">' + M.version.daemon + ' · WSL Ubuntu-24.04</dd>' +
      '<dt class="muted">Data folder</dt><dd class="mono" style="margin:0">~/.local/share/mandor (on the daemon machine)</dd>' +
      '<dt class="muted">Claude Code</dt><dd class="mono" style="margin:0">2.0.14 · logged in (subscription)</dd>' +
      '</dl>' + systemCheck() + '</div><div class="card-footer" style="justify-content:flex-start"><button class="btn btn-outline btn-sm" onclick="UI.toast(\'Opened logs folder\', \'~/.local/share/mandor/logs\', \'folder-open\')">' + ic('folder-open') + 'Open logs</button><button class="btn btn-ghost btn-sm" id="reset-demo">' + ic('rotate-ccw') + 'Clear local cache</button></div></div></section>';
  }

  function systemCheck() {
    var noext = U.params.get('ext') === 'off';
    var items = [
      ['ok', 'Daemon reachable', 'mandord 0.4.2'],
      ['ok', 'Claude Code logged in', 'subscription'],
      ['ok', 'GitHub CLI authenticated', 'gh 2.62.0 · 2 profiles'],
      noext ? ['missing', 'Claude in Chrome extension not connected', 'Browser reviews mark UI AC as Can\'t verify until it connects.'] : ['ok', 'Claude in Chrome extension connected', 'Chrome 141 · used for browser reviews']
    ];
    return '<hr class="separator" style="margin:14px 0"><h2 style="font-size:var(--text-base);font-weight:600;margin-bottom:8px">System check</h2><ul class="ready-list" data-ui="Settings / SystemCheck">' +
      items.map(function (it) { return U.ac.readyItem({ state: it[0], title: esc(it[1]), help: esc(it[2]), action: it[0] === 'missing' ? '<button class="btn btn-outline btn-xs">' + ic('refresh-cw') + 'Check again</button>' : '' }); }).join('') + '</ul>';
  }

  function render() {
    nav();
    var s = section();
    main.innerHTML = s === 'repos' ? reposSection() : s === 'general' ? generalSection() : s === 'about' ? aboutSection() : s === 'workspaces' ? workspacesSection() : s === 'design' ? designSection() : larkSection();
    if (s === 'workspaces') bindWorkspaces(); else if (s === 'design') bindDesign(); else if (s === 'repos') bindRepos(); else if (s === 'general') bindGeneral(); else if (s === 'about') { $('#reset-demo').onclick = function () { M.reset(); ['mandor:lark', 'mandor:larkcfg', 'mandor:dismiss:restart'].forEach(function (k) { localStorage.removeItem(k); }); U.toast('Local cache cleared', 'Reload to fetch everything from the daemon again', 'rotate-ccw'); }; } else bindLark();
    U.icons();
  }
  /* Loading (Skeleton) and error states while the daemon returns settings */
  function renderLoading() {
    nav();
    var bar = function (w, h) { return '<div class="skeleton" style="height:' + (h || 12) + 'px;width:' + w + '"></div>'; };
    main.setAttribute('aria-busy', 'true');
    main.innerHTML = '<span class="sr-only">Loading settings from the daemon…</span><section style="display:grid;gap:10px">' + bar('140px', 22) + bar('60%') + '</section>' +
      '<section><div class="card"><div class="card-content" style="display:grid;gap:18px"><div class="grid-2">' +
      [1, 2, 3, 4].map(function () { return '<div style="display:grid;gap:8px">' + bar('35%', 10) + bar('100%', 32) + '</div>'; }).join('') +
      '</div></div></div></section>';
  }
  function renderError() {
    nav();
    main.removeAttribute('aria-busy');
    main.innerHTML = '<section><div class="card"><div class="empty">' + '<span class="ico" style="color:var(--status-failed)">' + ic('server-crash') + '</span>' +
      '<h3>Couldn\'t load settings from the daemon</h3><p>mandord did not answer within 5 seconds. Your settings are safe; nothing was changed.</p>' +
      '<div style="display:flex;gap:8px"><button class="btn btn-secondary" id="st-retry">' + ic('refresh-cw') + 'Try again</button><a class="btn btn-ghost" href="connection.html?state=unreachable">Check the connection</a></div></div></div></section>';
    $('#st-retry').onclick = function () { failOnce = false; load(); };
    U.icons();
  }
  var failOnce = U.params.get('settings') === 'error';
  function load() {
    renderLoading();
    setTimeout(function () { main.removeAttribute('aria-busy'); if (failOnce) renderError(); else render(); }, U.params.get('loading') === 'hold' ? 600000 : 400);
  }
  load();
})();
