// Navigasi app: papan, workspace run, atau pengaturan.

import { workspace } from '#lib/stores/workspace.svelte.ts';

export type SettingsSection =
  | 'general'
  | 'lark'
  | 'repos'
  | 'workspaces'
  | 'design-sources'
  | 'background'
  | 'vm'
  | 'about';

class Nav {
  page = $state<'board' | 'settings' | 'overnight'>('board');
  section = $state<SettingsSection>('lark');
  /** Daemon yang diatur di Settings: laptop, atau daemon VM lewat link VM. */
  target = $state<'laptop' | 'vm'>('laptop');
  paletteOpen = $state(false);

  board(): void {
    this.page = 'board';
    workspace.close();
  }

  run(runId: string): void {
    this.page = 'board';
    workspace.open(runId);
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
