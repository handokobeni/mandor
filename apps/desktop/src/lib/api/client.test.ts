import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RpcClient, RpcFailure, type ConnectionState, type Transport } from './client';

/** Daemon palsu yang menjawab lewat transport. */
class FakeTransport implements Transport {
  sent: Record<string, unknown>[] = [];
  onLine: (line: string) => void = () => {};
  onClose: () => void = () => {};
  helloError: { code: string; message: string } | null = null;
  autoReply = true;
  opens = 0;

  async token() {
    return { token: 't', protocol: '1.0' };
  }

  async open(onLine: (l: string) => void, onClose: () => void) {
    this.opens += 1;
    this.onLine = onLine;
    this.onClose = onClose;
  }

  async send(line: string) {
    const req = JSON.parse(line);
    this.sent.push(req);
    if (!this.autoReply) return;
    queueMicrotask(() => {
      if (req.method === 'hello') {
        if (this.helloError) {
          this.onLine(JSON.stringify({ jsonrpc: '2.0', id: req.id, error: this.helloError }));
        } else {
          this.onLine(JSON.stringify({ jsonrpc: '2.0', id: req.id, result: { daemon_version: '0.1.0', protocol: '1.0' } }));
        }
      } else if (req.method === 'echo') {
        this.onLine(JSON.stringify({ jsonrpc: '2.0', id: req.id, result: req.params }));
      } else {
        this.onLine(JSON.stringify({ jsonrpc: '2.0', id: req.id, error: { code: 'method_not_found', message: 'x' } }));
      }
    });
  }
}

describe('RpcClient', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('melakukan hello lalu menjalankan request', async () => {
    const t = new FakeTransport();
    const states: ConnectionState[] = [];
    const c = new RpcClient(t, { onState: (s) => states.push(s) });
    await c.connect();
    expect(states.at(-1)).toEqual({ kind: 'ready', daemonVersion: '0.1.0' });
    expect(t.sent[0]).toMatchObject({ method: 'hello', params: { token: 't', protocol: '1.0' } });
    await expect(c.request('echo', { a: 1 })).resolves.toEqual({ a: 1 });
  });

  it('error daemon menjadi RpcFailure dengan kode', async () => {
    const t = new FakeTransport();
    const c = new RpcClient(t);
    await c.connect();
    await expect(c.request('lain')).rejects.toMatchObject({ code: 'method_not_found' });
  });

  it('versi tidak cocok adalah error fatal tanpa reconnect', async () => {
    const t = new FakeTransport();
    t.helloError = { code: 'version_mismatch', message: 'versi 2.0 vs 1.0' };
    const states: ConnectionState[] = [];
    const c = new RpcClient(t, { onState: (s) => states.push(s) });
    await c.connect();
    expect(states.at(-1)).toEqual({ kind: 'error', message: 'versi 2.0 vs 1.0', fatal: true });
    await vi.advanceTimersByTimeAsync(60_000);
    expect(t.opens).toBe(1);
  });

  it('koneksi putus menolak request yang menunggu dan menyambung ulang dengan backoff', async () => {
    const t = new FakeTransport();
    const c = new RpcClient(t, { backoffMs: [100, 1000] });
    await c.connect();
    t.autoReply = false;
    const waiting = c.request('echo', 1);
    t.onClose();
    await expect(waiting).rejects.toBeInstanceOf(RpcFailure);
    await expect(c.request('echo', 1)).rejects.toMatchObject({ code: 'disconnected' });
    t.autoReply = true;
    await vi.advanceTimersByTimeAsync(100);
    expect(t.opens).toBe(2);
    await expect(c.request('echo', 2)).resolves.toBe(2);
  });

  it('request tanpa jawaban berakhir timeout', async () => {
    const t = new FakeTransport();
    const c = new RpcClient(t, { requestTimeoutMs: 500 });
    await c.connect();
    t.autoReply = false;
    const p = c.request('echo', 1);
    vi.advanceTimersByTime(500);
    await expect(p).rejects.toMatchObject({ code: 'timeout' });
  });

  it('notifikasi diteruskan ke pelanggan', async () => {
    const t = new FakeTransport();
    const c = new RpcClient(t);
    await c.connect();
    const seen: unknown[] = [];
    const off = c.on('card.updated', (p) => seen.push(p));
    t.onLine(JSON.stringify({ jsonrpc: '2.0', method: 'card.updated', params: { id: 'r1' } }));
    off();
    t.onLine(JSON.stringify({ jsonrpc: '2.0', method: 'card.updated', params: { id: 'r2' } }));
    expect(seen).toEqual([{ id: 'r1' }]);
  });
});
