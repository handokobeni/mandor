<script lang="ts">
  import Bolt from '@lucide/svelte/icons/bolt';
  import MoonStar from '@lucide/svelte/icons/moon-star';
  import FolderGit2 from '@lucide/svelte/icons/folder-git-2';
  import Info from '@lucide/svelte/icons/info';
  import Layers from '@lucide/svelte/icons/layers';
  import Frame from '@lucide/svelte/icons/frame';
  import Link from '@lucide/svelte/icons/link';
  import Server from '@lucide/svelte/icons/server';
  import DesignSourceSettings from './DesignSourceSettings.svelte';
  import type { Rpc } from '#lib/rpc.ts';
  import { sectionFor, sectionsFor, type SettingsTarget } from '#lib/settings.ts';
  import { vmConnection } from '#lib/stores/connection.svelte.ts';
  import { vmLark } from '#lib/stores/lark.svelte.ts';
  import { nav, type SettingsSection } from '#lib/stores/nav.svelte.ts';
  import AboutSettings from './AboutSettings.svelte';
  import BackgroundSettings from './BackgroundSettings.svelte';
  import GeneralSettings from './GeneralSettings.svelte';
  import LarkSettings from './LarkSettings.svelte';
  import RepoSettings from './RepoSettings.svelte';
  import VmLarkSettings from './VmLarkSettings.svelte';
  import WorkspaceSettings from './WorkspaceSettings.svelte';
  import VmSettings from './VmSettings.svelte';

  let { rpc }: { rpc: Rpc } = $props();

  const SECTIONS: { id: SettingsSection; label: string; icon: typeof Info }[] = [
    { id: 'lark', label: 'Lark', icon: Link },
    { id: 'repos', label: 'Repos', icon: FolderGit2 },
    { id: 'workspaces', label: 'Workspaces', icon: Layers },
    { id: 'design-sources', label: 'Design sources', icon: Frame },
    { id: 'general', label: 'General', icon: Bolt },
    { id: 'background', label: 'Background work', icon: MoonStar },
    { id: 'vm', label: 'Overnight VM', icon: Server },
    { id: 'about', label: 'About', icon: Info }
  ];

  // Daemon yang diatur: laptop, atau daemon VM saat link VM tersambung.
  let target = $state<SettingsTarget>('laptop');
  const vmRpc = $derived(vmConnection.state.kind === 'ready' ? vmConnection.client : null);
  const active = $derived<SettingsTarget>(target === 'vm' && vmRpc ? 'vm' : 'laptop');
  const visible = $derived(SECTIONS.filter((s) => sectionsFor(active).includes(s.id)));
  const section = $derived(sectionFor(active, nav.section));
</script>

<div class="settings">
  <nav class="settings-nav" aria-label="Settings">
    {#if vmRpc}
      <div class="target" role="radiogroup" aria-label="Daemon to configure">
        <label class="radio-card"><input type="radio" name="s-target" value="laptop" bind:group={target} />Laptop</label>
        <label class="radio-card"><input type="radio" name="s-target" value="vm" bind:group={target} />VM</label>
      </div>
    {/if}
    {#each visible as s (s.id)}
      {@const Icon = s.icon}
      <button
        type="button"
        class="nav-item"
        aria-current={section === s.id ? 'page' : undefined}
        onclick={() => nav.settings(s.id)}
      >
        <Icon class="icon" aria-hidden="true" />{s.label}
      </button>
    {/each}
  </nav>
  <div class="settings-main scroll-area">
    {#if active === 'vm' && vmRpc}
      {#if section === 'repos'}
        <RepoSettings rpc={vmRpc} store={vmLark} />
      {:else if section === 'workspaces'}
        <WorkspaceSettings rpc={vmRpc} store={vmLark} />
      {:else}
        <VmLarkSettings />
      {/if}
    {:else if section === 'lark'}
      <LarkSettings />
    {:else if section === 'repos'}
      <RepoSettings {rpc} />
    {:else if section === 'workspaces'}
      <WorkspaceSettings {rpc} />
    {:else if section === 'design-sources'}
      <DesignSourceSettings {rpc} />
    {:else if section === 'general'}
      <GeneralSettings {rpc} />
    {:else if section === 'background'}
      <BackgroundSettings />
    {:else if section === 'vm'}
      <VmSettings />
    {:else}
      <AboutSettings {rpc} />
    {/if}
  </div>
</div>

<style>
  .target {
    display: flex;
    gap: 6px;
    margin-bottom: 10px;
  }
</style>
