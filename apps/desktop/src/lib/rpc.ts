// Antarmuka minimal client RPC yang dipakai store (RpcClient atau fake di test).

/** Opsi satu permintaan. */
export interface RequestOptions {
  /** Batas tunggu jawaban (ms); bawaan client dipakai bila tidak diisi. */
  timeoutMs?: number;
}

export interface Rpc {
  request<T = unknown>(method: string, params?: unknown, opts?: RequestOptions): Promise<T>;
  on(method: string, handler: (params: unknown) => void): () => void;
}
