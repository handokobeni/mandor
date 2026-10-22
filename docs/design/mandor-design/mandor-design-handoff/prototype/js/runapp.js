/* Release 2: Run app (section 4.4). Action bar control, confirmation dialog,
   start sequence per repo, logs drawer, and the takeover confirmation when
   another run owns the app. App state is global (MANDOR.getApp/setApp).
   .env values are never shown, only key names. */
(function () {
  var M = window.MANDOR, U = window.UI, esc = U.esc, ic = U.ic, $ = U.$, $$ = U.$$;

  /* Fixtures: per-repo start config from Settings › Repos (Test mode = Web) */
  var REPOS = {
    'kelola-api': { setup: 'cargo build', start: 'cargo run --bin kelola-api', port: 8080, url: 'http://localhost:8080', env: ['DATABASE_URL', 'JWT_SECRET', 'LARK_APP_ID'], envOn: ['DATABASE_URL', 'JWT_SECRET'] },
    'kelola-web': { setup: 'bun install', start: 'bun run dev', port: 5173, url: 'http://localhost:5173', env: ['PUBLIC_API_URL', 'SENTRY_DSN'], envOn: ['PUBLIC_API_URL'] }
  };
  var FLAG = /(^|\/)(package\.json|bun\.lockb?|Cargo\.(toml|lock)|justfile|Makefile|vite\.config\.\w+|svelte\.config\.\w+|scripts\/)/i;
  var LOGS = {
    'kelola-api': ['$ cargo run --bin kelola-api', '   Compiling kelola-api v0.9.0 (/home/beni/.local/share/mandor/workspaces/kelola-v2-pgs-1426/kelola-api)', '    Finished `dev` profile in 41.2s', '     Running `target/debug/kelola-api`', 'INFO kelola_api: connected to database', 'INFO kelola_api: listening on 127.0.0.1:8080'],
    'kelola-web': ['$ bun install', 'bun install v1.2.4', ' + svelte-sonner@0.3.28', ' 412 packages installed [2.31s]', '$ bun run dev', '  VITE v6.0.7  ready in 812 ms', '  ➜  Local:   http://localhost:5173/']
  };
  var FAIL_LOG = ['$ bun run dev', 'error: Cannot find module "./lib/po/RejectDialog.svelte" from "src/routes/po/[id]/+page.svelte"', 'error: script "dev" exited with code 1'];

  function mount(host) {
    /* host: { key, repos, diff(), openFile(repo, path), onChange() } */
    var P = U.params, timers = [], logs = { 'kelola-api': [], 'kelola-web': [] }, logRepo = host.repos[0];
    var drawerOpen = P.get('logs') === 'open';
    if (P.get('app') === 'failed') { logs['kelola-api'] = LOGS['kelola-api'].slice(); logs['kelola-web'] = FAIL_LOG.slice(); logRepo = 'kelola-web'; }
    if (P.get('app') === 'running' || P.get('app') === 'older') host.repos.forEach(function (r) { logs[r] = LOGS[r].slice(); });

    function app() { return M.getApp(); }
    function mine() { var a = app(); return a.owner === host.key; }
    function state() {
      var a = app();
      if (!mine()) { if (a.prevOwner === host.key) return 'stopped'; return a.owner && a.state !== 'off' ? 'other' : 'off'; }
      return a.state || 'off';
    }
    function set(st, extra) { var a = app(); M.setApp(Object.assign({ owner: host.key, state: st, prevOwner: a.prevOwner }, extra || {})); host.onChange(); renderDrawer(); }

    var LABEL = {
      off: ['muted', 'circle-pause', 'App off'],
      setup: ['working', 'loader-circle', 'Setting up · bun install'],
      starting_api: ['working', 'loader-circle', 'Starting kelola-api (1 of 2)'],
      starting_web: ['working', 'loader-circle', 'Starting kelola-web (2 of 2)'],
      starting: ['working', 'loader-circle', 'Starting kelola-api (1 of 2)'],
      port: ['working', 'loader-circle', 'Waiting for port 5173'],
      running: ['ok', 'circle-check', 'App running'],
      older: ['warning', 'history', 'Running older code'],
      failed: ['failed', 'circle-x', 'Failed to start · kelola-web'],
      stopped: ['muted', 'circle-pause', 'Stopped: PGS-1391 started its app'],
      other: ['muted', 'app-window', 'PGS-1391 owns the app']
    };
    function html() {
      var st = state(), l = (LABEL[st] || LABEL.off).slice(), busy = /setup|starting|port/.test(st), other = app().owner !== host.key ? app().owner : 'PGS-1391';
      if (st === 'stopped') l[2] = 'Stopped: ' + other + ' started its app';
      if (st === 'other') l[2] = other + ' owns the app';
      var btn = st === 'running' ? '<button class="btn btn-ghost btn-sm" id="app-stop">' + ic('square') + 'Stop app</button>'
        : st === 'older' ? '<button class="btn btn-outline btn-sm" id="app-restart">' + ic('rotate-ccw') + 'Restart app</button>'
        : st === 'failed' ? '<button class="btn btn-outline btn-sm" id="app-logs">' + ic('scroll-text') + 'View logs</button><button class="btn btn-ghost btn-sm" id="app-run">' + ic('rotate-ccw') + 'Try again</button>'
        : busy ? '<button class="btn btn-ghost btn-sm" id="app-logs">' + ic('scroll-text') + 'Logs</button><button class="btn btn-ghost btn-sm" id="app-stop">' + ic('square') + 'Stop</button>'
        : '<button class="btn btn-outline btn-sm" id="app-run">' + ic('play') + 'Run app</button>';
      return '<span class="run-app" data-ui="RunAppControl / state=' + st + '">' +
        '<span class="run" data-tone="' + l[0] + '">' + ic(l[1], busy ? 'spin' : '') + '<span class="lbl">' + l[2] + '</span></span>' +
        (st === 'running' || st === 'older' ? '<a class="link mono" href="#" onclick="return false" title="Open in Chrome. Log in there with your own account.">' + esc(REPOS['kelola-web'].url) + '</a>' : '') +
        btn + (st === 'running' || st === 'older' ? '<button class="btn btn-ghost btn-sm btn-icon" id="app-logs" aria-label="App logs" title="Logs">' + ic('scroll-text') + '</button>' : '') + '</span>';
    }
    function bind() {
      var b;
      if ((b = $('#app-run'))) b.onclick = function () { requestStart(); };
      if ((b = $('#app-restart'))) b.onclick = function () { requestStart({ restart: true }); };
      if ((b = $('#app-stop'))) b.onclick = stop;
      if ((b = $('#app-logs'))) b.onclick = function () { drawerOpen = !drawerOpen; renderDrawer(); };
    }

    /* ---------- dialogs ---------- */
    var dlg = document.createElement('dialog');
    dlg.className = 'dialog'; dlg.id = 'dlg-app'; dlg.style.width = '600px'; dlg.setAttribute('aria-labelledby', 'ap-title');
    document.body.appendChild(dlg);

    var after = null;
    function requestStart(opts) {
      opts = opts || {};
      after = opts.after || null;
      var a = app();
      if (a.owner && a.owner !== host.key && a.state !== 'off' && a.state !== 'stopped') return confirmTakeover(a, opts);
      confirmStart(opts);
    }
    function confirmTakeover(a, opts) {
      var other = M.getTask(a.owner);
      dlg.setAttribute('data-ui', 'AlertDialog / AppTakeover');
      dlg.innerHTML = '<div class="dialog-header"><h2 class="dialog-title" id="ap-title">Stop the app for ' + esc(a.owner) + '?</h2>' +
        '<p class="dialog-description">Only one run\'s app runs at a time. Starting the app for ' + esc(host.key) + ' stops the one for <b>' + esc(a.owner) + '</b>' + (other ? ' (' + esc(other.title) + ')' : '') + '. Its chat and worktree are not touched.</p></div>' +
        '<div class="dialog-footer"><button class="btn btn-ghost" data-close>Cancel</button><button class="btn btn-default" id="ap-go">Stop ' + esc(a.owner) + '\'s app and continue</button></div>';
      U.icons(); if (!dlg.open) dlg.showModal();
      $('#ap-go', dlg).onclick = function () { dlg.close(); M.setApp({ owner: host.key, state: 'off', prevOwner: a.owner }); confirmStart(opts); };
    }
    function confirmStart(opts) {
      var diff = host.diff(), first = !P.get('applast');
      var groups = host.repos.map(function (r) {
        var files = diff.filter(function (f) { return f.repo === r; });
        var cfg = REPOS[r];
        return '<section class="app-repo"><div class="app-repo-h">' + ic('folder-git-2', 'icon-sm') + '<b class="mono">' + esc(r) + '</b><span class="muted mono">' + esc(cfg.start) + ' · ' + esc(cfg.url) + '</span></div>' +
          '<div class="app-files">' + (files.length ? files.map(function (f) {
            var flag = FLAG.test(f.path);
            return '<button class="app-file" data-open="' + esc(r + '|' + f.path) + '"' + (flag ? ' data-flag="true"' : '') + '><span class="fs" data-s="' + f.s + '">' + f.s + '</span><span class="fp">' + esc(f.path) + '</span>' +
              (flag ? '<span class="chip" data-tone="warning">' + ic('triangle-alert') + 'Runs at start</span>' : '') + '</button>';
          }).join('') : '<p class="muted">No changes in this repo.</p>') + '</div>' +
          '<div class="app-env"><span class="label">.env keys passed to ' + esc(r) + '</span><div class="app-env-keys">' + cfg.env.map(function (k) {
            return '<label class="env-key"><input type="checkbox" class="checkbox"' + (cfg.envOn.indexOf(k) > -1 ? ' checked' : '') + '><span class="mono">' + esc(k) + '</span></label>';
          }).join('') + '</div></div></section>';
      }).join('');
      dlg.setAttribute('data-ui', 'Dialog / RunAppConfirm');
      dlg.innerHTML = '<div class="dialog-header"><span class="mono muted">' + esc(host.key) + ' · ' + esc(host.repos.join(' + ')) + '</span><h2 class="dialog-title" id="ap-title" style="margin-top:6px">' + (opts.restart ? 'Restart the app with the latest code?' : 'Run the app for ' + esc(host.key) + '?') + '</h2>' +
        '<p class="dialog-description">' + (first ? 'Files changed since the base branch' : 'Files changed since the app last ran') + '. Files that run at start (manifests, scripts, config) are flagged. Click a file to review it in Changes.</p></div>' +
        '<div class="dialog-body">' + groups +
          '<div class="alert alert-warning" style="margin-top:12px">' + ic('shield-alert') + '<div><div class="alert-title">The app runs code written by the agent</div><div class="muted">It starts on your machine with the .env keys checked above. Values stay in your .env files and are never shown or sent to the agent. Use local or dev credentials only.</div></div></div>' +
          '<p class="field-help" style="margin-top:10px">' + ic('database', 'icon-sm') + ' Start the database yourself first. Mandor does not start databases.</p>' +
        '</div>' +
        '<div class="dialog-footer"><span class="left">Keys are chosen per repo in Settings › Repos.</span><button class="btn btn-ghost" data-close>Cancel</button><button class="btn btn-default" id="ap-go">' + ic(opts.restart ? 'rotate-ccw' : 'play') + (opts.restart ? 'Restart app' : 'Run app') + '</button></div>';
      U.icons(); if (!dlg.open) dlg.showModal();
      $$('[data-open]', dlg).forEach(function (b) { b.onclick = function () { var p = b.dataset.open.split('|'); dlg.close(); host.openFile(p[0], p[1]); }; });
      $('#ap-go', dlg).onclick = function () { dlg.close(); start(); };
    }

    /* ---------- start sequence ---------- */
    function later(fn, ms) { timers.push(setTimeout(fn, ms)); }
    function stream(repo, lines, from, step) { lines.forEach(function (ln, i) { later(function () { logs[repo].push(ln); renderDrawer(); }, from + i * step); }); }
    function start() {
      timers.forEach(clearTimeout); timers = [];
      logs = { 'kelola-api': [], 'kelola-web': [] };
      set('setup');
      stream('kelola-web', LOGS['kelola-web'].slice(0, 4), 100, 200);
      later(function () { set('starting_api'); logRepo = 'kelola-api'; }, 1000);
      stream('kelola-api', LOGS['kelola-api'], 1100, 220);
      later(function () { set('starting_web'); logRepo = 'kelola-web'; }, 2500);
      stream('kelola-web', LOGS['kelola-web'].slice(4), 2600, 250);
      later(function () { set('port'); }, 3200);
      later(function () {
        if (P.get('appfail') === '1') { logs['kelola-web'] = FAIL_LOG.slice(); drawerOpen = true; set('failed'); U.toast('App failed to start', 'kelola-web exited with code 1. See the logs.', 'circle-x'); return; }
        set('running', { startedAt: new Date().toTimeString().slice(0, 5) });
        U.toast('App running', REPOS['kelola-web'].url + ' · log in there with your own account', 'app-window');
        if (after) { var f = after; after = null; f(); }
      }, 4000);
    }
    function stop() { timers.forEach(clearTimeout); timers = []; set('off'); U.toast('App stopped', null, 'square'); }
    /* Called when the agent changes code while the app runs */
    function codeChanged() { if (state() === 'running') set('older'); }

    /* ---------- logs drawer ---------- */
    var drawer = document.createElement('section');
    drawer.className = 'logs-drawer'; drawer.setAttribute('aria-label', 'App logs'); drawer.setAttribute('data-ui', 'Sheet / side=bottom / AppLogs'); drawer.hidden = true;
    (host.mountIn || document.body).appendChild(drawer);
    function renderDrawer() {
      drawer.hidden = !drawerOpen;
      if (!drawerOpen) return;
      var st = state();
      drawer.innerHTML = '<div class="ld-head"><b>' + ic('scroll-text', 'icon-sm') + 'App logs</b>' +
        '<div class="toggle-group" role="tablist" aria-label="Repo">' + host.repos.map(function (r) { return '<button role="tab" aria-pressed="' + (r === logRepo) + '" aria-selected="' + (r === logRepo) + '" data-lr="' + r + '">' + esc(r) + (st === 'failed' && r === 'kelola-web' ? ' ' + ic('circle-x', 'icon-sm') : '') + '</button>'; }).join('') + '</div>' +
        '<span class="muted ld-meta">' + esc(LABEL[st] ? LABEL[st][2] : '') + '</span>' +
        '<button class="btn btn-ghost btn-xs" id="ld-copy">' + ic('copy') + 'Copy</button><button class="btn btn-ghost btn-xs btn-icon" id="ld-close" aria-label="Close logs">' + ic('x') + '</button></div>' +
        '<pre class="ld-body" tabindex="0" aria-live="polite">' + (logs[logRepo].length ? esc(logs[logRepo].join('\n')) : '<span class="muted">No output yet.</span>') + '</pre>';
      U.icons();
      $$('[data-lr]', drawer).forEach(function (b) { b.onclick = function () { logRepo = b.dataset.lr; renderDrawer(); }; });
      $('#ld-close', drawer).onclick = function () { drawerOpen = false; renderDrawer(); };
      $('#ld-copy', drawer).onclick = function () { U.toast('Copied ' + logRepo + ' logs', null, 'copy'); };
      var pre = $('.ld-body', drawer); pre.scrollTop = pre.scrollHeight;
    }
    renderDrawer();

    return { html: html, bind: bind, state: state, requestStart: requestStart, stop: stop, codeChanged: codeChanged, url: REPOS['kelola-web'].url, openLogs: function () { drawerOpen = true; renderDrawer(); } };
  }

  window.RUNAPP = { mount: mount, REPOS: REPOS };
})();
