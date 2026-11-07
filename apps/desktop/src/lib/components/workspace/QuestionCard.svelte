<script lang="ts">
  // Kartu pertanyaan agent (plan 2026-11-07-001, R7–R11, R14). Tidak ada tombol
  // yang fokus otomatis; teks agent ditampilkan polos (tanpa Markdown).
  import MessageCircleQuestion from '@lucide/svelte/icons/message-circle-question';
  import { Button } from '#lib/components/ui/button/index.ts';
  import type { QuestionView } from '#lib/protocol/QuestionView.ts';
  import type { RunView } from '#lib/protocol/RunView.ts';
  import { canPick, chosenIndex, statusLabel } from '#lib/questions.ts';
  import { isWorking } from '#lib/board.ts';

  interface Props {
    question: QuestionView;
    run: RunView;
    submitting: boolean;
    note?: string;
    onanswer: (option: number) => void;
  }
  let { question, run, submitting, note, onanswer }: Props = $props();

  const pending = $derived(question.status === 'pending');
  const enabled = $derived(canPick(question, run, submitting));
  const label = $derived(statusLabel(question));
  const chosen = $derived(chosenIndex(question));
</script>

<article class="question" data-status={question.status} aria-label="Question from the agent">
  <header class="question-head">
    <MessageCircleQuestion class="icon-sm" aria-hidden="true" />
    <span class="title">The agent asks</span>
    <span class="state" role="status" data-pending={pending ? 'true' : undefined}>
      {#if submitting}Sending…{:else if label}{label}{/if}
    </span>
  </header>
  <p class="text">{question.question}</p>
  <div class="options" role="group" aria-label="Answers">
    {#each question.options as option, i (option.label)}
      <Button
        variant={option.recommended && pending ? 'default' : 'outline'}
        size="sm"
        class="option"
        data-chosen={i === chosen ? 'true' : undefined}
        disabled={!enabled}
        onclick={() => onanswer(i)}
      >
        <span class="option-label">
          {option.label}{#if option.recommended}<span class="rec">Recommended</span>{/if}
        </span>
        <span class="option-desc">{option.description}</span>
      </Button>
    {/each}
  </div>
  {#if question.status === 'answered_in_chat' && question.answer}
    <div class="muted small">Your reply: {question.answer}</div>
  {/if}
  {#if note}<div class="note" role="status">{note}</div>{/if}
  {#if pending}
    <div class="muted small">
      {isWorking(run.state) ? 'You can answer when the agent finishes this turn.' : 'Or type your own answer below.'}
    </div>
  {/if}
</article>

<style>
  .question { margin: 8px 0; padding: 10px 12px; border: 1px solid var(--border); border-left: 3px solid var(--status-waiting); border-radius: var(--radius); background: var(--card); display: grid; gap: 8px; }
  .question:not([data-status='pending']) { border-left-color: var(--border); opacity: 0.85; }
  .question-head { display: flex; align-items: center; gap: 6px; font-weight: 500; }
  .state { margin-left: auto; color: var(--muted-foreground); font-size: var(--text-sm); }
  .state[data-pending] { color: var(--status-waiting); }
  .text { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; }
  .options { display: grid; gap: 6px; }
  .options :global(.option) { height: auto; justify-content: flex-start; text-align: left; white-space: normal; display: grid; gap: 2px; padding: 6px 10px; }
  .options :global(.option[data-chosen]) { outline: 2px solid var(--status-done, var(--ring)); }
  .option-label { font-weight: 500; display: flex; gap: 6px; align-items: center; }
  .rec { font-size: var(--text-xs); padding: 0 6px; border-radius: 999px; border: 1px solid currentColor; }
  .option-desc { font-size: var(--text-sm); opacity: 0.85; overflow-wrap: anywhere; }
  .small { font-size: var(--text-sm); }
  .note { font-size: var(--text-sm); color: var(--status-failed); }
</style>
