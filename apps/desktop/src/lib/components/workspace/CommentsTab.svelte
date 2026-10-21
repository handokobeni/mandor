<script lang="ts">
  import MessageSquareDashed from '@lucide/svelte/icons/message-square-dashed';
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import ShieldQuestionMark from '@lucide/svelte/icons/shield-question-mark';
  import Clock from '@lucide/svelte/icons/clock';
  import SendHorizontal from '@lucide/svelte/icons/send-horizontal';
  import Timer from '@lucide/svelte/icons/timer';
  import { quietLeft } from '#lib/board.ts';
  import EmptyState from '#lib/components/mandor/EmptyState.svelte';
  import { Badge } from '#lib/components/ui/badge/index.ts';
  import type { CommentView } from '#lib/protocol/CommentView.ts';
  import type { RunView } from '#lib/protocol/RunView.ts';
  import { people } from '#lib/stores/people.svelte.ts';

  interface Props {
    run: RunView;
    comments: CommentView[] | null;
    error: string | null;
    now: number;
  }
  let { run, comments, error, now }: Props = $props();

  const inFix = $derived(run.state.startsWith('wf_'));
  const sorted = $derived([...(comments ?? [])].sort((a, b) => b.create_time - a.create_time));
  // Lark memberi detik epoch; nilai yang sudah milidetik tetap diterima.
  const fmt = (t: number) =>
    new Date(t < 1e12 ? t * 1000 : t).toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
</script>

{#if error}
  <EmptyState icon={MessageSquareDashed} title="Could not load comments">{error}</EmptyState>
{:else if comments === null}
  <EmptyState icon={MessageSquareDashed} title="Loading comments…" />
{:else if sorted.length === 0}
  <EmptyState icon={MessageSquareDashed} title={inFix ? 'No reviewer comments yet' : 'No review comments'}>
    {inFix
      ? 'Mandor checks the Lark record for new comments on every sync.'
      : 'Comments from the Lark record show up here when a reviewer moves the task to Waiting Fix.'}
  </EmptyState>
{:else}
  <div class="comments">
    {#if run.state === 'wf_quiet'}
      <div class="quiet">
        <Timer aria-hidden="true" />
        <div style="flex:1">
          <b>Quiet period</b>
          <div class="muted" style="font-size:12px">
            Waiting for more comments before forwarding trusted ones to the agent.
          </div>
        </div>
        {#if run.quiet_until_ms}<span class="time">{quietLeft(run.quiet_until_ms, now)}</span>{/if}
      </div>
    {/if}
    {#each sorted as c (c.reply_id)}
      <article class="comment">
        <div class="head">
          <b>{people.name(c.author)}</b>
          <span class="time">{fmt(c.create_time)}</span>
          <span class="right">
            <Badge variant="outline" class="trust" data-trusted={c.trusted}>
              {#if c.trusted}<ShieldCheck />Trusted{:else}<ShieldQuestionMark />Not trusted{/if}
            </Badge>
            <Badge variant="secondary">
              {#if c.sent}<SendHorizontal />Sent to agent{:else if c.trusted}<Clock />Not sent yet{:else}<Clock />Only shown to you{/if}
            </Badge>
          </span>
        </div>
        <div style="white-space:pre-wrap">{c.text}</div>
      </article>
    {/each}
  </div>
{/if}
