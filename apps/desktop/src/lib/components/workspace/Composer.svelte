<script lang="ts">
  import ListPlus from '@lucide/svelte/icons/list-plus';
  import SendHorizontal from '@lucide/svelte/icons/send-horizontal';
  import Square from '@lucide/svelte/icons/square';
  import Zap from '@lucide/svelte/icons/zap';
  import { composerBlocked, isWorking } from '#lib/board.ts';
  import { Button } from '#lib/components/ui/button/index.ts';
  import type { RunView } from '#lib/protocol/RunView.ts';
  import { applyCommand, commandQuery, suggestCommands } from '#lib/commands.ts';
  import { skills } from '#lib/stores/skills.svelte.ts';
  import { workspace } from '#lib/stores/workspace.svelte.ts';

  interface Props {
    run: RunView;
    onsend: (text: string, now: boolean) => void;
    onstop: () => void;
  }
  let { run, onsend, onstop }: Props = $props();

  const working = $derived(isWorking(run.state));
  const blocked = $derived(composerBlocked(run));
  const isMac = typeof navigator !== 'undefined' && navigator.userAgent.includes('Mac');
  let draft = $state('');
  let textarea: HTMLTextAreaElement | undefined = $state();

  // Saran perintah `/skill` saat draft diawali `/` dan belum ada spasi.
  let picked = $state(0);
  let dismissed = $state(false);
  const query = $derived(commandQuery(draft));
  const suggestions = $derived(query === null || dismissed ? [] : suggestCommands(query, skills.list).slice(0, 8));
  $effect(() => {
    void query;
    picked = 0;
  });

  function choose(name: string): void {
    update(applyCommand(name));
    dismissed = false;
    textarea?.focus();
  }

  // Draft per run disimpan di store agar bertahan saat pindah workspace.
  $effect.pre(() => {
    draft = workspace.draft(run.id);
  });

  function update(value: string): void {
    if (!value.startsWith('/')) dismissed = false;
    draft = value;
    workspace.setDraft(run.id, value);
    autosize();
  }

  function autosize(): void {
    if (!textarea) return;
    textarea.style.height = 'auto';
    textarea.style.height = `${Math.min(200, textarea.scrollHeight)}px`;
  }

  function send(now: boolean): void {
    const text = draft.trim();
    if (!text || blocked) return;
    onsend(text, now);
    update('');
  }

  function onkeydown(e: KeyboardEvent): void {
    if (suggestions.length > 0) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        const step = e.key === 'ArrowDown' ? 1 : -1;
        picked = (picked + step + suggestions.length) % suggestions.length;
        return;
      }
      if ((e.key === 'Enter' && !e.metaKey && !e.ctrlKey) || e.key === 'Tab') {
        e.preventDefault();
        choose(suggestions[picked].name);
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        dismissed = true;
        return;
      }
    }
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      send(false);
    }
  }
</script>

<div class="composer-wrap">
  <div class="composer">
    {#if run.queued.length > 0}
      <div class="queued" aria-label="Queued messages">
        {#each run.queued as q, i (i)}
          <span class="q-chip" title={q}><ListPlus class="icon-sm" aria-hidden="true" /><span>{q}</span></span>
        {/each}
      </div>
    {/if}
    <div class="composer-box">
      {#if suggestions.length > 0}
        <div class="cmd-list" role="listbox" aria-label="Skill commands">
          {#each suggestions as s, i (s.name)}
            <button
              type="button"
              role="option"
              class="cmd-row"
              aria-selected={i === picked}
              onmousedown={(e) => e.preventDefault()}
              onclick={() => choose(s.name)}
            >
              <span class="cmd-name">/{s.name}</span>
              <span class="cmd-desc">{s.description}</span>
            </button>
          {/each}
          <div class="cmd-hint">↑↓ choose · Enter or Tab insert · Esc close</div>
        </div>
      {/if}
      <label class="sr-only" for="composer-input">Message the agent</label>
      <textarea
        id="composer-input"
        bind:this={textarea}
        rows="2"
        value={draft}
        disabled={!!blocked}
        oninput={(e) => update(e.currentTarget.value)}
        {onkeydown}
        placeholder={blocked ??
          (working ? 'Queue a message for when this turn ends' : 'Reply to the agent, or type / for a skill. Drafts are kept per task.')}
      ></textarea>
      <div class="composer-bar">
        <span class="hint"><span class="kbd">{isMac ? '⌘' : 'Ctrl'} ↵</span> send</span>
        <span class="right">
          {#if working}
            <Button variant="ghost" size="sm" onclick={onstop}><Square />Stop</Button>
            <Button
              variant="outline"
              size="sm"
              disabled={!draft.trim()}
              title="Stop the current turn, then send"
              onclick={() => send(true)}><Zap />Send now</Button
            >
            <Button variant="secondary" size="sm" disabled={!draft.trim()} onclick={() => send(false)}><ListPlus />Queue</Button>
          {:else}
            <Button variant="secondary" size="sm" disabled={!draft.trim() || !!blocked} onclick={() => send(false)}>
              <SendHorizontal />Send
            </Button>
          {/if}
        </span>
      </div>
    </div>
  </div>
</div>
