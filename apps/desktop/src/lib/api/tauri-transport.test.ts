import { beforeEach, describe, expect, it, vi } from 'vitest';

const handlers = new Map<string, (e: { payload: unknown }) => void>();
const invoked: { cmd: string; args: unknown }[] = [];

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string, args?: unknown) => {
    invoked.push({ cmd, args });
    return cmd === 'daemon_token' ? { token: 't', protocol: '1.1' } : undefined;
  })
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, cb: (e: { payload: unknown }) => void) => {
    handlers.set(name, cb);
    return () => handlers.delete(name);
  })
}));
vi.mock('#lib/stores/background.svelte.ts', () => ({
  background: { ensureProvisioned: vi.fn(async () => undefined) }
}));

const { TauriTransport } = await import('./tauri-transport');
const { background } = await import('#lib/stores/background.svelte.ts');

describe('TauriTransport', () => {
  beforeEach(() => {
    handlers.clear();
    invoked.length = 0;
    vi.mocked(background.ensureProvisioned).mockClear();
  });

  it('event laptop dan VM tidak tercampur', async () => {
    const laptop = new TauriTransport('laptop');
    const vm = new TauriTransport('vm');
    const fromLaptop: string[] = [];
    const fromVm: string[] = [];
    await laptop.open((l) => fromLaptop.push(l), () => {});
    await vm.open((l) => fromVm.push(l), () => {});
    handlers.get('daemon://laptop/line')?.({ payload: 'a' });
    handlers.get('daemon://vm/line')?.({ payload: 'b' });
    expect(fromLaptop).toEqual(['a']);
    expect(fromVm).toEqual(['b']);
    await vm.send('{}');
    expect(invoked.at(-1)).toEqual({ cmd: 'daemon_send', args: { key: 'vm', message: '{}' } });
    expect(invoked.find((c) => c.cmd === 'daemon_open' && (c.args as { key: string }).key === 'laptop')).toBeTruthy();
  });

  it('provisioning daemon hanya untuk link laptop', async () => {
    await new TauriTransport('vm').token();
    expect(background.ensureProvisioned).not.toHaveBeenCalled();
    expect(invoked.at(-1)).toEqual({ cmd: 'daemon_token', args: { key: 'vm' } });
    await new TauriTransport().token();
    expect(background.ensureProvisioned).toHaveBeenCalledOnce();
  });
});
