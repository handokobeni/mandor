// Navigasi app: papan, workspace run, atau pengaturan.

import { workspace } from '#lib/stores/workspace.svelte.ts';

export type SettingsSection = 'general' | 'lark' | 'repos' | 'workspaces' | 'about';

class Nav {
  page = $state<'board' | 'settings'>('board');
  section = $state<SettingsSection>('lark');
  paletteOpen = $state(false);

  board(): void {
    this.page = 'board';
    workspace.close();
  }

  run(runId: string): void {
    this.page = 'board';
    workspace.open(runId);
  }

  settings(section: SettingsSection = this.section): void {
    this.section = section;
    this.page = 'settings';
  }
}

export const nav = new Nav();
