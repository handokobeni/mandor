<script lang="ts">
  import Bot from '@lucide/svelte/icons/bot';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import Info from '@lucide/svelte/icons/info';
  import MessagesSquare from '@lucide/svelte/icons/messages-square';
  import { isWorking } from '#lib/board.ts';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { type ChatBlock, toBlocks, withLiveRows } from '#lib/chat.ts';
  import EmptyState from '#lib/components/mandor/EmptyState.svelte';
  import { renderMarkdown } from '#lib/markdown.ts';
  import type { RunView } from '#lib/protocol/RunView.ts';
  import { workspace } from '#lib/stores/workspace.svelte.ts';
  import VirtualList from '../VirtualList.svelte';
  import type { QuestionsStore } from '#lib/stores/questions.svelte.ts';
  import QuestionCard from './QuestionCard.svelte';
  import ToolCallGroup from './ToolCallGroup.svelte';

  let { run, questions }: { run: RunView; questions?: QuestionsStore } = $props();

  const working = $derived(isWorking(run.state));
  const blocks = $derived(
    withLiveRows(toBlocks(workspace.messages.get(run.id) ?? []), {
      sending: workspace.sending.get(run.id) ?? null,
      working
    })
  );
  const lastId = $derived(blocks.filter((b) => b.kind !== 'thinking').at(-1)?.id);
</script>

{#snippet block(b: ChatBlock)}
  <div class="chat-row">
    {#if b.kind === 'dev'}
      <div class="msg-dev"><span class="who">You</span>{b.text}</div>
    {:else if b.kind === 'sending'}
      <div class="msg-dev sending"><span class="who">You · Sending…</span>{b.text}</div>
    {:else if b.kind === 'thinking'}
      <div class="msg-system thinking" role="status">
        <LoaderCircle class="icon-sm spin" aria-hidden="true" /><span>Agent is working…</span>
      </div>
    {:else if b.kind === 'system'}
      <div class="msg-system"><Info class="icon-sm" aria-hidden="true" /><span>{b.text}</span></div>
    {:else if b.kind === 'assistant'}
      <div class="msg-agent">
        <div class="msg-label"><Bot class="icon-sm" aria-hidden="true" />Agent</div>
        <!-- Teks agent tidak tepercaya: dirender lewat markdown yang disanitasi. -->
        <div class="md">{@html renderMarkdown(b.text)}</div>
      </div>
    {:else if b.kind === 'tools'}
      <ToolCallGroup calls={b.calls} live={working && b.id === lastId} />
    {:else if b.kind === 'question'}
      {@const q = questions?.byId(b.questionId)}
      {#if q && questions}
        <QuestionCard
          question={q}
          {run}
          submitting={!!questions.submitting[q.id]}
          note={questions.notes[q.id]}
          onanswer={(i) => questions.answer(q.id, i)}
        />
      {/if}
    {:else if b.kind === 'done'}
      <div class="msg-turn" data-failed={b.isError ? 'true' : undefined}>
        {#if b.isError}<CircleX class="icon-sm" aria-hidden="true" />{:else}<CircleCheck class="icon-sm" aria-hidden="true" />{/if}
        <span>{b.isError ? 'Turn finished with an error' : 'Turn finished'}</span>
      </div>
    {:else if b.kind === 'failed'}
      <div class="msg-system" style="color:var(--status-failed)">
        <CircleX class="icon-sm" aria-hidden="true" /><span>{b.text}</span>
      </div>
    {/if}
  </div>
{/snippet}

{#if blocks.length === 0}
  <div class="chat" style="display:flex">
    <EmptyState icon={MessagesSquare} title="No messages yet">
      {working ? 'The agent is reading the task. Its messages show up here.' : 'Messages between you and the agent show up here.'}
    </EmptyState>
  </div>
{:else}
  <VirtualList class="chat" items={blocks} row={block} />
{/if}

<style>
  .sending { opacity: 0.6; }
  .thinking { color: var(--muted-foreground); }
</style>
