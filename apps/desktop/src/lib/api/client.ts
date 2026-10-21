// Client JSON-RPC 2.0 ke mandord. Transport disuntikkan agar bisa diuji tanpa Tauri.

import type { Notification } from '#lib/protocol/Notification.ts';
import type { Response } from '#lib/protocol/Response.ts';
import type { RpcError } from '#lib/protocol/RpcError.ts';

export const PROTOCOL_VERSION = '1.0';

/** Saluran baris JSON ke daemon (relay stdio lewat Tauri, atau fake di test). */
export interface Transport {
  token(): Promise<{ token: string; protocol: string }>;
  open(onLine: (line: string) => void, onClose: () => void): Promise<void>;
  send(line: string): Promise<void>;
}

export type ConnectionState =
  | { kind: 'connecting' }
  | { kind: 'ready'; daemonVersion: string }
  | { kind: 'error'; message: string; fatal: boolean };

/** Error dari daemon atau dari koneksi. */
export class RpcFailure extends Error {
  constructor(
    readonly code: RpcError['code'] | 'disconnected' | 'timeout',
    message: string
  ) {
    super(message);
    this.name = 'RpcFailure';
  }
}

interface Pending {
  resolve: (value: unknown) => void;
  reject: (err: RpcFailure) => void;
  timer: ReturnType<typeof setTimeout>;
}

export interface ClientOptions {
  requestTimeoutMs?: number;
  /** Backoff reconnect: [awal, maksimum] dalam ms. */
  backoffMs?: [number, number];
  onState?: (state: ConnectionState) => void;
}

export class RpcClient {
  private nextId = 1;
  private pending = new Map<number, Pending>();
  private listeners = new Map<string, Set<(params: unknown) => void>>();
  private ready = false;
  private stopped = false;
  private attempt = 0;
  private readonly timeoutMs: number;
  private readonly backoff: [number, number];

  constructor(
    private readonly transport: Transport,
    private readonly opts: ClientOptions = {}
  ) {
    this.timeoutMs = opts.requestTimeoutMs ?? 30_000;
    this.backoff = opts.backoffMs ?? [500, 10_000];
  }

  /** Menyambung ke daemon dan melakukan `hello`; menyambung ulang otomatis bila putus. */
  async connect(): Promise<void> {
    this.stopped = false;
    this.setState({ kind: 'connecting' });
    try {
      const { token } = await this.transport.token();
      await this.transport.open(
        (line) => this.onLine(line),
        () => this.onClose()
      );
      const ok = (await this.call('hello', {
        token,
        protocol: PROTOCOL_VERSION,
        client: 'mandor-desktop'
      }, true)) as { daemon_version: string };
      this.ready = true;
      this.attempt = 0;
      this.setState({ kind: 'ready', daemonVersion: ok.daemon_version });
    } catch (err) {
      const failure = toFailure(err);
      const fatal = failure.code === 'version_mismatch';
      this.setState({ kind: 'error', message: failure.message, fatal });
      if (!fatal) this.scheduleReconnect();
    }
  }

  /** Menghentikan reconnect otomatis. */
  stop(): void {
    this.stopped = true;
  }

  /** Memanggil method daemon. */
  request<T = unknown>(method: string, params: unknown = null): Promise<T> {
    if (!this.ready) {
      return Promise.reject(new RpcFailure('disconnected', 'not connected to the daemon'));
    }
    return this.call(method, params, false) as Promise<T>;
  }

  /** Berlangganan notifikasi; mengembalikan fungsi untuk berhenti. */
  on(method: string, handler: (params: unknown) => void): () => void {
    const set = this.listeners.get(method) ?? new Set();
    set.add(handler);
    this.listeners.set(method, set);
    return () => set.delete(handler);
  }

  private call(method: string, params: unknown, allowBeforeReady: boolean): Promise<unknown> {
    if (!allowBeforeReady && !this.ready) {
      return Promise.reject(new RpcFailure('disconnected', 'not connected to the daemon'));
    }
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new RpcFailure('timeout', `daemon did not answer ${method}`));
      }, this.timeoutMs);
      this.pending.set(id, { resolve, reject, timer });
      const line = JSON.stringify({ jsonrpc: '2.0', id, method, params });
      this.transport.send(line).catch((err: unknown) => {
        clearTimeout(timer);
        this.pending.delete(id);
        reject(new RpcFailure('disconnected', String(err)));
      });
    });
  }

  private onLine(line: string): void {
    let msg: Response | Notification;
    try {
      msg = JSON.parse(line) as Response | Notification;
    } catch {
      return;
    }
    if ('id' in msg && typeof msg.id === 'number') {
      const p = this.pending.get(msg.id);
      if (!p) return;
      this.pending.delete(msg.id);
      clearTimeout(p.timer);
      if (msg.error) p.reject(new RpcFailure(msg.error.code, msg.error.message));
      else p.resolve(msg.result ?? null);
      return;
    }
    if ('method' in msg) {
      for (const handler of this.listeners.get(msg.method) ?? []) handler(msg.params);
    }
  }

  private onClose(): void {
    this.ready = false;
    for (const [id, p] of this.pending) {
      clearTimeout(p.timer);
      p.reject(new RpcFailure('disconnected', 'connection to the daemon was lost'));
      this.pending.delete(id);
    }
    if (this.stopped) return;
    this.setState({ kind: 'error', message: 'connection to the daemon was lost', fatal: false });
    this.scheduleReconnect();
  }

  private scheduleReconnect(): void {
    if (this.stopped) return;
    const [base, max] = this.backoff;
    const delay = Math.min(max, base * 2 ** this.attempt);
    this.attempt += 1;
    setTimeout(() => {
      if (!this.stopped) void this.connect();
    }, delay);
  }

  private setState(state: ConnectionState): void {
    this.opts.onState?.(state);
  }
}

function toFailure(err: unknown): RpcFailure {
  if (err instanceof RpcFailure) return err;
  return new RpcFailure('disconnected', err instanceof Error ? err.message : String(err));
}
