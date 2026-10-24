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
  import { nav, type SettingsSection } from '#lib/stores/nav.svelte.ts';
  import AboutSettings from './AboutSettings.svelte';
  import BackgroundSettings from './BackgroundSettings.svelte';
  import GeneralSettings from './GeneralSettings.svelte';
  import LarkSettings from './LarkSettings.svelte';
  import RepoSettings from './RepoSettings.svelte';
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
</script>

<div class="settings">
  <nav class="settings-nav" aria-label="Settings">
    {#each SECTIONS as s (s.id)}
      {@const Icon = s.icon}
      <button
        type="button"
        class="nav-item"
        aria-current={nav.section === s.id ? 'page' : undefined}
        onclick={() => nav.settings(s.id)}
      >
        <Icon class="icon" aria-hidden="true" />{s.label}
      </button>
    {/each}
  </nav>
  <div class="settings-main scroll-area">
    {#if nav.section === 'lark'}
      <LarkSettings />
    {:else if nav.section === 'repos'}
      <RepoSettings {rpc} />
    {:else if nav.section === 'workspaces'}
      <WorkspaceSettings {rpc} />
    {:else if nav.section === 'design-sources'}
      <DesignSourceSettings {rpc} />
    {:else if nav.section === 'general'}
      <GeneralSettings {rpc} />
    {:else if nav.section === 'background'}
      <BackgroundSettings />
    {:else if nav.section === 'vm'}
      <VmSettings />
    {:else}
      <AboutSettings {rpc} />
    {/if}
  </div>
</div>
