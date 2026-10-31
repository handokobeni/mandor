// Daemon VM tiruan untuk meninjau tab Overnight di browser (`/?mock`).
// Hanya dimuat lewat dynamic import di mode dev; tidak masuk build produksi.

import type { Transport } from '#lib/api/client.ts';
import type { OvernightSettingsView } from '#lib/protocol/OvernightSettingsView.ts';
import type { QueueItemView } from '#lib/protocol/QueueItemView.ts';
import type { ReportCardView } from '#lib/protocol/ReportCardView.ts';

const HOUR = 3_600_000;

let settings: OvernightSettingsView | null = { start: '19:00', end: '06:00', offset: '+07:00', token_limit: 2_000_000 };
let queue: QueueItemView[] = [
  {
    record_id: 'recA',
    key: 'PGS-1410',
    title: 'Bulk approve overtime requests',
    repo_id: 'pegasus-web',
    workspace_id: null,
    note: null,
    start_now: false,
    added_at_ms: Date.now() - 2 * HOUR,
    held: false,
    planned_state: null,
    planned_reason: null,
    design_sources: []
  },
  {
    record_id: 'recP',
    key: 'PGS-1426',
    title: 'PO approval with reasons',
    repo_id: 'pegasus-web',
    workspace_id: null,
    note: null,
    start_now: false,
    added_at_ms: Date.now() - HOUR,
    held: false,
    planned_state: 'waiting',
    planned_reason: null,
    design_sources: ['kelola-mock']
  },
  {
    record_id: 'recQ',
    key: 'PGS-1428',
    title: 'Reject with a reason',
    repo_id: 'pegasus-web',
    workspace_id: null,
    note: null,
    start_now: false,
    added_at_ms: Date.now() - HOUR / 2,
    held: false,
    planned_state: 'handover_pending',
    planned_reason: null,
    design_sources: []
  }
];
const designSources = [{ id: 'ds1', name: 'kelola-mock', kind: 'folder', path: '/srv/design/kelola-mock', ready: true, status: 'Ready' }];

function rc(over: Partial<ReportCardView>): ReportCardView {
  return {
    record_id: 'rec',
    key: 'PGS-0',
    title: '',
    run_id: 'vm-run',
    repo_id: 'pegasus-web',
    outcome: 'working',
    hold_reason: null,
    hold_note: null,
    pr_url: null,
    ac_pass: 0,
    ac_fail: 0,
    ac_cant_verify: 0,
    entered_waiting_fix: false,
    design_notes: [],
    ...over
  };
}

let cards: ReportCardView[] = [
  rc({
    record_id: 'recV1',
    key: 'PGS-1421',
    title: 'Export leave balance to CSV',
    run_id: 'vm1',
    outcome: 'draft',
    pr_url: 'https://github.com/talentlytica/pegasus-web/pull/412',
    ac_pass: 4
  }),
  rc({
    record_id: 'recV2',
    key: 'PGS-1422',
    title: 'Filter attendance by department',
    run_id: 'vm2',
    outcome: 'held',
    hold_reason: 'ac_gate',
    hold_note: '1 AC could not be checked headlessly',
    ac_pass: 3,
    ac_cant_verify: 1
  }),
  rc({
    record_id: 'recV3',
    key: 'PGS-1423',
    title: 'Reminder email for pending approvals',
    run_id: 'vm3',
    outcome: 'held',
    hold_reason: 'stuck',
    hold_note: 'Should the reminder go to the approver or to the requester as well?'
  }),
  rc({
    record_id: 'recV4',
    key: 'PGS-1424',
    title: 'Hide salary column for viewers',
    run_id: 'vm4',
    outcome: 'sent_to_review',
    pr_url: 'https://github.com/talentlytica/pegasus-web/pull/409',
    ac_pass: 5
  })
];

function handle(method: string, p: Record<string, unknown>): unknown {
  switch (method) {
    case 'hello':
      return { daemon_version: '0.1.0-vm', protocol: '1.1' };
    case 'design_sources.list':
      return designSources;
    case 'status':
      return {
        daemon_id: 'beni@vm~vm',
        version: '0.1.0-vm',
        uptime_ms: 0,
        repos: 1,
        lark: { configured: true, logged_in: true, open_id: 'ou_7f3a91', name: 'Beni', reauth_required: false },
        logins: [
          { service: 'lark', state: 'ok', fix: null },
          { service: 'gh', state: 'expired', fix: 'The GitHub token is no longer valid. Run `gh auth login` as your user on the VM.' },
          { service: 'claude', state: 'ok', fix: null }
        ]
      };
    case 'repos.list':
      return [
        {
          id: 'pegasus-web',
          path: '/srv/mandor/beni/workspaces/pegasus-web',
          default_branch: 'main',
          gh_profile: null,
          companions: [],
          trusted_reviewers: [],
          extra_allowed: ['Bash(pnpm test *)'],
          gate_mode: null,
          app: {
            test_mode: 'web',
            setup_command: 'pnpm install && pnpm seed:test-login',
            start_command: 'pnpm dev --port 5150',
            port: 5150,
            app_url: null,
            env_keys: [],
            browser_tests: true,
            auto_restart: false,
            unattended_login: true
          }
        }
      ];
    case 'lark.settings.get':
      return {
        app_id: 'cli_a6f2c81d93b8d00e',
        has_secret: true,
        app_token: 'NkQ2bXr7YcPq1sZt9vLmHg4eAbc',
        table_id: 'tbl8kX2pQm4vR7aW',
        ac_table_id: 'tblD3nF6hJ9kL2pS',
        redirect_port: 5199,
        open_base: 'https://open.larksuite.com',
        accounts_base: 'https://accounts.larksuite.com',
        quiet_secs: 120
      };
    case 'notifications.take':
      return [];
    case 'workspaces.list':
      return [];
    case 'overnight.queue.list':
      return {
        items: queue,
        dequeued: [
          { record_id: 'recX', key: 'PGS-1399', title: 'x', reason: 'the card moved to In Progress in Lark', at_ms: Date.now() - HOUR }
        ]
      };
    case 'overnight.queue.add': {
      const id = String(p.record_id);
      queue = [
        ...queue,
        {
          record_id: id,
          key: id,
          title: 'Queued card',
          repo_id: (p.repo_id as string) ?? null,
          workspace_id: null,
          note: (p.note as string) ?? null,
          start_now: false,
          added_at_ms: Date.now(),
          held: cards.some((c) => c.record_id === id),
          planned_state: p.planned ? 'handover_pending' : null,
          planned_reason: null,
          design_sources: (p.design_sources as string[] | undefined) ?? []
        }
      ];
      return {};
    }
    case 'overnight.queue.remove':
      queue = queue.filter((q) => q.record_id !== p.record_id);
      return {};
    case 'overnight.queue.reorder': {
      const ids = p.record_ids as string[];
      queue = ids.map((id) => queue.find((q) => q.record_id === id)).filter((q): q is QueueItemView => !!q);
      return {};
    }
    case 'overnight.queue.start_now':
      queue = queue.map((q) => (q.record_id === p.record_id ? { ...q, start_now: true } : q));
      return {};
    case 'overnight.settings.get':
      return settings;
    case 'overnight.settings.set':
      settings = p as unknown as OvernightSettingsView;
      return {};
    case 'overnight.report':
      return {
        cycle_start_ms: Date.now() - 14 * HOUR,
        used: 640_000,
        limit: settings?.token_limit ?? null,
        cache_read_tokens: 3_100_000,
        cards,
        dequeued: [],
        reviewed_without_fix: cards.filter((c) => c.outcome === 'sent_to_review' && !c.entered_waiting_fix).length,
        planned_held: [
          { record_id: 'recH', key: 'PGS-1429', title: 'Audit entries for approvals', repo_id: 'pegasus-web', reason: 'Design source "kelola-mock" is not available: Path missing.' }
        ]
      };
    case 'overnight.send_to_review':
      cards = cards.map((c) => (c.run_id === p.run_id ? { ...c, outcome: 'sent_to_review' } : c));
      return {};
    case 'overnight.gate_decide':
      cards = cards.map((c) =>
        c.run_id === p.run_id ? { ...c, outcome: 'draft', hold_reason: null, hold_note: null } : c
      );
      return {};
    case 'ledger.get':
      return {
        run_id: p.run_id,
        rows: [
          {
            ac_id: 'ac9',
            label: 'AC-9',
            text: 'The filter keeps its value after reload',
            verdict: 'cant_verify'
          }
        ],
        synced_at_ms: null,
        sync_error: null,
        design_sources: []
      };
    case 'runs.handoff':
      cards = cards.filter((c) => c.record_id !== p.record_id);
      queue = queue.filter((q) => q.record_id !== p.record_id);
      return { branch: 'beni/pgs-1423', repo_id: 'pegasus-web', workspace: null, planned: null };
    default:
      return {};
  }
}

export class MockVmTransport implements Transport {
  private onLine: ((line: string) => void) | null = null;

  async token(): Promise<{ token: string; protocol: string }> {
    return { token: 'mock-vm', protocol: '1.1' };
  }

  async open(onLine: (line: string) => void): Promise<void> {
    this.onLine = onLine;
  }

  async send(line: string): Promise<void> {
    const req = JSON.parse(line) as { id: number; method: string; params?: Record<string, unknown> };
    const result = handle(req.method, req.params ?? {});
    setTimeout(() => this.onLine?.(JSON.stringify({ jsonrpc: '2.0', id: req.id, result })), 100);
  }
}
