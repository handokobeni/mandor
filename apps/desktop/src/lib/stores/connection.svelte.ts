// Client JSON-RPC beserta status koneksinya: satu ke daemon laptop dan satu ke
// daemon VM (U9 plan VM). Store VM terpisah dari store laptop.

import { RpcClient, type ConnectionState, type Transport } from '#lib/api/client.ts';

class Connection {
  state = $state<ConnectionState>({ kind: 'connecting' });
  client: RpcClient | null = null;

  start(transport: Transport): RpcClient {
    this.client?.stop();
    this.client = new RpcClient(transport, { onState: (s) => (this.state = s) });
    void this.client.connect();
    return this.client;
  }

  /** Menghentikan koneksi (VM dilepas). */
  stop(): void {
    this.client?.stop();
    this.client = null;
    this.state = { kind: 'connecting' };
  }
}

export const connection = new Connection();
export const vmConnection = new Connection();
