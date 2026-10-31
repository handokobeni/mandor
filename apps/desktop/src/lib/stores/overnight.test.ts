import { describe, expect, it } from 'vitest';
import type { ReportCardView } from '#lib/protocol/ReportCardView.ts';
import type { Rpc } from '#lib/rpc.ts';
import { OvernightStore, type QueueStep } from './overnight.svelte.ts';

const PLANNED = { plan: '## Plan', branch: 'mandor/pgs-1', design: [], laptop_design: [] };

/** Daemon VM palsu dengan antrean di memori. */
class FakeVm implements Rpc {
  calls: [string, unknown][] = [];
  queue = ['a', 'b', 'c'];
  /** Kartu Planned yang klaimnya belum pindah. */
  pending = new Set<string>();
  /** Method yang gagal. */
  fail = new Set<string>();
  /** Workspace daemon ini. */
  workspaces: { id: string; name: string; root: string; repo_ids: string[] }[] = [];
  /** Anggota grup per item antrean (kunci = kartu pertama). */
  groups: Record<string, string[]> = {};
  /** Kartu grup yang dilaporkan `runs.prepare_for_vm`. */
  handoverCards: string[] = [];
  /** Hasil `runs.handoff`. */
  handback: unknown = { branch: 'b', repo_id: 'repo1', workspace: null, planned: PLANNED };
  async request<T>(method: string, params?: unknown): Promise<T> {
    this.calls.push([method, params]);
    if (this.fail.has(method)) throw new Error(`${method} refused`);
    switch (method) {
      case 'overnight.queue.list':
        return {
          items: this.queue.map((id) => ({
            record_id: id,
            key: id.toUpperCase(),
            title: id,
            repo_id: 'repo1',
            workspace_id: null,
            note: null,
            start_now: false,
            added_at_ms: 0,
            held: false,
            planned_state: this.pending.has(id) ? 'handover_pending' : null,
            planned_reason: null,
            design_sources: [],
            group: this.groups[id] ?? [],
            group_keys: (this.groups[id] ?? []).map((g) => g.toUpperCase())
          })),
          dequeued: []
        } as T;
      case 'overnight.queue.add': {
        const p = params as { record_id: string; planned?: unknown; group?: string[] };
        const first = p.group?.[0] ?? p.record_id;
        this.queue.push(first);
        if (p.group) this.groups[first] = p.group;
        if (p.planned) this.pending.add(p.record_id);
        return {} as T;
      }
      case 'runs.prepare_for_vm':
        return { planned: PLANNED, repo_id: 'repo1', workspace: null, cards: this.handoverCards } as T;
      case 'runs.handoff':
        return this.handback as T;
      case 'design_sources.list':
        return [{ id: 'ds1', name: 'kelola-mock', kind: 'folder', path: '/d', ready: true, status: 'Ready' }] as T;
      case 'overnight.queue.remove':
        this.queue = this.queue.filter((q) => q !== (params as { record_id: string }).record_id);
        return {} as T;
      case 'overnight.queue.reorder':
        this.queue = [...(params as { record_ids: string[] }).record_ids];
        return {} as T;
      case 'overnight.report':
        return { cycle_start_ms: null, used: 0, limit: null, cache_read_tokens: 0, cards: [], dequeued: [], reviewed_without_fix: 0, planned_held: [] } as T;
      case 'status':
        return {
          daemon_id: 'beni@vm~vm',
          lark: { configured: true, logged_in: true, open_id: 'ou_me', name: null, reauth_required: false },
          logins: [{ service: 'gh', state: 'expired', fix: 'Run gh auth login' }]
        } as T;
      case 'repos.list':
        return [] as T;
      case 'workspaces.list':
        return this.workspaces as T;
      case 'runs.gate_card_decision':
        return { outcome: (params as { decision: string }).decision === 'keep' ? 'kept' : 'reverted' } as T;
      case 'runs.takeover':
        return { id: 'run-laptop' } as T;
      default:
        return {} as T;
    }
  }
  on(): () => void {
    return () => undefined;
  }
  methods(): string[] {
    return this.calls.map(([m]) => m);
  }
}

const held: ReportCardView = {
  record_id: 'h',
  key: 'PGS-8',
  title: 'Held',
  run_id: 'run-vm',
  repo_id: 'repo1',
  outcome: 'held',
  hold_reason: 'stuck',
  hold_note: 'Which endpoint?',
  pr_url: null,
  ac_pass: 0,
  ac_fail: 0,
  ac_cant_verify: 0,
  entered_waiting_fix: false,
  design_notes: [],
  group_card: null,
  changed_in_lark: false
};

async function attached(): Promise<[OvernightStore, FakeVm]> {
  const vm = new FakeVm();
  const store = new OvernightStore();
  store.attach(vm);
  await store.refresh();
  return [store, vm];
}

describe('antrean VM', () => {
  it('tambah dengan target repo, urut ulang, dan keluarkan', async () => {
    const [store, vm] = await attached();
    await store.add('d', { repo_id: 'repo1' });
    expect(vm.calls.find(([m]) => m === 'overnight.queue.add')?.[1]).toEqual({
      record_id: 'd',
      repo_id: 'repo1',
      note: null
    });
    await store.move('d', -1);
    expect(store.queue?.items.map((i) => i.record_id)).toEqual(['a', 'b', 'd', 'c']);
    await store.move('a', -1);
    expect(store.queue?.items[0].record_id, 'tidak bisa naik dari posisi pertama').toBe('a');
    await store.remove('b');
    expect([...(store.queuedIds ?? [])]).toEqual(['a', 'd', 'c']);
  });

  it('Queue together: grup diantre sebagai satu item, semua kartunya dianggap di antrean', async () => {
    const [store, vm] = await attached();
    await store.addGroup(['g1', 'g2', 'g3'], { repo_id: 'repo1' }, 'mulai dari form', ['ds1']);
    expect(vm.calls.find(([m]) => m === 'overnight.queue.add')?.[1]).toEqual({
      record_id: 'g1',
      repo_id: 'repo1',
      note: 'mulai dari form',
      group: ['g1', 'g2', 'g3'],
      design_sources: ['ds1']
    });
    expect(store.queue?.items.map((i) => i.record_id)).toEqual(['a', 'b', 'c', 'g1']);
    expect([...(store.queuedIds ?? [])]).toEqual(['a', 'b', 'c', 'g1', 'g2', 'g3']);
  });

  it('target workspace dikirim sebagai workspace', async () => {
    const [store, vm] = await attached();
    await store.add('w', { workspace: { id: 'kelola', repo_ids: ['api', 'web'] } });
    expect(vm.calls.find(([m]) => m === 'overnight.queue.add')?.[1]).toMatchObject({
      record_id: 'w',
      workspace: { id: 'kelola' }
    });
  });

  it('link VM terputus: antrean tidak diketahui', async () => {
    const [store] = await attached();
    expect(store.queuedIds).not.toBeNull();
    store.detach();
    expect(store.queuedIds).toBeNull();
  });

  it('status login VM dibaca dari link VM', async () => {
    const [store] = await attached();
    expect(store.logins[0]).toEqual({ service: 'gh', state: 'expired', fix: 'Run gh auth login' });
  });
});

describe('aksi kartu VM', () => {
  it('keputusan AC memanggil overnight.gate_decide dengan keputusan dev', async () => {
    const [store, vm] = await attached();
    await store.decide('run-vm', ['ac2'], null);
    expect(vm.calls.find(([m]) => m === 'overnight.gate_decide')?.[1]).toEqual({
      run_id: 'run-vm',
      confirmed: ['ac2']
    });
  });

  it('jawaban kartu tertahan mengantre ulang dengan catatan', async () => {
    const [store, vm] = await attached();
    await store.answer(held, 'Use /api/v2/jobs');
    expect(vm.calls.find(([m]) => m === 'overnight.queue.add')?.[1]).toEqual({
      record_id: 'h',
      repo_id: 'repo1',
      note: 'Use /api/v2/jobs'
    });
  });

  it('Send to review memanggil overnight.send_to_review', async () => {
    const [store, vm] = await attached();
    await store.sendToReview('run-vm');
    expect(vm.calls.find(([m]) => m === 'overnight.send_to_review')?.[1]).toEqual({ run_id: 'run-vm' });
  });

  it('Take over: handoff di VM ke daemon laptop lalu takeover di laptop', async () => {
    const [store, vm] = await attached();
    const laptop = new FakeVm();
    const runId = await store.takeover('h', laptop, 'beni@laptop~laptop');
    expect(vm.calls.find(([m]) => m === 'runs.handoff')?.[1]).toEqual({
      record_id: 'h',
      to_daemon_id: 'beni@laptop~laptop'
    });
    expect(laptop.methods()).toEqual(['runs.takeover']);
    expect(laptop.calls[0][1]).toEqual({ record_id: 'h', repo_id: 'repo1', planned: PLANNED });
    expect(runId).toBe('run-laptop');
  });

  it('Take over draft PR VM: laptop diberi tahu PR masih draft dan hasil review AC', async () => {
    const [store, vm] = await attached();
    const review = { rows: [], snapshot: { repo1: 't1' } };
    vm.handback = { branch: 'b', repo_id: 'repo1', workspace: null, planned: null, draft: true, review };
    const laptop = new FakeVm();
    await store.takeover('h', laptop, 'beni@laptop~laptop');
    expect(laptop.calls.find(([m]) => m === 'runs.takeover')?.[1]).toEqual({
      record_id: 'h',
      repo_id: 'repo1',
      draft: true,
      review
    });
  });

  it('Keep atau Revert kartu grup yang berubah di Lark dikirim ke daemon VM', async () => {
    const [store, vm] = await attached();
    expect(await store.decideCard('run-vm', 'b', 'keep', 'AC lama masih benar')).toBe('kept');
    expect(vm.calls.find(([m]) => m === 'runs.gate_card_decision')?.[1]).toEqual({
      run_id: 'run-vm',
      record_id: 'b',
      decision: 'keep',
      reason: 'AC lama masih benar'
    });
    expect(await store.decideCard('run-vm', 'b', 'revert', null)).toBe('reverted');
    expect(vm.methods().at(-1), 'antrean dan laporan dimuat ulang').not.toBe('runs.gate_card_decision');
  });

  it('Take over grup: kartu grup dari VM diteruskan ke laptop', async () => {
    const [store, vm] = await attached();
    const cards = [
      { record_id: 'h', state: 'done', changed_in_lark: false, kept_reason: null, commits: [], plan: null, fix_requested_by: null, quiet_until_ms: null, in_fix: false, fix_replies: [] },
      { record_id: 'i', state: 'pending', changed_in_lark: false, kept_reason: null, commits: [], plan: null, fix_requested_by: null, quiet_until_ms: null, in_fix: false, fix_replies: [] }
    ];
    vm.handback = { branch: 'b', repo_id: 'repo1', workspace: null, planned: null, draft: true, review: null, cards };
    const laptop = new FakeVm();
    await store.takeover('h', laptop, 'beni@laptop~laptop');
    expect(laptop.calls.find(([m]) => m === 'runs.takeover')?.[1]).toEqual({
      record_id: 'h',
      repo_id: 'repo1',
      draft: true,
      cards
    });
  });

  it('Take over grup satu plan: cuplikan AC plan grup ikut ke laptop', async () => {
    const [store, vm] = await attached();
    const cards = [
      { record_id: 'h', state: 'done', changed_in_lark: false, kept_reason: null, commits: [], plan: null, fix_requested_by: null, quiet_until_ms: null, in_fix: false, fix_replies: [] }
    ];
    const one_plan = [{ card: 'i', ac_id: 'ac2', text: 'AC kartu 2' }];
    vm.handback = { branch: 'b', repo_id: 'repo1', workspace: null, planned: null, draft: false, review: null, cards, one_plan };
    const laptop = new FakeVm();
    await store.takeover('h', laptop, 'beni@laptop~laptop');
    expect(laptop.calls.find(([m]) => m === 'runs.takeover')?.[1]).toEqual({
      record_id: 'h',
      repo_id: 'repo1',
      cards,
      one_plan
    });
  });

  it('Take over workspace: workspace laptop dicari dari repo-nya, id boleh beda dengan VM', async () => {
    const [store, vm] = await attached();
    vm.handback = { branch: 'b', repo_id: null, workspace: { id: 'vm-ws', repo_ids: ['web', 'api'] }, planned: null };
    const laptop = new FakeVm();
    laptop.workspaces = [
      { id: 'lain', name: 'lain', root: '/l', repo_ids: ['web'] },
      { id: 'lap-ws', name: 'lap-ws', root: '/w', repo_ids: ['api', 'web', 'docs'] }
    ];
    await store.takeover('h', laptop, 'beni@laptop~laptop');
    expect(laptop.calls.find(([m]) => m === 'runs.takeover')?.[1]).toEqual({
      record_id: 'h',
      repo_id: 'web',
      workspace: { id: 'lap-ws', repo_ids: ['web', 'api'] }
    });
  });

  it('Take over workspace ditolak sebelum serah-terima bila laptop tak punya workspace yang cocok', async () => {
    const [store, vm] = await attached();
    vm.handback = { branch: 'b', repo_id: null, workspace: { id: 'vm-ws', repo_ids: ['web', 'api'] }, planned: null };
    const laptop = new FakeVm();
    await expect(store.takeover('h', laptop, 'beni@laptop~laptop', { workspace: { id: 'vm-ws', repo_ids: ['web', 'api'] } })).rejects.toThrow(
      'Add a workspace on this laptop with web, api, then take over again.'
    );
    expect(vm.methods()).not.toContain('runs.handoff');
    expect(laptop.methods()).not.toContain('runs.takeover');
  });
});

describe('plan siang, antre ke VM', () => {
  const req = {
    runId: 'run-l',
    recordId: 'p',
    follow: ['ac2'],
    target: { repo_id: 'repo1' },
    designSources: ['ds1'],
    laptopDaemonId: 'beni@laptop~laptop'
  };

  it('memuat sumber desain dan id daemon VM', async () => {
    const [store] = await attached();
    expect(store.designSources.map((d) => d.name)).toEqual(['kelola-mock']);
    expect(store.vmDaemonId).toBe('beni@vm~vm');
  });

  it('tiga langkah: siapkan di laptop, antre di VM, serahkan klaim', async () => {
    const [store, vm] = await attached();
    const laptop = new FakeVm();
    const steps: QueueStep[] = [];
    await store.queuePlanned(laptop, req, (s) => steps.push(s));
    expect(steps).toEqual(['pushing', 'queuing', 'handing_over']);
    expect(laptop.calls.map(([m]) => m)).toEqual(['runs.prepare_for_vm', 'runs.send_to_vm']);
    expect(laptop.calls[0][1]).toEqual({ run_id: 'run-l', follow_design: ['ac2'] });
    expect(laptop.calls[1][1]).toEqual({ run_id: 'run-l', vm_daemon_id: 'beni@vm~vm' });
    expect(vm.calls.find(([m]) => m === 'overnight.queue.add')?.[1]).toEqual({
      record_id: 'p',
      repo_id: 'repo1',
      note: null,
      design_sources: ['ds1'],
      planned: PLANNED,
      origin_daemon: 'beni@laptop~laptop'
    });
  });

  it('grup Planned: VM mengantre semua kartu grup dengan plan kartu pertama', async () => {
    const [store, vm] = await attached();
    const laptop = new FakeVm();
    laptop.handoverCards = ['p', 'q'];
    await store.queuePlanned(laptop, req, () => undefined);
    expect(vm.calls.find(([m]) => m === 'overnight.queue.add')?.[1]).toMatchObject({
      record_id: 'p',
      group: ['p', 'q'],
      planned: PLANNED
    });
  });

  it('VM menolak antrean: klaim tidak diserahkan dan run laptop tetap', async () => {
    const [store, vm] = await attached();
    vm.fail.add('overnight.queue.add');
    const laptop = new FakeVm();
    await expect(store.queuePlanned(laptop, req, () => undefined)).rejects.toThrow('refused');
    expect(laptop.methods()).toEqual(['runs.prepare_for_vm']);
  });

  it('langkah 3 gagal: item tetap di antrean; Retry membaca ulang antrean dulu', async () => {
    const [store, vm] = await attached();
    const laptop = new FakeVm();
    laptop.fail.add('runs.send_to_vm');
    await expect(store.queuePlanned(laptop, req, () => undefined)).rejects.toThrow('refused');
    expect(store.queue?.items.find((i) => i.record_id === 'p')?.planned_state).toBe('handover_pending');

    laptop.fail.clear();
    await store.retryHandover(laptop, 'p', 'run-l');
    expect(laptop.methods().at(-1)).toBe('runs.send_to_vm');

    vm.queue = vm.queue.filter((q) => q !== 'p');
    await expect(store.retryHandover(laptop, 'p', 'run-l')).rejects.toThrow('no longer');
  });

  it('antre tanpa plan membawa sumber desain', async () => {
    const [store, vm] = await attached();
    await store.add('d', { repo_id: 'repo1' }, null, ['ds1']);
    expect(vm.calls.find(([m]) => m === 'overnight.queue.add')?.[1]).toEqual({
      record_id: 'd',
      repo_id: 'repo1',
      note: null,
      design_sources: ['ds1']
    });
  });
});

describe('kartu VM yang terputus', () => {
  it('Queue again mengantre ulang kartu Failed di repo run-nya', async () => {
    const [store, vm] = await attached();
    await store.answer({ ...held, outcome: 'failed', hold_reason: null, hold_note: null }, '');
    expect(vm.calls.find(([m]) => m === 'overnight.queue.add')?.[1]).toEqual({
      record_id: 'h',
      repo_id: 'repo1',
      note: ''
    });
  });
});

/** Daemon VM yang belum siap: belum login Lark, atau masih berperan laptop. */
class NotReadyVm implements Rpc {
  constructor(private readonly vmRole: boolean) {}
  async request<T>(method: string): Promise<T> {
    if (method.startsWith('overnight.')) throw new Error('log in to Lark first');
    if (method === 'status') {
      return {
        lark: { configured: false, logged_in: false, open_id: null, name: null, reauth_required: false },
        ...(this.vmRole ? { logins: [{ service: 'lark', state: 'missing', fix: 'Run mandord login lark' }] } : {})
      } as T;
    }
    return [] as T;
  }
  on(): () => void {
    return () => undefined;
  }
}

describe('VM yang belum siap', () => {
  it('status dan login tetap terbaca walau data lembur butuh login Lark', async () => {
    const store = new OvernightStore();
    store.attach(new NotReadyVm(true));
    await store.refresh();
    expect(store.larkReady).toBe(false);
    expect(store.vmRole).toBe(true);
    expect(store.logins[0].service).toBe('lark');
    expect(store.error).toBeNull();
    expect(store.report).toBeNull();
  });

  it('daemon tanpa status login VM berarti masih berperan laptop', async () => {
    const store = new OvernightStore();
    store.attach(new NotReadyVm(false));
    await store.refresh();
    expect(store.vmRole).toBe(false);
  });
});
