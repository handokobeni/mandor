// Skill Claude di mesin daemon untuk saran perintah `/` di chat; dimuat sekali per sambungan.

import type { SkillView } from '#lib/protocol/SkillView.ts';
import type { Rpc } from '#lib/rpc.ts';

class SkillsStore {
  list = $state<SkillView[]>([]);
  private loaded: Rpc | null = null;

  load(rpc: Rpc): void {
    if (this.loaded === rpc) return;
    this.loaded = rpc;
    rpc
      .request<SkillView[]>('agent.skills')
      .then((list) => (this.list = list))
      .catch(() => {
        // Daftar kosong: perintah tetap bisa diketik manual.
        this.loaded = null;
      });
  }
}

export const skills = new SkillsStore();
