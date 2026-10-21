// Satu client JSON-RPC untuk seluruh app dan status koneksinya.

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
}

export const connection = new Connection();
