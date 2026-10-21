import { describe, expect, it } from 'vitest';
import type { Rpc } from '#lib/rpc.ts';
import { PeopleStore } from './people.svelte.ts';

class FakeRpc implements Rpc {
  calls: string[][] = [];
  async request<T>(_m: string, params?: unknown): Promise<T> {
    const ids = (params as { open_ids: string[] }).open_ids;
    this.calls.push(ids);
    return ids.filter((i) => i === 'ou_a').map((i) => ({ open_id: i, name: 'Beni' })) as T;
  }
  on(): () => void {
    return () => undefined;
  }
}

const tick = () => new Promise((r) => setTimeout(r, 0));

describe('nama orang', () => {
  it('open_id dipakai sampai nama tiba; permintaan digabung dan tidak diulang', async () => {
    const rpc = new FakeRpc();
    const people = new PeopleStore();
    people.attach(rpc);
    expect(people.name('ou_a')).toBe('ou_a');
    people.name('ou_b');
    people.name('ou_a');
    await tick();
    expect(rpc.calls).toEqual([['ou_a', 'ou_b']]);
    expect(people.name('ou_a')).toBe('Beni');
    expect(people.name('ou_b')).toBe('ou_b');
    people.name('ou_b');
    await tick();
    expect(rpc.calls).toHaveLength(1);
  });

  it('nama akun sendiri dari login dipakai tanpa meminta Contact API', async () => {
    const rpc = new FakeRpc();
    const people = new PeopleStore();
    people.attach(rpc);
    people.know('ou_me', 'Beni Handoko');
    expect(people.name('ou_me')).toBe('Beni Handoko');
    await tick();
    expect(rpc.calls).toEqual([]);
  });
});
