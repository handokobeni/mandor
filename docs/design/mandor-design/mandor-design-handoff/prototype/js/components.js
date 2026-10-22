/* Mandor component renderers shared by the screens and components.html.
   Each returns an HTML string; data-ui attributes carry the layer names
   used for the shadcn-svelte handoff (TaskCard, ChatMessage, ToolCallGroup). */
(function () {
  var M = window.MANDOR, U = window.UI, esc = U.esc, ic = U.ic;

  /* ---------------- TaskCard ---------------- */
  function cardActions(t) {
    var gate = M.startGate(t);
    var b = [];
    if (t.flag && t.flag.type === 'sync_failed') b.push('<button class="btn btn-outline btn-xs" data-act="retry" data-key="' + t.key + '">' + ic('refresh-cw', 'icon-sm') + 'Retry</button>');
    if (t.flag && (t.flag.type === 'deleted' || t.flag.type === 'unassigned')) b.push('<button class="btn btn-outline btn-xs" data-act="close" data-key="' + t.key + '">Close run</button>');
    if (t.run) b.push('<a class="btn btn-ghost btn-xs" href="workspace.html?task=' + t.key + '" data-ui="Button / variant=ghost / size=xs">Open' + ic('arrow-right', 'icon-sm') + '</a>');
    else if (t.owner && t.owner !== 'me' && !t.ownerActive) b.push('<button class="btn btn-outline btn-xs" data-act="takeover" data-key="' + t.key + '">' + ic('arrow-left-right', 'icon-sm') + 'Take over</button>');
    else if (t.column === 'todo') b.push('<button class="btn btn-default btn-xs" data-act="start" data-key="' + t.key + '"' + (gate ? ' disabled aria-describedby="r-' + t.key + '"' : '') + ' data-ui="Button / variant=default / size=xs">' + ic('play', 'icon-sm') + 'Start</button>');
    return b.join('');
  }
  /* Release 2: AC summary chip and design-gap marker */
  function acChip(t) {
    var a = M.acSummary && M.acSummary(t); if (!a) return '';
    var out = '';
    if (a.reviewing) out = '<span class="ac-chip" data-tone="working" data-ui="Badge / custom=ACSummary / reviewing">' + ic('scan-search') + 'Reviewing AC</span>';
    else if (a.stale) out = '<span class="ac-chip" data-tone="waiting" data-ui="Badge / custom=ACSummary / stale">' + ic('history') + 'Review stale</span>';
    else if (a.fail) out = '<span class="ac-chip" data-tone="failed" data-ui="Badge / custom=ACSummary / failed">' + ic('circle-x') + a.fail + ' failed</span>';
    else if (a.reviewed !== false) out = '<span class="ac-chip" data-tone="' + (a.cant ? 'warning' : 'ok') + '" data-ui="Badge / custom=ACSummary">' + ic(a.cant ? 'circle-help' : 'list-checks') + 'AC ' + a.ok + '/' + a.total + '</span>';
    if (a.gaps && (t.column === 'todo' || t.column === 'inprogress')) out += '<span class="ac-chip" data-tone="muted" title="Planning found AC the design does not show" data-ui="Badge / custom=DesignGaps">' + ic('frame') + a.gaps + ' design gap' + (a.gaps > 1 ? 's' : '') + '</span>';
    return out;
  }
  function card(t) {
    var gate = M.startGate(t);
    var other = t.owner && t.owner !== 'me';
    var readonly = other && t.ownerActive && !t.run;
    var attention = t.run && ['awaiting_dev', 'bs_question', 'plan_ready', 'push_held'].indexOf(t.run.state) > -1;
    var pr = t.run && t.run.pr || t.pr;
    var meta = '';
    if (t.run) meta += U.runBadge(t.run.state);
    if (other) meta += '<span class="tc-owner">' + U.avatar(t.owner) + esc(M.people[t.owner].name) +
      (t.ownerSeen ? ' · ' + (t.ownerActive ? 'active ' : 'inactive ') + esc(t.ownerSeen) : '') + '</span>';
    meta += acChip(t);
    if (readonly) meta += '<span class="badge badge-secondary" data-ui="Badge / variant=secondary">' + ic('eye') + 'Read-only</span>';
    var reason = '';
    if (t.column === 'todo' && gate && !t.run) reason = '<div class="tc-reason" id="r-' + t.key + '">' + ic('lock') + '<span>Can\'t start: ' + esc(gate.replace('task is not ready: ', 'not ready, ')).replace(/&quot;(.+?)&quot;/, '<code>$1</code>') + '</span></div>';
    if (t.column === 'unknown') reason = '<div class="tc-reason">' + ic('circle-help') + '<span>Lark status <code>' + esc(t.status) + '</code> has no column. Change it in Lark to move this card.</span></div>';
    if (t.run && t.run.state === 'orphaned') reason = '<div class="tc-reason">' + ic('info') + '<span>' + esc(t.run.error) + '</span></div>';
    if (t.run && t.run.state === 'failed') reason = '<div class="tc-reason">' + ic('info') + '<span>' + esc(t.run.error) + '</span></div>';
    var ready = (t.column === 'todo' || t.column === 'unknown') ? '<span class="ready" data-ok="' + (M.READY.indexOf(t.readiness) > -1) + '">' + ic(M.READY.indexOf(t.readiness) > -1 ? 'circle-check' : 'circle-dashed') + esc(t.readiness) + '</span>' : '';
    return '<article class="task-card" data-ui="TaskCard" tabindex="-1" draggable="false" data-key="' + t.key + '"' +
      (attention ? ' data-attention="true"' : '') + (readonly ? ' data-readonly="true"' : '') + (t.column === 'done' ? ' data-muted="true"' : '') + '>' +
      '<div class="tc-top"><span class="tc-key">' + t.key + '</span>' + ready +
        '<span class="right">' + (pr ? '<a class="pr-link" href="#" onclick="return false" title="Open PR #' + pr + ' on GitHub">' + ic('git-pull-request', 'icon-sm') + '#' + pr + '</a>' : '') +
        '<button class="btn btn-ghost btn-xs btn-icon" data-menu="' + t.key + '" aria-label="More actions for ' + t.key + '" aria-haspopup="menu">' + ic('ellipsis') + '</button></span></div>' +
      '<div class="tc-title" title="' + esc(t.title) + '">' + esc(t.title) + '</div>' +
      (meta ? '<div class="tc-meta">' + meta + '</div>' : '') +
      (t.flag ? U.flagBadge(t.flag) : '') + reason +
      '<div class="tc-foot">' + (t.run && t.run.branch ? '<span class="branch">' + ic('git-branch', 'icon-sm') + ' ' + esc(t.run.branch) + '</span>' : '') +
        '<span class="actions">' + cardActions(t) + '</span></div>' +
    '</article>';
  }

  /* ---------------- ChatMessage + ToolCallGroup ---------------- */
  function toolIcon(name) { return { Read: 'file-text', Grep: 'text-search', Glob: 'folder-search', Edit: 'file-pen', Write: 'file-plus', Bash: 'square-terminal' }[name] || 'wrench'; }
  function summarize(calls) {
    var c = { read: 0, search: 0, edit: 0, cmd: 0 };
    calls.forEach(function (x) { if (x.tool === 'Read') c.read++; else if (x.tool === 'Grep' || x.tool === 'Glob') c.search++; else if (x.tool === 'Bash') c.cmd++; else c.edit++; });
    var p = [];
    if (c.read) p.push('Read ' + c.read + ' file' + (c.read > 1 ? 's' : ''));
    if (c.search) p.push('searched ' + c.search + ' pattern' + (c.search > 1 ? 's' : ''));
    if (c.edit) p.push('edited ' + c.edit + ' file' + (c.edit > 1 ? 's' : ''));
    if (c.cmd) p.push('ran ' + c.cmd + ' command' + (c.cmd > 1 ? 's' : ''));
    return p.join(', ');
  }
  function toolGroup(m) {
    var denied = m.calls.filter(function (x) { return x.s === 'denied'; }).length, errs = m.calls.filter(function (x) { return x.s === 'error'; }).length;
    return '<details class="toolgroup" data-ui="ToolCallGroup"' + (m.live || denied || errs ? ' open' : '') + '><summary>' + ic('chevron-right', 'chev icon-sm') + ic(m.live ? 'loader-circle' : 'wrench', m.live ? 'spin icon-sm' : 'icon-sm') + '<span>' + summarize(m.calls) + '</span><span class="issues">' +
      (errs ? '<span class="badge" style="color:var(--status-failed)">' + ic('circle-x') + errs + ' failed</span>' : '') +
      (denied ? '<span class="badge" style="color:var(--status-failed)">' + ic('ban') + denied + ' denied</span>' : '') + '</span></summary>' +
      m.calls.map(function (c, i) {
        var live = m.live && i === m.calls.length - 1;
        return '<details class="toolcall" data-s="' + c.s + '"' + (c.s === 'denied' ? ' open' : '') + '><summary>' + ic(live ? 'loader-circle' : toolIcon(c.tool), live ? 'spin icon-sm' : 'icon-sm') + '<span class="tname">' + c.tool + '</span><span class="tprev">' + esc(c.prev) + '</span>' +
          '<span class="tstate">' + (c.s === 'denied' ? ic('ban', 'icon-sm') + 'Denied' : c.s === 'error' ? ic('circle-x', 'icon-sm') + 'Error' : '') + '</span></summary>' +
          (c.s === 'denied' ? '<div class="denial">' + esc(c.reason) + '</div>' : '<pre>' + esc(c.out || (live ? 'running…' : '')) + '</pre>') + '</details>';
      }).join('') + '</details>';
  }
  function msgHtml(m) {
    if (m.type === 'system') return '<div class="msg-system" data-ui="ChatMessage / type=system"' + (m.failed ? ' style="color:var(--status-failed)"' : '') + '>' + ic(m.icon || 'info', 'icon-sm') + '<span>' + esc(m.text) + '</span></div>';
    if (m.type === 'dev') return '<div class="msg-dev" data-ui="ChatMessage / from=developer"><span class="who">You</span>' + esc(m.text) + '</div>';
    if (m.type === 'agent') return '<div class="msg-agent" data-ui="ChatMessage / from=agent"><div class="msg-label">' + ic('bot', 'icon-sm') + 'Agent</div>' + m.html + '</div>';
    if (m.type === 'tools') return toolGroup(m);
    if (m.type === 'turn') return '<div class="msg-turn" data-ui="ChatMessage / type=turn-' + (m.failed ? 'failed' : 'finished') + '"' + (m.failed ? ' data-failed="true"' : '') + '>' + ic(m.failed ? 'circle-x' : 'circle-check', 'icon-sm') + '<span>' + m.text + ' · ' + m.dur + '</span><span class="tok">' + m.tok + '</span></div>';
    if (m.type === 'review') { var p = M.people[m.author]; return '<div class="msg-review" data-ui="ChatMessage / type=reviewer-comment"><div class="head">' + U.avatar(m.author) + '<b>' + esc(p.name) + '</b><span class="muted">Lark comment · ' + esc(m.time) + ' · forwarded to the agent</span><span class="badge trust" data-trusted="' + m.trusted + '" style="margin-left:auto">' + ic(m.trusted ? 'shield-check' : 'shield-question') + (m.trusted ? 'Trusted' : 'Not trusted') + '</span></div>' + esc(m.text) + '</div>'; }
    return '';
  }

  U.taskCard = card;
  U.chatMessage = msgHtml;
  U.toolGroup = toolGroup;
})();
