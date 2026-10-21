<script lang="ts">
  import Lock from '@lucide/svelte/icons/lock';
  import * as Alert from '#lib/components/ui/alert/index.ts';
  import { Badge } from '#lib/components/ui/badge/index.ts';
  import { renderMarkdown } from '#lib/markdown.ts';
  import type { CardView } from '#lib/protocol/CardView.ts';
  import type { RunView } from '#lib/protocol/RunView.ts';
  import type { TaskDetailsView } from '#lib/protocol/TaskDetailsView.ts';
  import TaskDetails from './TaskDetails.svelte';

  interface Props {
    run: RunView;
    card: CardView | undefined;
    details: TaskDetailsView | null;
    error: string | null;
  }
  let { run, card, details, error }: Props = $props();
</script>

<div class="task-doc">
  <Alert.Root>
    <Lock />
    <Alert.Description>
      Read-only. Lark owns the title, description, acceptance criteria, and assignees. Edit them in Lark.
    </Alert.Description>
  </Alert.Root>
  {#if error}
    <p class="field-error">Could not load the task from Lark: {error}</p>
  {/if}
  <section>
    <h2>Description</h2>
    {#if details === null && !error}
      <div class="skeleton" style="height:12px;width:70%"></div>
    {:else if details?.description.trim()}
      <!-- Isi Lark tidak tepercaya: markdown disanitasi. -->
      <div class="md msg-agent">{@html renderMarkdown(details.description)}</div>
    {:else}
      <p class="muted">No description in Lark.</p>
    {/if}
  </section>
  <section>
    <h2>Acceptance criteria</h2>
    {#if details && details.acs.length > 0}
      <div class="ac-list">
        {#each details.acs as ac (ac.id)}
          <div class="ac" data-dropped={ac.dropped}>
            <span class="id">{ac.label ?? ac.id}</span>
            <span class="txt">
              {ac.text}
              {#if ac.dropped}<Badge variant="secondary" class="ml-1.5">Dropped</Badge>{/if}
            </span>
          </div>
        {/each}
      </div>
    {:else if details}
      <p class="muted">No acceptance criteria linked in the AC table.</p>
    {/if}
  </section>
  <section>
    <h2>Fields</h2>
    <TaskDetails {run} {card} {details} />
  </section>
</div>
