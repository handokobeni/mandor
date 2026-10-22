/* Release 2: AC review. Pure gate logic (lib/ac-review.ts in the app),
   shared renderers (UI.ac, used by the components sheet), and the AC review
   tab controller (ACTAB.mount, used by the workspace). Evidence is always
   escaped plain text; file:line links open the Changes tab. */
(function () {
  var U = window.UI, esc = U.esc, ic = U.ic;

  /* ---------------- vocabulary ---------------- */
  var DESIGN = {
    not_checked: { label: 'Not checked yet', icon: 'circle-dashed', tone: 'muted' },
    in:          { label: 'In design', icon: 'circle-check', tone: 'ok' },
    partly:      { label: 'Partly in design', icon: 'contrast', tone: 'waiting' },
    missing:     { label: 'Missing from design', icon: 'circle-slash', tone: 'warning' },
    conflicts:   { label: 'Conflicts with design', icon: 'arrow-right-left', tone: 'failed' },
    follow:      { label: 'Follow design', icon: 'signpost-big', tone: 'pr' }
  };
  var VERDICT = {
    not_reviewed: { label: 'Not reviewed', icon: 'circle-dashed', tone: 'muted' },
    pass:         { label: 'Pass', icon: 'circle-check', tone: 'ok' },
    fail:         { label: 'Fail', icon: 'circle-x', tone: 'failed' },
    cant:         { label: "Can't verify", icon: 'circle-help', tone: 'warning' },
    manual:       { label: 'Checked manually', icon: 'user-round-check', tone: 'ok' },
    skipped:      { label: 'Skipped', icon: 'forward', tone: 'muted' },
    follows:      { label: 'Follows design', icon: 'signpost-big', tone: 'pr' }
  };
  var SOURCE_KINDS = {
    repo:   { label: 'Code repo or folder', icon: 'folder-code', release: '2.0' },
    figma:  { label: 'Figma file', icon: 'pen-tool', release: '2.1' },
    url:    { label: 'Deployed prototype URL', icon: 'globe', release: '2.1' },
    images: { label: 'Folder of images or PDFs', icon: 'file-image', release: '2.1' }
  };

  /* ---------------- gate logic (pure) ----------------
     Removed rows are never counted. Follows design is excluded from the gate.
     Warn mode never yields Blocked or Needs confirmation. */
  function counts(l) {
    var c = { total: 0, ok: 0, pass: 0, fail: 0, cant: 0, manual: 0, skipped: 0, follows: 0, not_reviewed: 0, stale: 0, pending: 0 };
    (l.rows || []).forEach(function (r) {
      c.total++;
      var v = r.verdict.v; c[v]++;
      if (r.verdict.stale) c.stale++;
      if (r.verdict.pending) c.pending++;
      if (v === 'pass' || v === 'manual' || v === 'skipped' || v === 'follows') c.ok++;
    });
    return c;
  }
  function gate(l) {
    var c = counts(l), mode = l.gate || 'block';
    var g = { mode: mode, c: c };
    if (!c.total) g.state = 'no_ac';
    else if (l.reviewing) g.state = 'reviewing';
    else if (c.stale || c.not_reviewed) g.state = 'review_needed';
    else if (mode === 'warn' && (c.fail || c.cant)) g.state = 'warn';
    else if (c.fail) g.state = 'blocked';
    else if (c.cant) g.state = 'needs_confirmation';
    else g.state = 'ready';
    var why = c.stale ? 'code changed after the last review' : (l.lastReview ? c.not_reviewed + ' AC not reviewed yet' : 'no review yet');
    g.text = {
      no_ac: 'No AC linked',
      reviewing: 'Reviewing ' + (l.reviewing ? l.reviewing.done + ' of ' + l.reviewing.total : '') + ' AC',
      review_needed: 'Review needed: ' + why,
      warn: (c.fail ? c.fail + ' AC failed' : '') + (c.fail && c.cant ? ', ' : '') + (c.cant ? c.cant + " can't be verified" : '') + ' (warn mode)',
      blocked: 'Push blocked by ' + c.fail + ' AC',
      needs_confirmation: 'Needs your check: ' + c.cant + " AC can't be verified",
      ready: 'Ready to push'
    }[g.state];
    g.tone = { no_ac: 'muted', reviewing: 'working', review_needed: 'waiting', warn: 'warning', blocked: 'failed', needs_confirmation: 'warning', ready: 'ok' }[g.state];
    g.icon = { no_ac: 'list-x', reviewing: 'loader-circle', review_needed: 'scan-search', warn: 'triangle-alert', blocked: 'lock', needs_confirmation: 'circle-help', ready: 'circle-check' }[g.state];
    g.warnings = c.fail + c.cant;
    return g;
  }
  function selectable(r) { return r.verdict.v === 'fail' && !r.verdict.stale && !r.fixing && !r.verdict.pending; }

  /* ---------------- renderers ---------------- */
  function chip(kind, v, opts) {
    opts = opts || {};
    var d = (kind === 'design' ? DESIGN : VERDICT)[v];
    if (opts.pending) return '<span class="chip" data-tone="working" data-ui="Badge / custom=VerdictChip / state=reviewing">' + ic('loader-circle', 'spin') + 'Reviewing</span>';
    return '<span class="chip" data-tone="' + d.tone + '"' + (opts.stale ? ' data-stale="true"' : '') + ' data-ui="Badge / custom=' + (kind === 'design' ? 'DesignChip' : 'VerdictChip') + ' / value=' + v + (opts.stale ? ' / stale' : '') + '">' +
      ic(d.icon) + d.label + (opts.stale ? '<span class="chip-stale" title="The code changed after this verdict">' + ic('history') + 'Stale</span>' : '') + '</span>';
  }
  function fileLink(f) {
    return '<button class="ev-file" data-open-file="' + esc(f[0] + '|' + f[1]) + '" title="Open in Changes">' + ic('file-code-2', 'icon-sm') +
      '<span class="repo">' + esc(f[0]) + '</span><span>' + esc(f[1]) + ':' + f[2] + '</span></button>';
  }
  function evidence(r, opts) {
    opts = opts || {};
    var v = r.verdict, out = '';
    var head = { pass: 'Reviewer found it working', fail: 'Reviewer found a problem', cant: "Reviewer couldn't verify this", manual: 'Checked manually', skipped: 'Skipped with Push anyway', follows: 'Not reviewed against the AC', not_reviewed: 'Not reviewed yet' }[v.v];
    out += '<div class="ev-col"><div class="ev-h">' + ic('scan-search', 'icon-sm') + 'Verdict' + (v.stale ? ' <span class="muted">(from before the code changed)</span>' : '') + '</div><p class="ev-lead">' + head + '</p>';
    if (v.why) out += '<p>' + esc(v.why) + '</p>';
    if (v.v === 'follows') out += '<p class="muted">You marked this AC as outdated. The agent follows the design, the row is left out of the gate, and the PM was asked to update the AC in Lark.</p>';
    if (v.files && v.files.length) out += '<div class="ev-files">' + v.files.map(fileLink).join('') + '</div>';
    if (v.test) out += '<div class="ev-sub">Test output</div><pre class="ev-pre">' + esc(v.test) + '</pre>';
    if (v.steps && v.steps.length) out += '<div class="ev-sub">Browser steps</div><ol class="ev-steps">' + v.steps.map(function (s) { return '<li>' + esc(s) + '</li>'; }).join('') + '</ol>';
    out += '</div>';
    if (!opts.noDesign) {
      out += '<div class="ev-col"><div class="ev-h">' + ic('frame', 'icon-sm') + 'Design</div>';
      if (!r.design) out += '<p class="muted">No design source for this run.</p>';
      else {
        out += '<p class="ev-lead">' + DESIGN[r.design.v].label + '</p>';
        if (r.design.where) out += '<p class="mono ev-where">' + esc(r.design.where) + '</p>';
        if (r.design.note) out += '<p>' + esc(r.design.note) + '</p>';
        if (r.design.v === 'not_checked') out += '<p class="muted">Planning has not reported on this AC yet.</p>';
      }
      out += '</div>';
    }
    return out;
  }
  function row(r, opts) {
    opts = opts || {};
    var v = r.verdict, sel = !opts.readonly && selectable(r);
    var lead = r.fixing && !opts.readonly
      ? '<span class="ac-fixing" title="Fix in progress">' + ic('wrench') + '<span class="sr-only">Fix in progress</span></span>'
      : sel ? '<input type="checkbox" class="checkbox" tabindex="-1" aria-label="Select ' + esc(r.id) + ' for Send to agent"' + (opts.selected ? ' checked' : '') + '>' : '';
    var marks = (r.isNew ? '<span class="mark" data-k="new">New</span>' : '') + (r.changed ? '<span class="mark" data-k="changed">Changed</span>' : '') + (r.fixing && !opts.readonly ? '<span class="mark" data-k="fix">' + ic('wrench') + 'Fix in progress</span>' : '');
    return '<div class="ac-row" role="row" tabindex="-1" data-id="' + esc(r.id) + '" aria-expanded="' + !!opts.expanded + '"' + (sel ? ' aria-selected="' + !!opts.selected + '"' : '') +
        (v.stale ? ' data-stale="true"' : '') + (r.fixing ? ' data-fixing="true"' : '') + ' data-ui="ACLedgerRow / design=' + (r.design ? r.design.v : 'none') + ' / verdict=' + v.v + '">' +
      '<span class="ac-c ac-sel" role="gridcell">' + lead + '</span>' +
      '<span class="ac-c ac-id" role="gridcell"><span class="mono">' + esc(r.id) + '</span>' + marks + '</span>' +
      '<span class="ac-c ac-txt" role="gridcell"><span class="clamp">' + esc(r.text) + '</span></span>' +
      '<span class="ac-c ac-d" role="gridcell">' + (r.design ? chip('design', r.design.v) : '') + '</span>' +
      '<span class="ac-c ac-v" role="gridcell">' + chip('verdict', v.v, { stale: v.stale, pending: v.pending }) + '</span>' +
      '<span class="ac-c ac-x" role="gridcell"><button class="btn btn-ghost btn-xs btn-icon" tabindex="-1" data-expand aria-label="' + (opts.expanded ? 'Collapse ' : 'Expand ') + esc(r.id) + '">' + ic(opts.expanded ? 'chevron-down' : 'chevron-right') + '</button></span>' +
    '</div>' +
    (opts.expanded ? '<div class="ac-detail" role="row"><div role="gridcell" class="ac-ev">' + evidence(r, { noDesign: opts.noSource && !r.design }) + '</div></div>' : '');
  }
  function removedRow(r) {
    return '<div class="ac-row" role="row" data-removed="true" data-ui="ACLedgerRow / removed">' +
      '<span class="ac-c ac-sel" role="gridcell"></span><span class="ac-c ac-id" role="gridcell"><span class="mono">' + esc(r.id) + '</span></span>' +
      '<span class="ac-c ac-txt" role="gridcell"><span class="clamp">' + esc(r.text) + '</span></span>' +
      '<span class="ac-c ac-rm" role="gridcell"><span class="chip" data-tone="muted">' + ic('circle-minus') + 'Removed in Lark</span><span class="muted">Not counted, not in the gate</span></span></div>';
  }
  function countChips(c) {
    var order = [['pass', 'pass'], ['fail', 'fail'], ['cant', 'cant'], ['manual', 'manual'], ['skipped', 'skipped'], ['follows', 'follows'], ['not_reviewed', 'not_reviewed']];
    return order.filter(function (o) { return c[o[0]]; }).map(function (o) { var d = VERDICT[o[1]];
      return '<span class="count-chip" data-tone="' + d.tone + '">' + ic(d.icon) + '<b>' + c[o[0]] + '</b> ' + d.label + '</span>'; }).join('');
  }
  function summary(l, opts) {
    opts = opts || {};
    var g = gate(l), c = g.c, lr = l.lastReview;
    var meta = l.reviewing ? 'Started just now · ' + esc(l.reviewing.trigger || 'Manual') + (l.reviewing.browser ? ' · browser used' : '')
      : lr ? 'Last review ' + esc(lr.at) + ' · ' + esc(lr.trigger) + (lr.browser ? ' · browser used' : ' · no browser') : 'No review yet';
    var runBtn = opts.readonly ? '' : l.reviewing
      ? '<button class="btn btn-outline btn-sm" disabled>' + ic('loader-circle', 'spin') + 'Reviewing…</button>'
      : '<button class="btn btn-' + (g.state === 'review_needed' ? 'default' : 'outline') + ' btn-sm" id="ac-run"' + (opts.busy ? ' disabled aria-describedby="ac-run-why"' : '') + '>' + ic(l.reviewFailed ? 'rotate-ccw' : 'scan-search') + (l.reviewFailed ? 'Retry review' : 'Run review') + '</button>';
    var src = l.designSource ? '<span class="src">' + ic(SOURCE_KINDS[l.designSource.kind].icon, 'icon-sm') + 'Design source <b>' + esc(l.designSource.name) + '</b></span>' : '<span class="src" data-none="true">' + ic('frame', 'icon-sm') + 'No design source</span>';
    return '<div class="ac-summary" data-ui="ACReviewSummary / gate=' + g.state + '">' +
      '<div class="ac-sum-top">' +
        '<span class="gate-state" data-tone="' + g.tone + '">' + ic(g.icon, g.state === 'reviewing' ? 'spin' : '') + '<span>' + esc(g.text) + '</span></span>' +
        (g.mode === 'warn' ? '<span class="badge" title="Set per workspace in Settings">' + ic('triangle-alert') + 'Warn mode</span>' : '') +
        '<span class="muted ac-meta">' + meta + '</span>' +
        '<span class="right">' + (opts.busy && !l.reviewing ? '<span class="muted" id="ac-run-why">The agent is working. Review when the turn ends.</span>' : '') + runBtn + '</span>' +
      '</div>' +
      '<div class="ac-sum-bottom">' + countChips(c) + (l.reviewing ? '<span class="ac-progress" aria-label="Review progress"><i style="width:' + Math.round(l.reviewing.done / l.reviewing.total * 100) + '%"></i></span>' : '') + src + '</div>' +
    '</div>';
  }
  function tabCount(l) {
    var c = counts(l);
    if (!c.total) return '';
    if (l.reviewing) return ic('loader-circle', 'spin icon-sm') + '<span class="sr-only">reviewing</span>';
    return '<span class="mono">' + c.ok + '/' + c.total + '</span>' + (c.fail ? '<span class="tab-fail" title="' + c.fail + ' failed">' + ic('circle-x', 'icon-sm') + '<span class="sr-only">' + c.fail + ' failed</span></span>' : '');
  }
  /* Gate button (action bar). Returns html; the controller binds #act-push. */
  function gateButton(l, opts) {
    opts = opts || {};
    var g = gate(l), dis = opts.disabled ? ' disabled' : '';
    var base = 'Push &amp; create PR';
    switch (g.state) {
      case 'reviewing':
        return '<button class="btn btn-default btn-sm" id="act-push" disabled data-ui="Button / custom=GateButton / state=reviewing">' + ic('loader-circle', 'spin') + 'Reviewing ' + l.reviewing.done + '/' + l.reviewing.total + '</button>';
      case 'review_needed':
        return '<button class="btn btn-default btn-sm" id="act-push"' + dis + ' data-ui="Button / custom=GateButton / state=review-needed" title="Starts a review first. Push again when it finishes.">' + ic('scan-search') + base + '<span class="gate-tag">Review needed</span></button>';
      case 'blocked':
        return '<button class="btn btn-outline btn-sm" id="act-push" aria-haspopup="dialog" data-ui="Button / custom=GateButton / state=blocked">' + ic('lock') + 'Push blocked<span class="gate-tag" data-tone="failed">' + g.c.fail + ' AC failed</span>' + ic('chevron-down') + '</button>';
      case 'needs_confirmation':
        return '<button class="btn btn-default btn-sm" id="act-push"' + dis + ' aria-haspopup="dialog" data-ui="Button / custom=GateButton / state=needs-confirmation">' + ic('circle-help') + base + '<span class="gate-tag">Needs confirmation</span></button>';
      case 'warn':
        return '<button class="btn btn-default btn-sm" id="act-push"' + dis + ' aria-haspopup="dialog" data-ui="Button / custom=GateButton / state=warn">' + ic('triangle-alert') + 'Push with ' + g.warnings + ' warning' + (g.warnings > 1 ? 's' : '') + '</button>';
      default:
        return '<button class="btn btn-default btn-sm" id="act-push"' + dis + ' data-ui="Button / custom=GateButton / state=ready">' + ic('git-pull-request-arrow') + base + '</button>';
    }
  }
  /* Readiness checklist item (section 4.4) */
  function readyItem(it) {
    var st = { ok: ['circle-check', 'ok'], missing: ['circle-x', 'failed'], warn: ['triangle-alert', 'warning'], confirm: ['circle-dashed', 'muted'] }[it.state];
    return '<li class="ready-item" data-state="' + it.state + '" data-ui="ReadinessItem / state=' + it.state + '">' +
      '<span class="ri-ico" data-tone="' + st[1] + '">' + ic(st[0]) + '</span>' +
      '<div class="ri-txt"><div class="ri-title">' + it.title + '</div>' + (it.help ? '<div class="muted">' + it.help + '</div>' : '') + '</div>' +
      (it.action ? '<span class="ri-act">' + it.action + '</span>' : '') + '</li>';
  }
  /* Design source list item (Settings, Start dialog) */
  function sourceItem(s) {
    var k = SOURCE_KINDS[s.kind];
    var st = { ok: ['circle-check', 'ok', 'Reachable'], error: ['circle-x', 'failed', s.error || 'Unreachable'], untested: ['circle-dashed', 'muted', 'Not tested'] }[s.status || 'ok'];
    return '<div class="source-item" data-ui="DesignSourceItem / kind=' + s.kind + '"' + (k.release === '2.1' ? ' data-release="2.1"' : '') + '>' +
      '<span class="si-ico">' + ic(k.icon) + '</span>' +
      '<div class="si-txt"><div class="si-name">' + esc(s.name) + (k.release === '2.1' ? '<span class="rel-tag">2.1</span>' : '') + '</div><div class="si-loc mono">' + esc(s.location) + '</div><div class="muted si-kind">' + k.label + '</div></div>' +
      '<span class="si-status" data-tone="' + st[1] + '">' + ic(st[0], 'icon-sm') + esc(st[2]) + '</span>' +
      (s.actions === false ? '' : '<span class="si-act"><button class="btn btn-ghost btn-xs">Test</button><button class="btn btn-ghost btn-xs btn-icon" aria-label="More actions for ' + esc(s.name) + '">' + ic('ellipsis') + '</button></span>') + '</div>';
  }

  /* Run app control (action bar). States from section 3.3. */
  var APP = {
    off:      { tone: 'muted', icon: 'circle-pause', label: 'App off', btn: ['play', 'Run app', 'outline'] },
    setup:    { tone: 'working', icon: 'loader-circle', spin: true, label: 'Setting up · bun install', btn: ['square', 'Stop app', 'ghost'] },
    starting: { tone: 'working', icon: 'loader-circle', spin: true, label: 'Starting kelola-api (1 of 2)', btn: ['square', 'Stop app', 'ghost'] },
    port:     { tone: 'working', icon: 'loader-circle', spin: true, label: 'Waiting for port 5173', btn: ['square', 'Stop app', 'ghost'] },
    running:  { tone: 'ok', icon: 'circle-check', label: 'Running', url: 'http://localhost:5173', btn: ['square', 'Stop app', 'ghost'] },
    older:    { tone: 'warning', icon: 'history', label: 'Running older code', url: 'http://localhost:5173', btn: ['rotate-ccw', 'Restart app', 'outline'] },
    failed:   { tone: 'failed', icon: 'circle-x', label: 'Failed to start · kelola-web', btn: ['scroll-text', 'View logs', 'outline'] },
    stopped:  { tone: 'muted', icon: 'circle-pause', label: 'Stopped: PGS-1430 started its app', btn: ['play', 'Run app', 'outline'] }
  };
  function runApp(state) {
    var a = APP[state];
    return '<span class="run-app" data-ui="RunAppControl / state=' + state + '">' +
      '<span class="run" data-tone="' + a.tone + '">' + ic(a.icon, a.spin ? 'spin' : '') + '<span class="lbl">' + a.label + '</span></span>' +
      (a.url ? '<a class="link mono" href="#" onclick="return false" title="Open in Chrome">' + esc(a.url) + '</a>' : '') +
      '<button class="btn btn-' + a.btn[2] + ' btn-sm">' + ic(a.btn[0]) + a.btn[1] + '</button></span>';
  }

  U.ac = { runApp: runApp, APP: APP, DESIGN: DESIGN, VERDICT: VERDICT, SOURCE_KINDS: SOURCE_KINDS, counts: counts, gate: gate, selectable: selectable, chip: chip, row: row, removedRow: removedRow, summary: summary, tabCount: tabCount, gateButton: gateButton, readyItem: readyItem, sourceItem: sourceItem, evidence: evidence };

  /* =====================================================================
     ACTAB controller: mounts the tab into #p-ac and owns review + gate flows.
     host: { key, panel, countEl, isBusy(), setRunState(s), publish(),
             openFile(repo, path), sendToAgent(text), onChange() }
     ===================================================================== */
  function mount(host) {
    var D = window.AC_DATA, P = U.params, $ = U.$, $$ = U.$$;
    var forced = P.get('ac') || host.previewScenario || null;
    var FRAME = P.get('frame') === '1';
    var SKEY = 'mandor:v1:ac:' + host.key;
    var l = (!forced && !FRAME && JSON.parse(localStorage.getItem(SKEY) || 'null')) || D.scenario(forced || host.defaultScenario || 'design');
    var round = l.round || (forced === 'current' || forced === 'stale' || forced === 'warn' ? 1 : 0);
    var focusIdx = 0, expanded = {}, selected = {}, viewing = null, timers = [];
    if (P.get('expand')) P.get('expand').split(',').forEach(function (id) { expanded[id] = true; });
    /* A review cannot survive a reload of the prototype: show it as interrupted */
    if (!forced && l.reviewing) {
      var lost = l.rows.filter(function (r) { return r.verdict.pending; });
      lost.forEach(function (r) { r.verdict = { v: 'not_reviewed' }; });
      l.reviewing = null; l.reviewFailed = 'The review was interrupted. ' + lost.map(function (r) { return r.id; }).join(', ') + ' were not reviewed.';
    }
    if (forced === 'selected') l.rows.forEach(function (r) { if (selectable(r)) selected[r.id] = true; });
    if (forced === 'fixing') l.rows.forEach(function (r) { if (r.verdict.v === 'fail') r.fixing = true; });

    function save() { l.round = round; if (!forced && !FRAME) localStorage.setItem(SKEY, JSON.stringify(l)); }
    function rows() { return l.rows; }

    function render() {
      var panel = host.panel, busy = host.isBusy();
      host.countEl.innerHTML = tabCount(l);
      if (!l.rows.length) {
        panel.innerHTML = '<div class="empty" data-ui="ACReview / empty">' + '<span class="ico">' + ic('list-x') + '</span><h3>No acceptance criteria for this task</h3><p>Nothing to review against. Link AC to the task in Lark and Mandor adds them here on the next sync. Push &amp; create PR is not gated.</p><button class="btn btn-outline btn-sm" id="ac-open-lark">' + ic('external-link') + 'Open in Lark</button></div>';
        U.$('#ac-open-lark').onclick = function () { U.toast('Opening ' + host.key + ' in Lark', null, 'external-link'); };
        U.icons(); host.onChange(); return;
      }
      var view = viewing !== null ? l.history[viewing] : null;
      var src = view ? l.rows.map(function (r) { return Object.assign({}, r, { verdict: view.snap[r.id] || { v: 'not_reviewed' }, fixing: false }); }) : l.rows;
      var nSel = Object.keys(selected).filter(function (k) { return selected[k]; }).length;
      var noSource = !l.designSource;
      var html = '';
      if (view) html += '<div class="alert alert-info ac-viewing" role="status">' + ic('history') + '<div><div class="alert-title">Viewing the review from ' + esc(view.at) + ' (read-only)</div><div class="muted">' + esc(view.trigger) + (view.browser ? ' · browser used' : ' · no browser') + (view.failed ? ' · stopped before finishing' : '') + '</div></div><div class="alert-actions"><button class="btn btn-outline btn-sm" id="ac-back">Back to current</button></div></div>';
      else html += summary(l, { busy: busy });
      if (!view && l.reviewFailed) html += '<div class="alert alert-failed" role="alert">' + ic('circle-x') + '<div><div class="alert-title">Review failed</div><div class="muted">' + esc(l.reviewFailed) + ' Their verdicts stay Not reviewed.</div></div><div class="alert-actions"><button class="btn btn-outline btn-sm" id="ac-retry">' + ic('rotate-ccw') + 'Retry</button></div></div>';
      if (!view && l.staleReason && gate(l).c.stale) html += '<div class="alert alert-warning" role="status">' + ic('history') + '<div><div class="alert-title">Results are stale</div><div class="muted">' + esc(l.staleReason) + ' Previous verdicts are kept for reference. Run a review before pushing.</div></div></div>';
      if (!view && nSel) html += '<div class="ac-selbar" data-ui="ACReview / SelectionBar"><span><b>' + nSel + '</b> failed AC selected</span><button class="btn btn-ghost btn-sm" id="ac-clear">Clear</button><button class="btn btn-default btn-sm" id="ac-send"' + (busy ? ' disabled title="The agent is working"' : '') + '>' + ic('send-horizontal') + 'Send to agent</button></div>';
      html += '<div class="ac-ledger" role="grid" aria-label="AC ledger for ' + esc(host.key) + '" aria-multiselectable="true" aria-readonly="' + !!view + '" data-ui="ACLedger">' +
        '<div class="ac-row ac-head" role="row"><span class="ac-c ac-sel" role="columnheader"><span class="sr-only">Select</span></span><span class="ac-c ac-id" role="columnheader">AC</span><span class="ac-c ac-txt" role="columnheader">Criterion</span>' +
        '<span class="ac-c ac-d" role="columnheader">Design' + (noSource ? '<span class="chip" data-tone="muted" data-ui="Badge / custom=DesignChip / value=no-source">' + ic('frame') + 'No design source</span>' : '') + '</span>' +
        '<span class="ac-c ac-v" role="columnheader">Verdict' + (view ? ' <span class="muted">(' + esc(view.at) + ')</span>' : '') + '</span><span class="ac-c ac-x" role="columnheader"><span class="sr-only">Expand</span></span></div>' +
        src.map(function (r) { return row(r, { expanded: !!expanded[r.id], selected: !!selected[r.id], readonly: !!view, noSource: noSource }); }).join('') +
        (l.removed || []).map(removedRow).join('') +
      '</div>';
      html += historyHtml();
      panel.innerHTML = '<div class="ac-wrap">' + html + '</div>';
      bindTab();
      U.icons(); host.onChange();
    }

    function historyHtml() {
      if (!l.history.length) return '<div class="ac-history"><div class="ac-hist-empty muted">' + ic('history', 'icon-sm') + 'No reviews yet. Each review is kept here with its counts.</div></div>';
      return '<details class="ac-history" data-ui="Collapsible / ReviewHistory"' + (viewing !== null || P.get('history') === 'open' ? ' open' : '') + '><summary>' + ic('chevron-right', 'icon-sm') + 'Review history <span class="muted">(' + l.history.length + ')</span></summary><ol>' +
        l.history.map(function (h, i) {
          var cc = Object.keys(h.counts).map(function (k) { var d = VERDICT[k]; return '<span class="count-chip" data-tone="' + d.tone + '">' + ic(d.icon) + '<b>' + h.counts[k] + '</b> ' + d.label + '</span>'; }).join('');
          return '<li><button class="ac-hist-item" data-hist="' + i + '" aria-pressed="' + (viewing === i) + '"><span class="when">' + esc(h.at) + '</span><span class="muted">' + esc(h.trigger) + (h.browser ? ' · browser' : '') + '</span>' +
            (h.failed ? '<span class="chip" data-tone="failed">' + ic('circle-x') + 'Stopped</span>' : '') + '<span class="cc">' + cc + '</span>' + (i === 0 && !h.failed ? '<span class="muted latest">Latest</span>' : '') + '</button></li>';
        }).join('') + '</ol></details>';
    }

    function bindTab() {
      var panel = host.panel;
      var rowsEl = $$('.ac-ledger .ac-row[data-id]', panel);
      rowsEl.forEach(function (el, i) {
        el.tabIndex = i === Math.min(focusIdx, rowsEl.length - 1) ? 0 : -1;
        el.onclick = function (e) {
          focusIdx = i;
          if (e.target.closest('[data-open-file]')) return;
          if (e.target.matches('input[type="checkbox"]')) { toggleSel(el.dataset.id); return; }
          toggleExp(el.dataset.id);
        };
        el.onkeydown = function (e) {
          if (e.target !== el) return;
          if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); focusIdx = Math.max(0, Math.min(rowsEl.length - 1, i + (e.key === 'ArrowDown' ? 1 : -1))); rowsEl[focusIdx].focus(); rowsEl.forEach(function (x, j) { x.tabIndex = j === focusIdx ? 0 : -1; }); }
          if (e.key === 'Home' || e.key === 'End') { e.preventDefault(); focusIdx = e.key === 'Home' ? 0 : rowsEl.length - 1; rowsEl[focusIdx].focus(); }
          if (e.key === ' ') { e.preventDefault(); if (el.hasAttribute('aria-selected')) toggleSel(el.dataset.id, true); }
          if (e.key === 'Enter') { e.preventDefault(); toggleExp(el.dataset.id, true); }
        };
      });
      $$('[data-open-file]', panel).forEach(function (b) { b.onclick = function () { var p = b.dataset.openFile.split('|'); host.openFile(p[0], p[1]); }; });
      $$('[data-hist]', panel).forEach(function (b) { b.onclick = function () { var i = +b.dataset.hist; viewing = viewing === i && i !== 0 ? null : (i === 0 && !l.history[0].failed && !gate(l).c.stale ? null : i); render(); }; });
      bindIf('#ac-back', function () { viewing = null; render(); });
      bindIf('#ac-run', function () { startReview('Manual'); });
      bindIf('#ac-retry', function () { startReview('Manual'); });
      bindIf('#ac-clear', function () { selected = {}; render(); });
      bindIf('#ac-send', sendSelected);
    }
    function bindIf(sel, fn) { var el = $(sel); if (el) el.onclick = fn; }
    function refocus() { var el = $$('.ac-ledger .ac-row[data-id]', host.panel)[focusIdx]; if (el) el.focus(); }
    function toggleSel(id, keep) { if (viewing !== null) return; selected[id] = !selected[id]; render(); if (keep) refocus(); }
    function toggleExp(id, keep) { expanded[id] = !expanded[id]; render(); if (keep) refocus(); }

    /* ---------- Send to agent ---------- */
    function sendSelected() {
      var picked = l.rows.filter(function (r) { return selected[r.id] && selectable(r); });
      if (!picked.length) return;
      picked.forEach(function (r) { r.fixing = true; });
      selected = {}; save(); render();
      host.sendToAgent('Fix these AC review findings:\n' + picked.map(function (r) { return '- ' + r.id + ' failed: ' + (r.verdict.why || r.text); }).join('\n'), picked);
    }
    /* Called by the workspace when an agent turn finishes with code changes */
    function codeChanged(files) {
      var any = false;
      l.rows.forEach(function (r) { if (r.verdict.v !== 'not_reviewed' && r.verdict.v !== 'follows') { r.verdict.stale = true; any = true; } });
      if (any) l.staleReason = (files || 'Files') + ' changed after the review at ' + (l.lastReview ? l.lastReview.at.replace('Today ', '') : 'the last review') + '.';
      save(); render();
    }

    /* ---------- Review run ---------- */
    function needsReadiness() { return l.webRepo && l.browserTesting; }
    function startReview(trigger) {
      if (host.isBusy() || l.reviewing) return;
      if (needsReadiness() && P.get('readiness') !== 'skip') openReadiness(trigger);
      else runReview(trigger, false);
    }
    function runReview(trigger, browser) {
      var script = D.reviewScript[Math.min(round, D.reviewScript.length - 1)];
      var targets = l.rows.filter(function (r) { return r.verdict.v !== 'follows'; });
      l.reviewFailed = null; l.staleReason = null;
      l.reviewing = { done: 0, total: targets.length, trigger: trigger, browser: browser };
      targets.forEach(function (r) { r.verdict = { v: 'not_reviewed', pending: true }; });
      host.setRunState('reviewing');
      render();
      var full = D.scenario('current').rows.reduce(function (m, r) { m[r.id] = r.verdict; return m; }, {});
      targets.forEach(function (r, i) {
        timers.push(setTimeout(function () {
          var res = script[r.id] || 'pass', v;
          if (r.id === 'AC-3' && res === 'pass') v = JSON.parse(JSON.stringify(D.fixedAc3));
          else if (res === 'pass' && full[r.id] && full[r.id].v === 'pass') v = JSON.parse(JSON.stringify(full[r.id]));
          else if (full[r.id] && full[r.id].v === res) v = JSON.parse(JSON.stringify(full[r.id]));
          else v = { v: res, why: res === 'pass' ? 'Reviewer found the behavior in the code and tests.' : 'Reviewer found no code for this AC.' };
          if (res === 'cant' && !browser) v.why = "Can't verify: browser testing was not used for this review, so the reviewer could not check the screen.";
          if (res === 'cant' && r.id === 'AC-4' && browser) v.why = "Can't verify: Lark messages are stubbed in local development, so the reviewer cannot see them.";
          r.verdict = v; r.fixing = false; l.reviewing.done++;
          if (P.get('reviewfail') === '1' && i === 2) return failReview(trigger, browser);
          if (l.reviewing.done === l.reviewing.total) finishReview(trigger, browser);
          else { render(); }
        }, 650 * (i + 1)));
      });
    }
    function stamp() { var d = new Date(); return 'Today ' + String(d.getHours()).padStart(2, '0') + ':' + String(d.getMinutes()).padStart(2, '0'); }
    function snap() { var s = {}; l.rows.forEach(function (r) { s[r.id] = JSON.parse(JSON.stringify(r.verdict)); }); return s; }
    function failReview(trigger, browser) {
      timers.forEach(clearTimeout); timers = [];
      var left = l.rows.filter(function (r) { return r.verdict.pending; });
      left.forEach(function (r) { r.verdict = { v: 'not_reviewed' }; });
      l.reviewFailed = 'The review stopped after 15 minutes (time limit). ' + left.map(function (r) { return r.id; }).join(', ') + ' were not reviewed.';
      var c = counts(l); l.history.unshift({ at: stamp(), trigger: trigger, browser: browser, counts: pick(c), failed: true, snap: snap() });
      l.reviewing = null; save(); host.setRunState('awaiting_dev'); render();
      U.toast('Review failed', 'Time limit reached. Retry from the AC review tab.', 'circle-x');
    }
    function pick(c) { var o = {}; ['pass', 'fail', 'cant', 'manual', 'skipped', 'follows'].forEach(function (k) { if (c[k]) o[k] = c[k]; }); return o; }
    function finishReview(trigger, browser) {
      var c = counts(l);
      l.lastReview = { at: stamp(), trigger: trigger, browser: browser };
      l.history.unshift({ at: l.lastReview.at, trigger: trigger, browser: browser, counts: pick(c), snap: snap() });
      l.reviewing = null; round++; save();
      host.setRunState('awaiting_dev'); render();
      var g = gate(l);
      var fails = l.rows.filter(function (r) { return r.verdict.v === 'fail'; }).map(function (r) { return r.id; });
      var cants = l.rows.filter(function (r) { return r.verdict.v === 'cant'; }).map(function (r) { return r.id; });
      var desc = (fails.length ? fails.join(', ') + ' failed. ' : '') + (cants.length ? cants.join(', ') + " can't be verified. " : '') + (!fails.length && !cants.length ? 'Reviewer found every AC passing. ' : '');
      U.toast('Review finished: ' + c.pass + ' passed' + (c.fail ? ', ' + c.fail + ' failed' : ''), desc + (trigger === 'Push & create PR' ? (g.state === 'blocked' ? 'Nothing was pushed. Send ' + fails.join(', ') + ' to the agent, or push anyway with a reason.' : 'Nothing was pushed yet. Press Push & create PR again.') : ''), fails.length ? 'circle-x' : 'scan-search');
    }

    /* ---------- dialogs ---------- */
    var dlg = document.createElement('dialog');
    dlg.className = 'dialog'; dlg.id = 'dlg-gate'; dlg.style.width = '560px'; dlg.setAttribute('aria-labelledby', 'gd-title');
    document.body.appendChild(dlg);
    function openDlg(html, ui) { dlg.setAttribute('data-ui', ui); dlg.innerHTML = html; U.icons(); if (!dlg.open) dlg.showModal(); }

    var readyConfirmed = {};
    function openReadiness(trigger) {
      var app = l.app || { state: 'off' };
      var ast = host.appState ? host.appState() : app.state;
      if (P.get('ready') === 'older') ast = 'older'; if (P.get('ready') === 'off') ast = 'off';
      var starting = /setup|starting|port/.test(ast), older = ast === 'older', off = !starting && ast !== 'running' && ast !== 'older', noext = P.get('ready') === 'noext' || !l.extension;
      var url = (app.urls || []).filter(function (u) { return u[0] === l.webRepo; })[0];
      var ck = function (k) { return readyConfirmed[k] ? ' checked' : ''; };
      var items = [
        { state: 'confirm', key: 'db', title: 'Database is running', help: 'Mandor does not start databases. Start it yourself, then confirm.', action: '<label class="ri-check"><input type="checkbox" class="checkbox" data-ready="db"' + ck('db') + '> Confirmed</label>' },
        starting ? { state: 'missing', title: 'App is starting', help: 'Wait until it is running, or run the review without the browser.' }
          : off ? { state: 'missing', title: 'App is not running', help: (ast === 'other' ? 'The app running now belongs to another run. ' : ast === 'failed' ? 'The last start failed. ' : '') + 'Start it with Run app. The reviewer uses ' + esc(l.webRepo) + ' in the browser.', action: '<button class="btn btn-outline btn-xs" data-ra="start">' + ic('play') + 'Run app</button>' }
          : older ? { state: 'warn', title: 'App is running older code', help: 'The code changed after the app started. Restart so the reviewer sees the latest changes.', action: '<button class="btn btn-outline btn-xs" data-ra="restart">' + ic('rotate-ccw') + 'Restart app</button>' }
          : { state: 'ok', title: 'App is running from the latest code', help: '<span class="mono">' + esc(url ? url[1] : '') + '</span>' },
        { state: 'confirm', key: 'login', title: 'Logged in at <span class="mono">' + esc(url ? url[1] : 'the app URL') + '</span>', help: 'Log in with your own account in Chrome. Mandor cannot see the login and never receives your password.', action: '<label class="ri-check"><input type="checkbox" class="checkbox" data-ready="login"' + ck('login') + '> I\'m logged in</label>' },
        noext ? { state: 'missing', title: 'Claude in Chrome extension not connected', help: 'Open Chrome and enable the extension, then check again.', action: '<button class="btn btn-outline btn-xs" data-ra="recheck">' + ic('refresh-cw') + 'Check again</button>' }
          : { state: 'ok', title: 'Claude in Chrome extension connected', help: 'The reviewer uses its own tab, only at the app URL.' },
        { state: 'ok', title: 'Browser testing allowed for ' + esc(l.workspace), help: 'Page content is read by the AI model. Change this in Settings › Workspaces.' }
      ];
      function body() {
        var dbOk = $('[data-ready="db"]', dlg) && $('[data-ready="db"]', dlg).checked, lgOk = $('[data-ready="login"]', dlg) && $('[data-ready="login"]', dlg).checked;
        return items.every(function (it) { return it.state === 'ok' || (it.key === 'db' && dbOk) || (it.key === 'login' && lgOk); });
      }
      openDlg('<div class="dialog-header"><span class="mono muted">' + esc(host.key) + ' · ' + esc(trigger) + '</span><h2 class="dialog-title" id="gd-title" style="margin-top:6px">Ready for a browser review?</h2>' +
        '<p class="dialog-description">The reviewer checks UI AC in your running app. Anything missing here makes those AC <b>Can\'t verify</b>.</p></div>' +
        '<div class="dialog-body"><ul class="ready-list" data-ui="ReadinessChecklist">' + items.map(readyItem).join('') + '</ul></div>' +
        '<div class="dialog-footer"><span class="left" id="rd-left"></span><button class="btn btn-ghost" data-close>Cancel</button><button class="btn btn-outline" id="rd-anyway">Run review anyway</button><button class="btn btn-default" id="rd-go" disabled>' + ic('scan-search') + 'Start review</button></div>', 'Dialog / ReadinessChecklist');
      function sync() { var ok = body(); $('#rd-go', dlg).disabled = !ok; $('#rd-left', dlg).textContent = ok ? 'All set.' : 'Confirm or fix the items above, or run without the browser.'; }
      $$('[data-ready]', dlg).forEach(function (cb) { cb.onchange = function () { readyConfirmed[cb.dataset.ready] = cb.checked; sync(); }; });
      var okItem = {
        start: { state: 'ok', title: 'App is running from the latest code', help: '<span class="mono">' + esc(url ? url[1] : '') + '</span>' },
        restart: { state: 'ok', title: 'App is running from the latest code', help: 'Restarted just now · <span class="mono">' + esc(url ? url[1] : '') + '</span>' },
        recheck: { state: 'ok', title: 'Claude in Chrome extension connected', help: 'The reviewer uses its own tab, only at the app URL.' }
      };
      function bindActions() {
        $$('[data-ra]', dlg).forEach(function (b) { b.onclick = function () {
          var li = b.closest('.ready-item'), idx = $$('.ready-item', dlg).indexOf(li), kind = b.dataset.ra;
          /* Run app and Restart app go through the Run app confirmation, then come back here */
          if (kind !== 'recheck' && host.appStart) { dlg.close(); host.appStart({ restart: kind === 'restart', after: function () { openReadiness(trigger); } }); return; }
          b.disabled = true; b.innerHTML = ic('loader-circle', 'spin') + (kind === 'recheck' ? 'Checking…' : 'Starting…'); U.icons();
          setTimeout(function () { items[idx] = okItem[kind]; li.outerHTML = readyItem(items[idx]); U.icons(); sync(); }, 1200);
        }; });
      }
      bindActions(); sync();
      $('#rd-anyway', dlg).onclick = function () { dlg.close(); runReview(trigger, false); };
      $('#rd-go', dlg).onclick = function () { dlg.close(); runReview(trigger, true); };
    }

    function rowsOf(v) { return l.rows.filter(function (r) { return r.verdict.v === v; }); }
    function syncWarn() { return P.get('acsync') === 'failed' ? '<div class="alert alert-warning" style="margin-bottom:12px">' + ic('refresh-cw-off') + '<div><div class="alert-title">Couldn\'t refresh the AC from Lark just now</div><div class="muted">Using the AC synced at ' + esc(l.acSyncedAt) + '. The PR records that time.</div></div></div>' : ''; }
    function prPreview(extra) {
      var lines = l.rows.map(function (r) { var v = (extra && extra[r.id]) || r.verdict; return '- ' + r.id + ': ' + VERDICT[v.v].label + (v.reason ? ' (reason: ' + v.reason + ')' : '') + (v.v === 'manual' ? ' (checked by Beni Handoko)' : ''); });
      (l.removed || []).forEach(function (r) { lines.push('- ' + r.id + ': removed in Lark, not reviewed'); });
      return '<details class="pr-preview" data-ui="Collapsible / PRDescriptionPreview"><summary>' + ic('chevron-right', 'icon-sm') + 'Preview the AC summary added to each PR</summary><pre class="ev-pre">## Acceptance criteria (' + esc(l.workspace) + ')\n' + esc(lines.join('\n')) + '\n\nReviewed by an AI reviewer. Verdicts are findings, not sign-off.\nAC synced from Lark at ' + esc(l.acSyncedAt) + '.</pre></details>';
    }
    function cantList(rows) {
      return '<div class="gate-list">' + rows.map(function (r) {
        return '<label class="gate-row"><input type="checkbox" class="checkbox" data-mc="' + esc(r.id) + '"><span><span class="mono">' + esc(r.id) + '</span> ' + esc(r.text) + '<span class="muted gate-why">' + esc(r.verdict.why || '') + '</span><span class="gate-ck">I checked this myself</span></span></label>';
      }).join('') + '</div>';
    }
    function failList(rows) {
      return '<div class="gate-list">' + rows.map(function (r) {
        return '<div class="gate-row">' + ic('circle-x', 'gate-x') + '<span><span class="mono">' + esc(r.id) + '</span> ' + esc(r.text) + '<span class="muted gate-why">Reviewer found: ' + esc(r.verdict.why || '') + '</span></span></div>';
      }).join('') + '</div>';
    }
    function confirmManual() {
      var cants = rowsOf('cant');
      openDlg('<div class="dialog-header"><h2 class="dialog-title" id="gd-title">Confirm the AC the reviewer couldn\'t verify</h2><p class="dialog-description">Check each one yourself before pushing. Your confirmation is recorded in the AC ledger and in the PR description.</p></div>' +
        '<div class="dialog-body">' + syncWarn() + cantList(cants) + prPreview() + '</div>' +
        '<div class="dialog-footer"><button class="btn btn-ghost" data-close>Cancel</button><button class="btn btn-default" id="gd-go" disabled>' + ic('git-pull-request-arrow') + 'Confirm and push</button></div>', 'Dialog / ConfirmManualCheck');
      var boxes = $$('[data-mc]', dlg);
      boxes.forEach(function (b) { b.onchange = function () { $('#gd-go', dlg).disabled = !boxes.every(function (x) { return x.checked; }); }; });
      $('#gd-go', dlg).onclick = function () { cants.forEach(function (r) { r.verdict = { v: 'manual', why: 'Checked manually by Beni Handoko.' }; }); save(); dlg.close(); render(); host.publish(l); };
    }
    function pushAnyway() {
      var fails = rowsOf('fail'), cants = rowsOf('cant');
      openDlg('<div class="dialog-header"><h2 class="dialog-title" id="gd-title">Push with ' + fails.length + ' failing AC?</h2><p class="dialog-description">These rows become <b>Skipped</b>. Your reason is written next to them in the PR description.</p></div>' +
        '<div class="dialog-body">' + syncWarn() + failList(fails) +
        '<div class="field" style="margin-top:14px"><label class="label" for="gd-reason">Reason <span class="muted" style="font-weight:400">(required, covers all of them)</span></label><textarea class="textarea" id="gd-reason" placeholder="e.g. AC-3 is handled in a follow-up task agreed with the PM." aria-describedby="gd-err"></textarea><p class="field-error" id="gd-err" hidden>' + ic('circle-alert', 'icon-sm') + 'Write a reason before pushing.</p></div>' +
        (cants.length ? '<div class="field"><span class="label">Also not verified</span>' + cantList(cants) + '</div>' : '') + prPreview() + '</div>' +
        '<div class="dialog-footer"><button class="btn btn-ghost" data-close>Cancel</button><button class="btn btn-destructive" id="gd-go">Push anyway</button></div>', 'AlertDialog / PushAnyway');
      $('#gd-go', dlg).onclick = function () {
        var reason = $('#gd-reason', dlg).value.trim();
        var allCk = $$('[data-mc]', dlg).every(function (x) { return x.checked; });
        if (!reason) { $('#gd-reason', dlg).setAttribute('aria-invalid', 'true'); $('#gd-err', dlg).hidden = false; $('#gd-reason', dlg).focus(); return; }
        if (!allCk) { U.toast('Confirm every AC the reviewer couldn\'t verify', null, 'circle-help'); return; }
        fails.forEach(function (r) { r.verdict = { v: 'skipped', reason: reason, why: 'Skipped with Push anyway: ' + reason }; });
        cants.forEach(function (r) { r.verdict = { v: 'manual', why: 'Checked manually by Beni Handoko.' }; });
        save(); dlg.close(); render(); host.publish(l);
      };
    }
    function warnConfirm() {
      var fails = rowsOf('fail'), cants = rowsOf('cant');
      openDlg('<div class="dialog-header"><h2 class="dialog-title" id="gd-title">Push with ' + (fails.length + cants.length) + ' warnings?</h2><p class="dialog-description">' + esc(l.workspace) + ' uses warn mode, so these do not block the push. They are listed in the PR description.</p></div>' +
        '<div class="dialog-body">' + syncWarn() + failList(fails) + (cants.length ? '<div class="gate-list" style="margin-top:6px">' + cants.map(function (r) { return '<div class="gate-row">' + ic('circle-help', 'gate-q') + '<span><span class="mono">' + esc(r.id) + '</span> ' + esc(r.text) + '<span class="muted gate-why">' + esc(r.verdict.why || '') + '</span></span></div>'; }).join('') + '</div>' : '') + prPreview() + '</div>' +
        '<div class="dialog-footer"><button class="btn btn-ghost" data-close>Cancel</button><button class="btn btn-default" id="gd-go">' + ic('git-pull-request-arrow') + 'Push &amp; create PR</button></div>', 'AlertDialog / WarnPush');
      $('#gd-go', dlg).onclick = function () { dlg.close(); host.publish(l); };
    }
    function readyPush() {
      if (P.get('acsync') === 'failed') {
        openDlg('<div class="dialog-header"><h2 class="dialog-title" id="gd-title">Push without fresh AC?</h2></div><div class="dialog-body">' + syncWarn() + prPreview() + '</div><div class="dialog-footer"><button class="btn btn-ghost" data-close>Cancel</button><button class="btn btn-default" id="gd-go">' + ic('git-pull-request-arrow') + 'Push &amp; create PR</button></div>', 'AlertDialog / ACSyncWarning');
        $('#gd-go', dlg).onclick = function () { dlg.close(); host.publish(l); };
      } else host.publish(l);
    }
    var pop = document.createElement('div');
    pop.className = 'popover gate-pop'; pop.hidden = true; pop.setAttribute('role', 'dialog'); pop.setAttribute('aria-label', 'Push blocked'); pop.setAttribute('data-ui', 'Popover / GateBlocked');
    document.body.appendChild(pop);
    document.addEventListener('click', function (e) { if (!e.target.closest('.gate-pop') && !e.target.closest('#act-push')) pop.hidden = true; });
    document.addEventListener('keydown', function (e) { if (e.key === 'Escape' && !pop.hidden) { pop.hidden = true; var b = $('#act-push'); if (b) b.focus(); } });
    function blockedPop(btn) {
      var fails = rowsOf('fail');
      pop.innerHTML = '<div class="gp-head">' + ic('lock') + '<b>Push blocked by ' + fails.length + ' AC</b></div>' +
        fails.map(function (r) { return '<div class="gp-row"><span class="mono">' + esc(r.id) + '</span><span>' + esc(r.verdict.why || r.text) + '</span></div>'; }).join('') +
        '<div class="gp-act"><button class="btn btn-ghost btn-sm" id="gp-view">View in AC review</button><button class="btn btn-outline btn-sm" id="gp-anyway">Push anyway…</button><button class="btn btn-default btn-sm" id="gp-send">' + ic('send-horizontal') + 'Send to agent</button></div>';
      var r = btn.getBoundingClientRect();
      pop.style.top = (r.bottom + 6) + 'px'; pop.style.left = Math.max(8, r.right - 360) + 'px'; pop.hidden = false; U.icons();
      $('#gp-view', pop).onclick = function () { pop.hidden = true; host.selectTab('ac'); };
      $('#gp-anyway', pop).onclick = function () { pop.hidden = true; pushAnyway(); };
      $('#gp-send', pop).onclick = function () { pop.hidden = true; fails.forEach(function (r) { if (selectable(r)) selected[r.id] = true; }); sendSelected(); };
      var first = $('button', pop); if (first) first.focus();
    }
    function onPush(e) {
      var g = gate(l);
      if (g.state === 'review_needed') startReview('Push & create PR');
      else if (g.state === 'blocked') blockedPop(e.currentTarget);
      else if (g.state === 'needs_confirmation') confirmManual();
      else if (g.state === 'warn') warnConfirm();
      else if (g.state === 'ready' || g.state === 'no_ac') readyPush();
    }

    render();
    return {
      ledger: function () { return l; }, gate: function () { return gate(l); }, render: render, codeChanged: codeChanged,
      gateButton: function (opts) { return gateButton(l, opts); }, onPush: onPush, startReview: startReview,
      openDialog: function (which) { ({ manual: confirmManual, anyway: pushAnyway, warn: warnConfirm, readiness: function () { openReadiness('Manual'); }, blocked: function () { var b = $('#act-push'); if (b) blockedPop(b); } }[which] || function () {})(); },
      reset: function () { localStorage.removeItem(SKEY); },
      /* Planning hands the ledger over: Design filled, no verdicts yet */
      load: function (name) { l = D.scenario(name); round = 0; expanded = {}; selected = {}; viewing = null; save(); render(); },
      /* Follow design (plan approval). Undo restores the original Design value. */
      setFollow: function (id, on) {
        var r = l.rows.filter(function (x) { return x.id === id; })[0]; if (!r || !r.design) return;
        if (on) { r.design = { v: 'follow', orig: r.design.v, where: r.design.where, note: r.design.note }; r.verdict = { v: 'follows' }; }
        else if (r.design.orig) { r.design = { v: r.design.orig, where: r.design.where, note: r.design.note }; r.verdict = { v: 'not_reviewed' }; }
        save(); render();
      }
    };
  }

  window.ACTAB = { mount: mount };
})();
