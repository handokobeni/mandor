// Antarmuka minimal client RPC yang dipakai store (RpcClient atau fake di test).

export interface Rpc {
  request<T = unknown>(method: string, params?: unknown): Promise<T>;
  on(method: string, handler: (params: unknown) => void): () => void;
}
