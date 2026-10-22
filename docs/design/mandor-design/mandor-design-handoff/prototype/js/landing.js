/* Mandor landing page: playable hero demo + waitlist forms.
   The demo walks one task through the real loop: Start → Working →
   Waiting for you → Push & create PR → Review → Waiting Fix → Fixing →
   back to Review. Other agents keep working on their own. */
(function () {
  function $(s, r) { return (r || document).querySelector(s); }
  function $$(s, r) { return Array.prototype.slice.call((r || document).querySelectorAll(s)); }
  function ic(n, c) { return '<i data-lucide="' + n + '"' + (c ? ' class="' + c + '"' : '') + ' aria-hidden="true"></i>'; }
  function icons() { if (window.lucide) window.lucide.createIcons({ attrs: { 'stroke-width': 1.75 } }); }
  var reduce = window.matchMedia && matchMedia('(prefers-reduced-motion: reduce)').matches;

  /* ---------------- demo state ---------------- */
  var RUN = {
    preparing: ['folder-git-2', 'Preparing', 'working'],
    running: ['loader-circle', 'Working', 'working', true],
    awaiting_dev: ['hand', 'Waiting for you', 'attention'],
    publishing: ['git-pull-request-arrow', 'Creating PR', 'working'],
    in_review_loop: ['git-pull-request', 'Done, waiting for review', 'waiting'],
    wf_quiet: ['timer', 'Quiet period', 'waiting'],
    wf_running: ['wrench', 'Fixing', 'working']
  };
  function badge(state, label) {
    var r = RUN[state];
    return '<span class="run" data-tone="' + r[2] + '">' + ic(r[0], r[3] && !reduce ? 'spin' : '') + '<span class="lbl">' + (label || r[1]) + '</span></span>';
  }
  var others = [
    { key: 'PGS-1405', title: 'Bulk approve leave requests from the team calendar', col: 'todo', blocked: 'not ready, Development Readiness <code>Review AC By PM</code>' },
    { key: 'PGS-1388', title: 'Fix rounding in overtime pay for half-hour shifts', col: 'prog', run: 'running', branch: 'beni/pgs-1388-overtime-rounding' },
    { key: 'PGS-1380', title: 'Payroll summary: group totals by cost center', col: 'review', run: 'in_review_loop', pr: 412, branch: 'beni/pgs-1380-cost-center' }
  ];
  var me = { key: 'PGS-1402', title: 'Invoice list: filter by payment status and date range', col: 'todo', run: null, branch: 'beni/pgs-1402-invoice-filter' };
  var step = 0, timers = [], quiet = 0, panelOpen = false;

  function later(fn, ms) { timers.push(setTimeout(fn, reduce ? Math.min(ms, 300) : ms)); }
  function lark(text) {
    var el = $('#d-lark'); el.textContent = 'Lark · ' + text; el.classList.add('flash');
    setTimeout(function () { el.classList.remove('flash'); }, 900);
  }
  function hint(html) { $('#d-hint').innerHTML = html; icons(); }
  function setStep(n) {
    step = n;
    $$('#d-steps li').forEach(function (li) {
      var s = +li.dataset.s;
      li.setAttribute('data-on', String(s === n)); li.setAttribute('data-done', String(s < n));
    });
  }

  function card(t, mine) {
    var actions = '';
    if (mine && t.col === 'todo') actions = '<button class="btn btn-default btn-xs btn-pulse" id="d-start">' + ic('play', 'icon-sm') + 'Start</button>';
    if (mine && t.run === 'awaiting_dev') actions = '<button class="btn btn-ghost btn-xs" id="d-open">Open' + ic('arrow-right', 'icon-sm') + '</button>';
    var meta = t.run ? '<div class="tc-meta">' + badge(t.run, t.run === 'wf_quiet' ? 'Quiet period · 0:0' + quiet : null) + '</div>' : '';
    var blocked = t.blocked ? '<div class="tc-reason">' + ic('lock') + '<span>Can\'t start: ' + t.blocked + '</span></div>' : '';
    var ready = t.col === 'todo' && !t.blocked ? '<span class="ready" data-ok="true">' + ic('circle-check') + 'Ready To Dev</span>' : '';
    return '<article class="task-card" data-me="' + !!mine + '"' + (t.run === 'awaiting_dev' ? ' data-attention="true"' : '') + '>' +
      '<div class="tc-top"><span class="tc-key">' + t.key + '</span>' + ready + '<span class="right">' + (t.pr ? '<span class="pr-link">' + ic('git-pull-request', 'icon-sm') + '#' + t.pr + '</span>' : '') + '</span></div>' +
      '<div class="tc-title">' + t.title + '</div>' + meta + blocked +
      '<div class="tc-foot">' + (t.run && t.branch ? '<span class="branch">' + ic('git-branch', 'icon-sm') + ' ' + t.branch + '</span>' : '') + '<span class="actions">' + actions + '</span></div></article>';
  }
  function render() {
    var cols = [['todo', 'Todo'], ['prog', 'In Progress'], ['review', 'Review']];
    $('#d-board').innerHTML = cols.map(function (c) {
      var list = others.filter(function (t) { return t.col === c[0]; });
      var mine = me.col === c[0];
      var n = list.length + (mine ? 1 : 0);
      return '<section class="dcol" aria-label="' + c[1] + '"><div class="dcol-h">' + c[1] + '<span class="count">' + n + '</span></div>' +
        (mine && me.run === 'awaiting_dev' ? card(me, true) : '') +
        list.map(function (t) { return card(t); }).join('') +
        (mine && me.run !== 'awaiting_dev' ? card(me, true) : '') + '</section>';
    }).join('');
    // sidebar
    var rows = [];
    if (me.run) rows.push(me);
    rows = rows.concat(others.filter(function (t) { return t.run; }));
    rows.sort(function (a, b) { return (b.run === 'awaiting_dev') - (a.run === 'awaiting_dev'); });
    $('#d-agents').innerHTML = rows.map(function (t) {
      var r = RUN[t.run], att = t.run === 'awaiting_dev';
      var dotc = { working: 'var(--status-working)', attention: 'var(--status-attention)', waiting: 'var(--status-waiting)' }[r[2]];
      return '<div class="agent-row"' + (att ? ' data-attention="true"' : '') + '><span class="dot' + (att && !reduce ? ' pulse' : '') + '" style="--run-c:' + dotc + '"></span>' +
        '<span class="ar-line"><span class="ar-key">' + t.key + '</span><span class="ar-title">' + t.title + '</span></span>' + badge(t.run, t.run === 'wf_quiet' ? 'Quiet period' : null) + '</div>';
    }).join('');
    $('#d-tally').hidden = me.run !== 'awaiting_dev';
    var s = $('#d-start'); if (s) s.onclick = start;
    var o = $('#d-open'); if (o) o.onclick = openPanel;
    icons();
  }

  /* ---------------- the loop ---------------- */
  function start() {
    setStep(1);
    me.col = 'prog'; me.run = 'preparing'; render(); lark('PGS-1402 → In Progress');
    hint(ic('folder-git-2', 'icon-sm') + '<span>Claimed in Lark. Creating the worktree <span class="mono">' + me.branch + '</span> and starting Claude Code.</span>');
    later(function () {
      setStep(2); me.run = 'running'; render();
      hint(ic('loader-circle', 'icon-sm') + '<span>The agent reads the task and acceptance criteria, then works. PGS-1388 keeps going next to it.</span>');
    }, 1600);
    later(function () {
      setStep(3); me.run = 'awaiting_dev'; render();
      hint(ic('hand', 'icon-sm') + '<span><b>Your turn.</b> PGS-1402 is pinned to the top of the sidebar. Press <b>Open</b> on the card.</span>');
    }, 4200);
  }
  function openPanel() {
    panelOpen = true;
    var p = $('#d-panel'); p.hidden = false;
    p.innerHTML = '<div class="dp-head"><span class="mono">PGS-1402</span><b>' + me.title + '</b><button class="btn btn-ghost btn-xs btn-icon" id="d-close" aria-label="Close">' + ic('x', 'icon-sm') + '</button></div>' +
      '<div class="dp-body">' +
        '<details class="toolgroup"><summary>' + ic('chevron-right', 'chev icon-sm') + ic('wrench', 'icon-sm') + '<span>Read 3 files, edited 2 files, ran 1 command</span></summary></details>' +
        '<div class="msg-agent"><div class="msg-label">' + ic('bot', 'icon-sm') + 'Agent</div><p>Added status and date-range filters to the invoice list. The filters live in the URL, so a filtered view can be shared.</p><p><code>listInvoices()</code> now takes <code>status</code>, <code>from</code>, and <code>to</code>. Tests pass.</p></div>' +
        '<div class="msg-turn">' + ic('circle-check', 'icon-sm') + '<span>Turn finished · 3m 48s</span></div>' +
      '</div>' +
      '<div class="dp-foot"><span class="muted">2 files changed on ' + me.branch + '</span><button class="btn btn-default btn-sm btn-pulse" id="d-push">' + ic('git-pull-request-arrow') + 'Push &amp; create PR</button></div>';
    $('#d-close').onclick = closePanel;
    $('#d-push').onclick = push;
    hint(ic('file-diff', 'icon-sm') + '<span>Read the reply, check the diff, then <b>Push &amp; create PR</b>.</span>');
    icons(); $('#d-push').focus({ preventScroll: true });
  }
  function closePanel() { panelOpen = false; $('#d-panel').hidden = true; }
  function push() {
    closePanel(); setStep(4);
    me.run = 'publishing'; render();
    hint(ic('git-commit-horizontal', 'icon-sm') + '<span>Committing and pushing the branch. The agent itself never pushes.</span>');
    later(function () {
      me.col = 'review'; me.run = 'in_review_loop'; me.pr = 418; render(); lark('PGS-1402 → Waiting Review');
      hint(ic('git-pull-request', 'icon-sm') + '<span>PR #418 is open and Lark says Waiting Review. Merging stays with your team.</span>');
    }, 1400);
    later(reviewerAsksForFix, 4600);
  }
  function reviewerAsksForFix() {
    setStep(5);
    me.col = 'prog'; me.run = 'wf_quiet'; quiet = 5; render(); lark('Sari moved PGS-1402 → Waiting Fix');
    hint(ic('message-square-text', 'icon-sm') + '<span>A reviewer asked for a fix in Lark. Mandor waits a quiet period (about 2 min, shortened here) before forwarding trusted comments.</span>');
    (function tick() {
      later(function () {
        quiet--; if (quiet > 0) { render(); tick(); return; }
        me.run = 'wf_running'; render(); lark('PGS-1402 → In Progress');
        hint(ic('wrench', 'icon-sm') + '<span>Trusted comments forwarded. The agent is fixing in the same worktree.</span>');
        later(function () {
          me.col = 'review'; me.run = 'in_review_loop'; render(); lark('PGS-1402 → Waiting Review');
          setStep(6);
          hint(ic('circle-check', 'icon-sm') + '<span>Fix pushed to PR #418 and back in review. You did not open a terminal.</span><button class="btn btn-outline btn-xs" id="d-replay">' + ic('rotate-ccw', 'icon-sm') + 'Replay</button>');
          $('#d-replay').onclick = reset;
        }, 2600);
      }, 1000);
    })();
  }
  function reset() {
    timers.forEach(clearTimeout); timers = [];
    me.col = 'todo'; me.run = null; me.pr = null; closePanel(); setStep(0); render();
    $('#d-lark').textContent = 'Lark · Sprint Tasks synced';
    hint(ic('mouse-pointer-click', 'icon-sm') + '<span>Try it: press <b>Start</b> on PGS-1402.</span>');
  }

  /* ---------------- waitlist forms ---------------- */
  $$('.lp-join').forEach(function (form) {
    form.addEventListener('submit', function (e) {
      e.preventDefault();
      var input = form.querySelector('input[type="email"]'), msg = form.querySelector('.lp-form-msg'), v = input.value.trim();
      if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(v)) {
        input.setAttribute('aria-invalid', 'true'); msg.dataset.s = 'err';
        msg.textContent = v ? 'That email does not look right.' : 'Enter your work email to join.';
        input.focus(); return;
      }
      input.removeAttribute('aria-invalid');
      var btn = form.querySelector('button'); btn.disabled = true; btn.textContent = 'Joining…';
      setTimeout(function () {
        msg.dataset.s = 'ok';
        msg.textContent = 'You are on the list. We will email ' + v + ' when your invite is ready.';
        btn.textContent = 'On the list';
      }, 700);
    });
    form.querySelector('input[type="email"]').addEventListener('input', function (e) { e.target.removeAttribute('aria-invalid'); var m = form.querySelector('.lp-form-msg'); if (m.dataset.s === 'err') { m.textContent = ''; m.dataset.s = ''; } });
  });

  reset();
  icons();
})();
