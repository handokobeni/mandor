// Navigasi app: papan, workspace run, atau pengaturan.

import { workspace } from '#lib/stores/workspace.svelte.ts';

export type SettingsSection =
  | 'general'
  | 'lark'
  | 'repos'
  | 'workspaces'
  | 'design-sources'
  | 'background'
  | 'storage'
  | 'vm'
  | 'about';

class Nav {
  page = $state<'board' | 'settings' | 'overnight'>('board');
  section = $state<SettingsSection>('lark');
  /** Daemon yang diatur di Settings: laptop, atau daemon VM lewat link VM. */
  target = $state<'laptop' | 'vm'>('laptop');
  paletteOpen = $state(false);
  /** Run yang dibuka dari notifikasi kartu: tab Chat di kartu terlama yang menunggu. */
  approvalsFor = $state<string | null>(null);

  board(): void {
    this.page = 'board';
    workspace.close();
  }

  run(runId: string): void {
    this.page = 'board';
    workspace.open(runId);
  }

  /** Workspace run di tab Chat, digulir ke kartu persetujuan terlama (R9, R24). */
  runApprovals(runId: string): void {
    this.approvalsFor = runId;
    this.run(runId);
  }

  /** Tab Overnight: antrean dan laporan daemon VM (U10 plan VM). */
  overnight(): void {
    this.page = 'overnight';
    workspace.close();
  }

  settings(section: SettingsSection = this.section, target: 'laptop' | 'vm' = this.target): void {
    this.section = section;
    this.target = target;
    this.page = 'settings';
  }
}

export const nav = new Nav();
