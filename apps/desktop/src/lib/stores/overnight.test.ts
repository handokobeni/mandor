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
            design_sources: []
          })),
          dequeued: []
        } as T;
      case 'overnight.queue.add': {
        const p = params as { record_id: string; planned?: unknown };
        this.queue.push(p.record_id);
        if (p.planned) this.pending.add(p.record_id);
        return {} as T;
      }
      case 'runs.prepare_for_vm':
        return { planned: PLANNED, repo_id: 'repo1', workspace: null } as T;
      case 'runs.handoff':
        return { branch: 'b', repo_id: 'repo1', workspace: null, planned: PLANNED } as T;
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
      case 'workspaces.list':
        return [] as T;
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
  design_notes: []
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
