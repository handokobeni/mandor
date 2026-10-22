/* Release 2 fixtures: AC ledger for PGS-1426 (workspace kelola-v2 =
   kelola-api + kelola-web). Scenarios for every AC review state are built
   from this base with ?ac=<state>. Evidence is plain text only. */
(function () {
  var base = {
    workspace: 'kelola-v2', repos: ['kelola-api', 'kelola-web'], gate: 'block',
    designSource: { name: 'kelola-design', kind: 'repo', location: '/home/beni/work/kelola-design' },
    rows: [
      { id: 'AC-1', text: 'Approvers see a "Pending approval" filter on the purchase order list, with the number of POs waiting.',
        design: { v: 'in', where: 'kelola-design · src/routes/po/+page.svelte (filter bar)' },
        verdict: { v: 'pass', files: [['kelola-web', 'src/routes/po/+page.svelte', 42]], steps: ['Opened /po as an approver', 'Clicked "Pending approval"', 'List showed 3 POs and the count read 3'] } },
      { id: 'AC-2', text: 'Approve and Reject appear only for users with the approver role; other users see the status only.',
        design: { v: 'partly', where: 'kelola-design · src/routes/po/[id]/+page.svelte (shows both buttons, no role rule)' },
        verdict: { v: 'pass', files: [['kelola-api', 'src/po/policy.rs', 18], ['kelola-web', 'src/routes/po/[id]/+page.svelte', 61]], test: 'test po::policy::non_approver_cannot_approve ... ok\ntest po::policy::approver_can_approve ... ok' } },
      { id: 'AC-3', text: 'Rejecting a PO requires a reason of at least 10 characters; the reason is shown to the requester.',
        design: { v: 'missing', note: 'The reject dialog in the design has no reason field.' },
        verdict: { v: 'fail', files: [['kelola-web', 'src/lib/po/RejectDialog.svelte', 27]], steps: ['Opened PO-2291 as an approver', 'Clicked Reject', 'Left the reason empty and pressed Confirm', 'The PO was rejected without a reason'], why: 'The form submits with an empty reason; the API accepts it because kelola-api has no length check on reject.' } },
      { id: 'AC-4', text: 'When a PO is approved, the requester gets a Lark message with the PO number and approver.',
        design: { v: 'missing', note: 'Not a screen; nothing to show in the design.' },
        verdict: { v: 'cant', why: 'Can\'t verify: Lark messages are stubbed in local development, so the reviewer cannot see them.', files: [['kelola-api', 'src/notify/lark.rs', 40]] } },
      { id: 'AC-5', text: 'Approvers can bulk approve up to 20 POs from the list page.',
        design: { v: 'follow', orig: 'conflicts', where: 'kelola-design · src/routes/approvals/+page.svelte', note: 'The design moved bulk approve to a separate Approvals page. Marked Follow design by Beni Handoko; PM asked to update the AC.' },
        verdict: { v: 'follows' } },
      { id: 'AC-6', text: 'The PO detail shows an approval history with who, when, and the reason.',
        design: { v: 'in', where: 'kelola-design · src/routes/po/[id]/+page.svelte (History card)' },
        verdict: { v: 'pass', files: [['kelola-web', 'src/routes/po/[id]/History.svelte', 12]], steps: ['Opened PO-2288', 'History lists "Approved by Sari Wulandari, 21 Oct 09:14"'] } },
      { id: 'AC-7', text: 'Every approve and reject is written to the audit log.',
        design: { v: 'missing', note: 'Not a screen; nothing to show in the design.' },
        verdict: { v: 'pass', files: [['kelola-api', 'src/audit/mod.rs', 77]], test: 'test audit::po_approve_writes_entry ... ok\ntest audit::po_reject_writes_entry ... ok' } }
    ],
    removed: [{ id: 'AC-8', text: 'Approved POs can be exported to CSV.' }],
    history: [
      { at: 'Today 10:42', trigger: 'Manual', browser: true, counts: { pass: 4, fail: 1, cant: 1, follows: 1 } },
      { at: 'Today 09:58', trigger: 'Push & create PR', browser: false, counts: { pass: 3, fail: 2, cant: 1, follows: 1 }, failed: false,
        override: { 'AC-2': { v: 'fail', files: [['kelola-web', 'src/routes/po/[id]/+page.svelte', 58]], why: 'Approve and Reject render for every signed-in user; the page does not check the approver role.' } } }
    ],
    lastReview: { at: 'Today 10:42', trigger: 'Manual', browser: true },
    /* Run app + readiness inputs (section 3.3 / 4.4). Values are fixtures. */
    app: { state: 'running', owner: 'PGS-1426', urls: [['kelola-web', 'http://localhost:5173'], ['kelola-api', 'http://localhost:8080']] },
    browserTesting: true, extension: true, webRepo: 'kelola-web',
    acSyncedAt: '10:31'
  };

  function clone(o) { return JSON.parse(JSON.stringify(o)); }

  /* Outcome of each review in the prototype loop: first review fails AC-3;
     after Send to agent, AC-3 passes and AC-4 still can't be verified. */
  var reviewScript = [
    { 'AC-1': 'pass', 'AC-2': 'pass', 'AC-3': 'fail', 'AC-4': 'cant', 'AC-6': 'pass', 'AC-7': 'pass' },
    { 'AC-1': 'pass', 'AC-2': 'pass', 'AC-3': 'pass', 'AC-4': 'cant', 'AC-6': 'pass', 'AC-7': 'pass' }
  ];
  var fixedAc3 = { v: 'pass', files: [['kelola-web', 'src/lib/po/RejectDialog.svelte', 31], ['kelola-api', 'src/po/reject.rs', 22]], steps: ['Opened PO-2291 as an approver', 'Clicked Reject with an empty reason', 'Confirm stayed disabled; helper text "At least 10 characters"', 'Entered a 24-character reason and confirmed', 'Requester view showed the reason'], test: 'test po::reject::short_reason_is_rejected ... ok' };

  /* Each history entry keeps a snapshot of its verdicts for the read-only view */
  function snapshots(l) {
    l.history.forEach(function (h) {
      if (h.snap) return;
      var snap = {};
      l.rows.forEach(function (r) { snap[r.id] = clone(r.verdict); delete snap[r.id].stale; delete snap[r.id].pending; });
      if (h.override) Object.keys(h.override).forEach(function (k) { snap[k] = h.override[k]; });
      h.snap = snap;
    });
  }

  function scenario(name) {
    var l = clone(base);
    function each(fn) { l.rows.forEach(fn); }
    snapshots(l);
    switch (name) {
      case 'plan':
        l.rows[4].design = { v: 'conflicts', where: 'kelola-design · src/routes/approvals/+page.svelte', note: 'The design moved bulk approve to a separate Approvals page; the AC says the list page.' };
        each(function (r) { r.verdict = { v: 'not_reviewed' }; }); l.history = []; l.lastReview = null; break;
      case 'noac':
        l.rows = []; l.removed = []; l.history = []; l.lastReview = null; break;
      case 'nodesign':
        l.designSource = null; each(function (r) { r.design = null; if (r.verdict.v === 'follows') r.verdict = { v: 'pass', files: [['kelola-web', 'src/routes/po/+page.svelte', 118]], steps: ['Selected 3 POs on /po', 'Clicked Approve selected', 'All 3 moved to Approved'] }; }); break;
      case 'design':
        each(function (r) { if (r.verdict.v !== 'follows') r.verdict = { v: 'not_reviewed' }; }); l.history = []; l.lastReview = null; break;
      case 'notchecked':
        each(function (r) { r.design = { v: 'not_checked' }; r.verdict = { v: 'not_reviewed' }; }); l.history = []; l.lastReview = null; break;
      case 'reviewing':
        each(function (r, i) { if (r.verdict.v !== 'follows' && i > 2) r.verdict = { v: 'not_reviewed', pending: true }; }); l.reviewing = { done: 3, total: 6 }; break;
      case 'failedreview':
      case 'reviewfailed':
        each(function (r, i) { if (r.verdict.v !== 'follows' && i > 2) r.verdict = { v: 'not_reviewed' }; });
        l.reviewFailed = 'The review stopped after 15 minutes (time limit). AC-4, AC-6, and AC-7 were not reviewed.';
        l.history.unshift({ at: 'Today 11:05', trigger: 'Manual', browser: true, counts: { pass: 2, fail: 1 }, failed: true }); break;
      case 'stale':
        each(function (r) { if (r.verdict.v !== 'follows') r.verdict.stale = true; }); l.staleReason = '2 files changed in kelola-web after the review at 10:42.'; break;
      case 'changed':
        l.rows[2].changed = true; l.rows[2].text = 'Rejecting a PO requires a reason of at least 20 characters; the reason is shown to the requester.'; l.rows[2].verdict = { v: 'not_reviewed' };
        l.rows.push({ id: 'AC-9', isNew: true, text: 'Approvers can add an optional note when approving.', design: { v: 'missing', note: 'The approve dialog has no note field.' }, verdict: { v: 'not_reviewed' } }); break;
      case 'allpass':
        l.rows[2].verdict = clone(fixedAc3); l.rows[3].verdict = { v: 'manual', why: 'Checked manually by Beni Handoko: Lark message received in the staging group.' };
        l.lastReview = { at: 'Today 11:20', trigger: 'Manual', browser: true }; break;
      case 'needsconfirm':
        l.rows[2].verdict = clone(fixedAc3); l.lastReview = { at: 'Today 11:20', trigger: 'Manual', browser: false };
        l.rows[3].verdict.why = "Can't verify: browser testing was not used for this review, so the reviewer could not check the screen."; break;
      case 'warn':
        l.gate = 'warn'; break;
    }
    snapshots(l);
    return l;
  }

  window.AC_DATA = { scenario: scenario, reviewScript: reviewScript, fixedAc3: fixedAc3 };
})();
