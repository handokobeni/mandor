<script lang="ts">
  import Bot from '@lucide/svelte/icons/bot';
  import FolderPlus from '@lucide/svelte/icons/folder-plus';
  import Play from '@lucide/svelte/icons/play';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Settings from '@lucide/svelte/icons/settings';
  import SquareDashed from '@lucide/svelte/icons/square-dashed';
  import SquareKanban from '@lucide/svelte/icons/square-kanban';
  import { cardAction, runMeta } from '#lib/board.ts';
  import * as Command from '#lib/components/ui/command/index.ts';
  import type { CardView } from '#lib/protocol/CardView.ts';
  import { board } from '#lib/stores/board.svelte.ts';
  import { nav } from '#lib/stores/nav.svelte.ts';

  interface Props {
    onstart: (card: CardView) => void;
    onrefresh: () => void;
  }
  let { onstart, onrefresh }: Props = $props();

  const startable = $derived(board.cards.filter((c) => cardAction(c) === 'start' && c.start_blocked === null));

  function run(fn: () => void): void {
    nav.paletteOpen = false;
    fn();
  }

  function openCard(c: CardView): void {
    if (c.run) nav.run(c.run.id);
    else {
      nav.board();
      board.query = c.key ?? c.title;
    }
  }

  function onkeydown(e: KeyboardEvent): void {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      nav.paletteOpen = !nav.paletteOpen;
    }
  }
</script>

<svelte:window {onkeydown} />

<Command.Dialog bind:open={nav.paletteOpen} title="Command palette" description="Open a task, start one, or go somewhere">
  <Command.Input placeholder="Type a task key, a title, or a command" />
  <Command.List>
    <Command.Empty>No task or command matches.</Command.Empty>
    <Command.Group heading="Tasks">
      {#each board.cards as c (c.record_id)}
        <Command.Item value="{c.key ?? ''} {c.title}" onSelect={() => run(() => openCard(c))}>
          {#if c.run}<Bot />{:else}<SquareDashed />{/if}
          <span class="mono muted">{c.key ?? ''}</span>
          <span class="truncate">{c.title}</span>
          <span class="muted ml-auto text-xs">{c.run ? runMeta(c.run.state).label : c.status}</span>
        </Command.Item>
      {/each}
    </Command.Group>
    <Command.Separator />
    <Command.Group heading="Actions">
      {#each startable as c (c.record_id)}
        <Command.Item value="start {c.key ?? ''} {c.title}" onSelect={() => run(() => onstart(c))}>
          <Play />Start {c.key ?? c.title}
        </Command.Item>
      {/each}
      <Command.Item value="go to board" onSelect={() => run(() => nav.board())}><SquareKanban />Go to board</Command.Item>
      <Command.Item value="refresh board" onSelect={() => run(onrefresh)}><RefreshCw />Refresh board</Command.Item>
      <Command.Item value="go to settings" onSelect={() => run(() => nav.settings())}><Settings />Go to settings</Command.Item>
      <Command.Item value="add a repo" onSelect={() => run(() => nav.settings('repos'))}><FolderPlus />Add a repo</Command.Item>
    </Command.Group>
  </Command.List>
</Command.Dialog>
