import { describe, expect, it } from 'vitest';
import type { ReportCardView } from '#lib/protocol/ReportCardView.ts';
import type { Rpc } from '#lib/rpc.ts';
import { OvernightStore } from './overnight.svelte.ts';

/** Daemon VM palsu dengan antrean di memori. */
class FakeVm implements Rpc {
  calls: [string, unknown][] = [];
  queue = ['a', 'b', 'c'];
  async request<T>(method: string, params?: unknown): Promise<T> {
    this.calls.push([method, params]);
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
            held: false
          })),
          dequeued: []
        } as T;
      case 'overnight.queue.add':
        this.queue.push((params as { record_id: string }).record_id);
        return {} as T;
      case 'overnight.queue.remove':
        this.queue = this.queue.filter((q) => q !== (params as { record_id: string }).record_id);
        return {} as T;
      case 'overnight.queue.reorder':
        this.queue = [...(params as { record_ids: string[] }).record_ids];
        return {} as T;
      case 'overnight.report':
        return { cycle_start_ms: null, used: 0, limit: null, cache_read_tokens: 0, cards: [], dequeued: [], reviewed_without_fix: 0 } as T;
      case 'status':
        return { logins: [{ service: 'gh', state: 'expired', fix: 'Run gh auth login' }] } as T;
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
  entered_waiting_fix: false
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
    const runId = await store.takeover(held, laptop, 'beni@laptop~laptop');
    expect(vm.calls.find(([m]) => m === 'runs.handoff')?.[1]).toEqual({
      record_id: 'h',
      to_daemon_id: 'beni@laptop~laptop'
    });
    expect(laptop.methods()).toEqual(['runs.takeover']);
    expect(runId).toBe('run-laptop');
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
