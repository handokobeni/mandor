/* Workspace content fixtures: chat streams, diffs, extra task details.
   Agent text is markdown in production (sanitised with DOMPurify); here
   it is pre-rendered, trusted fixture HTML. */
(function () {
  var M = window.MANDOR;

  var chats = {
    'PGS-1426': [
      { type: 'system', icon: 'git-branch', text: 'Run started on beni/pgs-1426-po-approval in kelola-api and kelola-web (workspace kelola-v2)' },
      { type: 'system', icon: 'frame', text: 'Design source: kelola-design. Planning compared 7 AC with the design: 2 in design, 1 partly, 3 missing, 1 conflict' },
      { type: 'system', icon: 'signpost-big', text: 'AC-5 marked Follow design by Beni Handoko. PM asked to update the AC in Lark' },
      { type: 'system', icon: 'circle-check', text: 'Plan approved. Design gaps written to the task in Lark' },
      { type: 'tools', calls: [
        { tool: 'Read', prev: 'kelola-api/src/po/mod.rs', s: 'ok', out: '186 lines' },
        { tool: 'Edit', prev: 'kelola-api/src/po/policy.rs', s: 'ok', out: '+24 −3' },
        { tool: 'Edit', prev: 'kelola-api/src/audit/mod.rs', s: 'ok', out: '+12 −0' },
        { tool: 'Write', prev: 'kelola-web/src/lib/po/RejectDialog.svelte', s: 'ok', out: '64 lines' },
        { tool: 'Edit', prev: 'kelola-web/src/routes/po/+page.svelte', s: 'ok', out: '+31 −6' },
        { tool: 'Bash', prev: 'cargo test po:: audit::', s: 'ok', out: 'test result: ok. 18 passed; 0 failed' },
        { tool: 'Bash', prev: 'bun run test src/lib/po', s: 'ok', out: ' ✓ RejectDialog.test.ts (4 tests)\n Test Files  1 passed (1)' }
      ] },
      { type: 'agent', html: '<p>Approve and reject are in place across both repos.</p><ul><li><code>kelola-api</code>: approver policy, reject endpoint, audit entries for both actions.</li><li><code>kelola-web</code>: Pending approval filter with count, Approve and Reject on the PO detail, a reject dialog, and the approval history card.</li></ul><p>AC-3 and AC-7 are missing from the design, so I built them from the AC. AC-5 follows the design (Approvals page) as you marked. The Lark message for AC-4 goes through the existing notifier, which is stubbed locally.</p>' },
      { type: 'turn', text: 'Turn finished', dur: '11m 26s', tok: '96.4k in · 12.8k out' }
    ],
    'PGS-1391': [
      { type: 'system', icon: 'git-branch', text: 'Run started on beni/pgs-1391-partial-payments from origin/main' },
      { type: 'system', icon: 'file-text', text: 'Task brief sent: title, description, 4 acceptance criteria (1 dropped)' },
      { type: 'dev', text: 'Use the Timeline component from the leave history page if it fits. Keep amounts formatted with formatRupiah.' },
      { type: 'tools', calls: [
        { tool: 'Read', prev: 'src/routes/invoices/[id]/+page.svelte', s: 'ok', out: '142 lines' },
        { tool: 'Read', prev: 'src/lib/components/Timeline.svelte', s: 'ok', out: '88 lines' },
        { tool: 'Grep', prev: '"formatRupiah" in src/lib', s: 'ok', out: 'src/lib/format.ts:12\nsrc/routes/invoices/+page.svelte:31\nsrc/routes/payroll/summary/+page.svelte:57' },
        { tool: 'Edit', prev: 'src/lib/api/invoice.ts', s: 'ok', out: '+18 −2' },
        { tool: 'Write', prev: 'src/lib/components/invoice/PaymentTimeline.svelte', s: 'ok', out: '48 lines' },
        { tool: 'Bash', prev: 'pnpm test src/lib/api/invoice', s: 'ok', out: ' ✓ invoice.test.ts (6 tests) 412ms\n Test Files  1 passed (1)\n      Tests  6 passed (6)' },
        { tool: 'Bash', prev: 'git push origin beni/pgs-1391-partial-payments', s: 'denied', out: '', reason: '`git push` is run only by the daemon. Use Push & create PR in Mandor.' }
      ] },
      { type: 'agent', html: '<p>The payments timeline is on the invoice detail page, under the totals.</p>' +
        '<ul><li><code>PaymentTimeline.svelte</code> wraps the existing <code>Timeline</code> and renders date, amount, method, recorder, and the balance after each payment.</li>' +
        '<li><code>getInvoicePayments()</code> returns payments newest first and computes the running balance on the server side.</li>' +
        '<li>An invoice with no payments shows <em>No payments recorded yet</em> (AC-5523).</li></ul>' +
        '<p>AC-5524 (inline edit) is dropped in Lark, so I did not build it.</p>' +
        '<p>One thing to confirm: the invoice tests were not running in CI, so I added the package to the test matrix in <code>.github/workflows/ci.yml</code>. Tell me if you want that reverted.</p>' },
      { type: 'turn', text: 'Turn finished', dur: '4m 12s', tok: '38.2k in · 6.1k out' }
    ],
    'PGS-1388': [
      { type: 'system', icon: 'git-branch', text: 'Run started on beni/pgs-1388-overtime-rounding from origin/develop' },
      { type: 'system', icon: 'file-text', text: 'Task brief sent: title, description, 3 acceptance criteria' },
      { type: 'tools', calls: [
        { tool: 'Read', prev: 'src/payroll/overtime.rs', s: 'ok', out: '214 lines' },
        { tool: 'Grep', prev: '"round_half" in src/', s: 'ok', out: 'src/payroll/overtime.rs:88\nsrc/payroll/allowance.rs:41' },
        { tool: 'Bash', prev: 'cargo test overtime', s: 'error', out: 'test overtime::tests::half_hour_rounds_up ... FAILED\nassertion `left == right` failed\n  left: 37_500\n right: 43_750' }
      ] },
      { type: 'agent', html: '<p>Found it: <code>minutes_to_hours</code> truncates 30 minutes to 0 before the 1.5× multiplier. I am switching to integer minutes all the way through and rounding once at the end.</p>' }
    ],
    'PGS-1371': [
      { type: 'system', icon: 'git-pull-request', text: 'PR #407 opened · Lark moved to Waiting Review' },
      { type: 'system', icon: 'arrow-left', text: 'Sari Wulandari moved the task to Waiting Fix in Lark' },
      { type: 'system', icon: 'timer', text: '2 trusted comments received. Waiting for the quiet period before forwarding.' }
    ],
    'PGS-1362': [
      { type: 'system', icon: 'git-pull-request', text: 'PR #399 opened · Lark moved to Waiting Review' },
      { type: 'review', author: 'sari', time: '10:02', trusted: true, text: 'Overlap check ignores night shifts that cross midnight. 22:00 to 06:00 followed by 05:00 to 13:00 is accepted.' },
      { type: 'system', icon: 'wrench', text: 'Fix started automatically after the quiet period' },
      { type: 'tools', calls: [
        { tool: 'Read', prev: 'src/schedule/overlap.rs', s: 'ok', out: '96 lines' },
        { tool: 'Edit', prev: 'src/schedule/overlap.rs', s: 'ok', out: '+14 −5' }
      ], live: true }
    ],
    'PGS-1379': [
      { type: 'system', icon: 'git-branch', text: 'Run started on beni/pgs-1379-payslip-month from origin/develop' },
      { type: 'tools', calls: [
        { tool: 'Read', prev: 'src/payslip/render.rs', s: 'ok', out: '301 lines' },
        { tool: 'Bash', prev: 'cargo add chrono --features unstable-locales', s: 'ok', out: 'Adding chrono v0.4.38 to dependencies' }
      ] },
      { type: 'system', icon: 'circle-x', text: 'Run interrupted: the daemon restarted at 09:58', failed: true },
      { type: 'turn', text: 'Turn failed', dur: '1m 03s', tok: '9.4k in · 0.8k out', failed: true }
    ]
  };

  function genericChat(t) {
    var r = t.run || {};
    var out = [{ type: 'system', icon: 'git-branch', text: 'Run started on ' + (r.branch || 'a new branch') }];
    if (r.fresh) {
      out.push({ type: 'system', icon: 'file-text', text: 'Task brief sent: title, description, acceptance criteria' });
      out.push({ type: 'tools', calls: [{ tool: 'Read', prev: 'CLAUDE.md', s: 'ok', out: '64 lines' }], live: true });
      return out;
    }
    if (r.pr) out.push({ type: 'system', icon: 'git-pull-request', text: 'PR #' + r.pr + ' opened · Lark moved to Waiting Review' });
    if (r.state === 'orphaned') out.push({ type: 'system', icon: 'square-parking', text: 'Run parked: claim lost to Rizky Pratama at 09:12' });
    if (r.state === 'stopped') out.push({ type: 'system', icon: 'circle-pause', text: 'Agent stopped by sync: ' + (t.flag ? M.flags[t.flag.type].label : 'run closed') });
    return out;
  }

  /* Diffs: rows [t, oldLine, newLine, text]; t = h(unk) | c(ontext) | a(dd) | d(el) */
  var diffs = {
    'PGS-1426': [
      { repo: 'kelola-api', s: 'M', path: 'src/po/policy.rs', add: 24, del: 3, rows: [
        ['h', '', '', '@@ -10,9 +10,30 @@ use crate::auth::Role;'],
        ['a', '', 18, 'pub fn can_approve(user: &User) -> bool {'],
        ['a', '', 19, '    user.roles.contains(&Role::Approver)'],
        ['a', '', 20, '}'],
        ['c', 11, 21, '']
      ] },
      { repo: 'kelola-api', s: 'A', path: 'src/po/reject.rs', add: 41, del: 0, rows: [
        ['h', '', '', '@@ -0,0 +1,41 @@'],
        ['a', '', 1, 'pub async fn reject(id: PoId, reason: String, by: &User) -> Result<Po> {'],
        ['a', '', 2, '    ensure!(can_approve(by), Error::Forbidden);'],
        ['a', '', 3, '    let po = repo::set_status(id, Status::Rejected, Some(reason)).await?;']
      ] },
      { repo: 'kelola-api', s: 'M', path: 'src/audit/mod.rs', add: 12, del: 0, rows: [
        ['h', '', '', '@@ -70,6 +70,18 @@ impl AuditLog'],
        ['a', '', 77, '    pub fn po_decision(&self, po: &Po, by: &User) -> Result<()> {']
      ] },
      { repo: 'kelola-web', s: 'M', path: 'src/routes/po/+page.svelte', add: 31, del: 6, rows: [
        ['h', '', '', '@@ -38,8 +38,33 @@'],
        ['a', '', 42, '  <FilterChip value="pending" count={pendingCount}>Pending approval</FilterChip>'],
        ['c', 39, 43, '  <PoTable rows={filtered} />']
      ] },
      { repo: 'kelola-web', s: 'A', path: 'src/lib/po/RejectDialog.svelte', add: 64, del: 0, rows: [
        ['h', '', '', '@@ -0,0 +1,64 @@'],
        ['a', '', 26, '  <Textarea bind:value={reason} placeholder="Why is this PO rejected?" />'],
        ['a', '', 27, '  <Button onclick={confirm}>Confirm</Button>']
      ] },
      { repo: 'kelola-web', s: 'M', path: 'package.json', add: 1, del: 0, rows: [
        ['h', '', '', '@@ -24,6 +24,7 @@ "dependencies": {'],
        ['a', '', 27, '    "svelte-sonner": "^0.3.28",'],
        ['c', 26, 28, '    "zod": "^3.23.8"']
      ] },
      { repo: 'kelola-web', s: 'A', path: 'src/routes/po/[id]/History.svelte', add: 38, del: 0, rows: [
        ['h', '', '', '@@ -0,0 +1,38 @@'],
        ['a', '', 12, '  {#each history as h}<HistoryRow who={h.by} when={h.at} reason={h.reason} />{/each}']
      ] }
    ],
    'PGS-1391': [
      { s: 'M', path: 'src/routes/invoices/[id]/+page.svelte', add: 12, del: 3, rows: [
        ['h', '', '', '@@ -41,9 +41,18 @@ <section class="totals">'],
        ['c', 41, 41, '    <dt>Remaining</dt>'],
        ['c', 42, 42, '    <dd class="num">{formatRupiah(invoice.remaining)}</dd>'],
        ['c', 43, 43, '  </dl>'],
        ['d', 44, '', '  {#if invoice.paid > 0}'],
        ['d', 45, '', '    <p class="muted">Paid {formatRupiah(invoice.paid)}</p>'],
        ['d', 46, '', '  {/if}'],
        ['a', '', 44, '  <h3>Payments</h3>'],
        ['a', '', 45, '  {#await getInvoicePayments(invoice.id)}'],
        ['a', '', 46, '    <Skeleton class="h-24" />'],
        ['a', '', 47, '  {:then payments}'],
        ['a', '', 48, '    {#if payments.length}'],
        ['a', '', 49, '      <PaymentTimeline {payments} total={invoice.total} />'],
        ['a', '', 50, '    {:else}'],
        ['a', '', 51, '      <p class="muted">No payments recorded yet</p>'],
        ['a', '', 52, '    {/if}'],
        ['a', '', 53, '  {/await}'],
        ['a', '', 54, ''],
        ['a', '', 55, ''],
        ['c', 47, 56, '</section>']
      ] },
      { s: 'A', path: 'src/lib/components/invoice/PaymentTimeline.svelte', add: 48, del: 0, rows: [
        ['h', '', '', '@@ -0,0 +1,48 @@'],
        ['a', '', 1, '<script lang="ts">'],
        ['a', '', 2, '  import Timeline from "$lib/components/Timeline.svelte";'],
        ['a', '', 3, '  import { formatRupiah, formatDate } from "$lib/format";'],
        ['a', '', 4, '  import type { Payment } from "$lib/api/invoice";'],
        ['a', '', 5, ''],
        ['a', '', 6, '  let { payments, total }: { payments: Payment[]; total: number } = $props();'],
        ['a', '', 7, '</script>'],
        ['a', '', 8, ''],
        ['a', '', 9, '<Timeline items={payments} let:item>'],
        ['a', '', 10, '  <span class="date">{formatDate(item.paidAt)}</span>'],
        ['a', '', 11, '  <span class="num">{formatRupiah(item.amount)}</span>']
      ] },
      { s: 'M', path: 'src/lib/api/invoice.ts', add: 18, del: 2, rows: [
        ['h', '', '', '@@ -88,6 +88,22 @@ export async function getInvoice(id: string)'],
        ['c', 88, 88, '  return res.json();'],
        ['c', 89, 89, '}'],
        ['a', '', 90, ''],
        ['a', '', 91, 'export async function getInvoicePayments(id: string): Promise<Payment[]> {'],
        ['a', '', 92, '  const res = await api(`/invoices/${id}/payments?order=desc`);'],
        ['a', '', 93, '  return res.json();'],
        ['a', '', 94, '}']
      ] },
      { s: 'A', path: 'src/lib/api/invoice.test.ts', add: 41, del: 0, rows: [
        ['h', '', '', '@@ -0,0 +1,41 @@'],
        ['a', '', 1, 'import { describe, it, expect } from "vitest";'],
        ['a', '', 2, 'import { runningBalance } from "./invoice";'],
        ['a', '', 3, ''],
        ['a', '', 4, 'describe("runningBalance", () => {'],
        ['a', '', 5, '  it("returns the balance after each payment, newest first", () => {']
      ] },
      { s: 'M', path: '.github/workflows/ci.yml', add: 2, del: 1, sensitive: true, rows: [
        ['h', '', '', '@@ -18,7 +18,8 @@ jobs:'],
        ['c', 18, 18, '    strategy:'],
        ['c', 19, 19, '      matrix:'],
        ['d', 20, '', '        package: [payroll, attendance]'],
        ['a', '', 20, '        package: [payroll, attendance, invoice]'],
        ['a', '', 21, '      fail-fast: false'],
        ['c', 21, 22, '    steps:']
      ] }
    ],
    'PGS-1388': [
      { s: 'M', path: 'src/payroll/overtime.rs', add: 9, del: 6, rows: [
        ['h', '', '', '@@ -84,12 +84,15 @@ impl OvertimeCalc {'],
        ['d', 86, '', '    let hours = minutes_to_hours(entry.minutes);'],
        ['d', 87, '', '    let pay = hours * self.hourly_rate * 3 / 2;'],
        ['a', '', 86, '    // Keep minutes until the end; round once (PGS-1388).'],
        ['a', '', 87, '    let pay = entry.minutes as i64 * self.hourly_rate * 3 / (2 * 60);'],
        ['c', 88, 88, '    Rupiah::new(pay)']
      ] }
    ]
  };

  var extraDetails = {
    'PGS-1371': {
      description: 'HR searches employees by nickname and by NIK (16-digit national ID) when people call in. Today only the full legal name matches.',
      ac: [
        { id: 'AC-5390', text: 'Search matches legal name, nickname, and NIK.' },
        { id: 'AC-5391', text: 'Results show which field matched.' },
        { id: 'AC-5392', text: 'Search responds within the existing page load budget.' }
      ], reporter: 'ayu', assignees: ['me']
    },
    'PGS-1388': {
      description: 'Overtime for shifts ending on a half hour is underpaid. 7.5 hours at 1.5× pays as 7 hours.',
      ac: [
        { id: 'AC-5478', text: 'Overtime is computed from minutes, not truncated hours.' },
        { id: 'AC-5479', text: 'A 30-minute overtime block pays 0.75 of the hourly rate.' },
        { id: 'AC-5480', text: 'Existing payslips are not recalculated.' }
      ], reporter: 'ayu', assignees: ['me']
    }
  };

  window.WS = {
    chatFor: function (t) { return JSON.parse(JSON.stringify(chats[t.key] || genericChat(t))); },
    diffFor: function (t) { return JSON.parse(JSON.stringify(diffs[t.key] || [])); },
    detailFor: function (t) { return M.details[t.key] || extraDetails[t.key] || null; }
  };
})();
