/* Plan tab (Release 1 as built) with the Release 2 design gaps (section 4.2).
   Rows whose Design value is Missing, Partly, or Conflicts are listed with
   Follow design; undo is possible until the plan is approved. The plan body
   is trusted fixture HTML (agent markdown is sanitised in the app). */
(function () {
  var U = window.UI, esc = U.esc, ic = U.ic, $ = U.$, $$ = U.$$;

  var PLAN_MD = [
    '# Plan: PGS-1426 purchase order approval',
    '',
    '## Scope',
    '- kelola-api: approver policy, reject endpoint with a reason, audit entries, Lark notification on approve.',
    '- kelola-web: Pending approval filter with count, Approve and Reject on the PO detail, reject dialog, approval history card.',
    '',
    '## Steps',
    '1. Add `can_approve` to `src/po/policy.rs` and guard approve and reject (AC-2).',
    '2. Add `reject(id, reason)` with a 10 character minimum; return 422 below it (AC-3).',
    '3. Write audit entries for approve and reject (AC-7).',
    '4. Send the approval message through the existing Lark notifier (AC-4).',
    '5. Web: filter chip with count (AC-1), detail actions by role (AC-2), reject dialog (AC-3), history card (AC-6).',
    '6. Tests: cargo tests for policy, reject, audit; component tests for the reject dialog.',
    '',
    '## Out of scope',
    '- CSV export (AC-8 was removed in Lark).'
  ].join('\n');

  function md(src) {
    /* Minimal renderer for the fixture: headings, lists, inline code */
    var out = '', list = null;
    function inline(s) { return esc(s).replace(/`([^`]+)`/g, '<code>$1</code>'); }
    src.split('\n').forEach(function (ln) {
      var m;
      if (list && !/^(\d+\.|-) /.test(ln)) { out += '</' + list + '>'; list = null; }
      if ((m = ln.match(/^(#{1,3}) (.*)/))) out += '<h' + (m[1].length + 1) + '>' + inline(m[2]) + '</h' + (m[1].length + 1) + '>';
      else if ((m = ln.match(/^- (.*)/))) { if (!list) { list = 'ul'; out += '<ul>'; } out += '<li>' + inline(m[1]) + '</li>'; }
      else if ((m = ln.match(/^\d+\. (.*)/))) { if (!list) { list = 'ol'; out += '<ol>'; } out += '<li>' + inline(m[1]) + '</li>'; }
      else if (ln.trim()) out += '<p>' + inline(ln) + '</p>';
    });
    if (list) out += '</' + list + '>';
    return out;
  }

  var GAP = ['missing', 'partly', 'conflicts'];
  function gaps(l) { return l.rows.filter(function (r) { return r.design && (GAP.indexOf(r.design.v) > -1 || (r.design.v === 'follow' && r.design.orig)); }); }

  function mount(host) {
    /* host: { key, panel, ac (ACTAB instance), runState(), approved(), approvedAt(), onApprove() } */
    function render() {
      var st = host.runState(), panel = host.panel;
      if (['brainstorming', 'bs_question', 'planning'].indexOf(st) > -1) {
        panel.innerHTML = '<div class="empty" data-ui="PlanTab / empty"><span class="ico">' + ic(st === 'planning' ? 'list-tree' : 'lightbulb') + '</span><h3>' + (st === 'planning' ? 'The agent is writing the plan' : st === 'bs_question' ? 'The agent asked a question' : 'The agent is brainstorming') + '</h3><p>' +
          (st === 'bs_question' ? 'Answer in Chat, or press Continue to plan. The plan shows up here when it is ready.' : 'The plan shows up here when it is ready, with the AC the design does not cover.') + '</p></div>';
        U.icons(); return;
      }
      var l = host.ac.ledger(), approved = host.approved(), list = gaps(l);
      var noSource = !l.designSource;
      var gapHtml = noSource ? '<div class="alert alert-info">' + ic('frame') + '<div><div class="alert-title">No design source for this run</div><div class="muted">Planning did not compare the AC with a design.</div></div></div>'
        : '<section class="plan-gaps" data-ui="PlanTab / DesignGaps"><div class="pg-head"><h2>' + ic('frame', 'icon-sm') + 'Design gaps <span class="muted">(' + list.length + ' of ' + l.rows.length + ' AC)</span></h2>' +
          '<span class="muted">Compared with <b>' + esc(l.designSource.name) + '</b>. By default the agent builds every AC; the design only sets how it looks.</span></div>' +
          (list.length ? '<div class="pg-list">' + list.map(function (r) {
            var followed = r.design.v === 'follow', orig = followed ? r.design.orig : r.design.v;
            return '<div class="pg-row" data-id="' + esc(r.id) + '"' + (followed ? ' data-follow="true"' : '') + '>' +
              '<span class="mono pg-id">' + esc(r.id) + '</span>' +
              '<div class="pg-txt"><div>' + esc(r.text) + '</div><div class="pg-meta">' + U.ac.chip('design', orig) + (r.design.note ? '<span class="muted">' + esc(r.design.note) + '</span>' : '') + '</div>' +
                (followed ? '<div class="pg-follow">' + ic('signpost-big', 'icon-sm') + '<span>The agent follows the design for ' + esc(r.id) + '. This AC leaves the gate, and the PM is asked to update it in Lark.</span></div>' : '') + '</div>' +
              '<div class="pg-act">' + (approved
                ? (followed ? U.ac.chip('design', 'follow') : '<span class="muted pg-default">Build from the AC</span>')
                : followed ? '<button class="btn btn-ghost btn-sm" data-undo="' + esc(r.id) + '">' + ic('undo-2') + 'Undo</button>'
                : '<button class="btn btn-outline btn-sm" data-follow="' + esc(r.id) + '" title="Use when the design is newer and this AC is outdated">' + ic('signpost-big') + 'Follow design</button>') + '</div></div>';
          }).join('') + '</div>' : '<p class="muted" style="padding:10px 12px">Every AC is shown in the design.</p>') +
          (approved ? '<div class="pg-foot">' + ic('circle-check', 'icon-sm') + 'Plan approved ' + esc(host.approvedAt()) + '. Design gaps were written to ' + esc(host.key) + ' in Lark (see Task).</div>'
            : '<div class="pg-foot">' + ic('info', 'icon-sm') + 'Missing and Partly rows are informational: the agent still builds them. Follow design changes that, until you approve the plan.</div>') +
          '</section>';
      panel.innerHTML = '<div class="plan-doc" data-ui="PlanTab">' +
        (approved ? '' : '<div class="alert alert-info plan-ready">' + ic('file-check') + '<div><div class="alert-title">Plan ready</div><div class="muted">Review the gaps and the plan, then approve to start implementing. Push &amp; create PR stays unavailable until then.</div></div><div class="alert-actions"><button class="btn btn-outline btn-sm" id="pl-approve">' + ic('play') + 'Approve plan &amp; implement</button><button class="btn btn-outline btn-sm" id="pl-queue-vm" title="Approve the plan and let the VM implement it overnight">' + ic('moon-star') + 'Approve &amp; queue for VM</button></div></div>') +
        gapHtml +
        '<section class="plan-md"><div class="pm-head"><span class="mono muted">docs/plans/pgs-1426-po-approval.md</span><button class="btn btn-ghost btn-sm" id="pl-copy">' + ic('copy') + 'Copy Markdown</button></div><div class="md">' + md(PLAN_MD) + '</div></section></div>';
      U.icons();
      $$('[data-follow]', panel).forEach(function (b) { b.onclick = function () { host.ac.setFollow(b.dataset.follow, true); render(); U.toast(b.dataset.follow + ' will follow the design', 'Undo is possible until you approve the plan', 'signpost-big'); }; });
      $$('[data-undo]', panel).forEach(function (b) { b.onclick = function () { host.ac.setFollow(b.dataset.undo, false); render(); }; });
      var c = $('#pl-copy', panel); if (c) c.onclick = function () { if (navigator.clipboard) navigator.clipboard.writeText(PLAN_MD); U.toast('Copied the plan as Markdown', null, 'copy'); };
      var a = $('#pl-approve', panel); if (a) a.onclick = host.onApprove;
      var q = $('#pl-queue-vm', panel); if (q) q.onclick = function () { U.toast('Queued for the VM', 'The VM implements the approved plan tonight', 'moon-star'); };
    }
    return { render: render, gaps: function () { return gaps(host.ac.ledger()); } };
  }

  window.PLAN = { mount: mount, gaps: gaps, md: md, PLAN_MD: PLAN_MD };
})();
