<script lang="ts">
  import { acChip, runLabel } from '#lib/board.ts';
  import ListChecks from '@lucide/svelte/icons/list-checks';
  import Layers from '@lucide/svelte/icons/layers';
  import { progress, progressText } from '#lib/group.ts';
  import Palette from '@lucide/svelte/icons/palette';
  import ArrowLeftRight from '@lucide/svelte/icons/arrow-left-right';
  import ArrowRight from '@lucide/svelte/icons/arrow-right';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import CircleDashed from '@lucide/svelte/icons/circle-dashed';
  import CircleQuestionMark from '@lucide/svelte/icons/circle-question-mark';
  import Copy from '@lucide/svelte/icons/copy';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import Eye from '@lucide/svelte/icons/eye';
  import GitBranch from '@lucide/svelte/icons/git-branch';
  import GitPullRequest from '@lucide/svelte/icons/git-pull-request';
  import Info from '@lucide/svelte/icons/info';
  import Lock from '@lucide/svelte/icons/lock';
  import PanelRightOpen from '@lucide/svelte/icons/panel-right-open';
  import Play from '@lucide/svelte/icons/play';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import UserRound from '@lucide/svelte/icons/user-round';
  import { prLinks } from '#lib/workspaces.ts';
  import { assigneeLabel, cardAction, cardReason, flagMeta, isReadOnly, isReady, ownerLabel } from '#lib/board.ts';
  import { Badge } from '#lib/components/ui/badge/index.ts';
  import { Button } from '#lib/components/ui/button/index.ts';
  import * as DropdownMenu from '#lib/components/ui/dropdown-menu/index.ts';
  import type { CardView } from '#lib/protocol/CardView.ts';
  import { people } from '#lib/stores/people.svelte.ts';
  import CardFlagBadge from './CardFlagBadge.svelte';
  import RunStatusBadge from './RunStatusBadge.svelte';
  import { copyText } from './copy.ts';

  interface Props {
    card: CardView;
    onstart: (card: CardView) => void;
    onopen: (card: CardView) => void;
    ontakeover: (card: CardView) => void;
    onretry: (card: CardView) => void;
    onclose: (card: CardView) => void;
  }
  let { card, onstart, onopen, ontakeover, onretry, onclose }: Props = $props();

  const key = $derived(card.key ?? card.record_id);
  const action = $derived(cardAction(card));
  const reason = $derived(cardReason(card));
  const readOnly = $derived(isReadOnly(card));
  const owner = $derived(ownerLabel(card, (id) => people.name(id)));
  const assignees = $derived(assigneeLabel(card, (id) => people.name(id)));
  const flagAction = $derived(card.flag ? flagMeta(card.flag.kind).action : null);
  const prUrl = $derived(card.run?.pr_url ?? card.pr_url);
  const prs = $derived(prLinks(prUrl));
  const branch = $derived(card.run?.branch ?? card.branch);
  const showReady = $derived((card.column === 'todo' || card.column === 'unknown') && !!card.readiness);
  const ready = $derived(isReady(card));
  const ReasonIcon = $derived(
    card.column === 'unknown' ? CircleQuestionMark : card.column === 'todo' && !card.run ? Lock : Info
  );
</script>

<article
  class="task-card"
  draggable="false"
  data-key={key}
  data-attention={card.run?.state === 'awaiting_dev' ? 'true' : undefined}
  data-readonly={readOnly ? 'true' : undefined}
  data-muted={card.column === 'done' ? 'true' : undefined}
  aria-label="{key}: {card.title}"
>
  <div class="tc-top">
    <span class="tc-key">{key}</span>
    {#if showReady}
      <span class="ready" data-ok={ready}>
        {#if ready}<CircleCheck aria-hidden="true" />{:else}<CircleDashed aria-hidden="true" />{/if}
        {card.readiness}
      </span>
    {/if}
    <span class="right">
      {#each prs as pr (pr.url)}
        <a class="pr-link" href={pr.url} target="_blank" rel="noopener noreferrer" title="Open the PR on GitHub">
          <GitPullRequest class="icon-sm" aria-hidden="true" />{pr.label}
        </a>
      {/each}
      <DropdownMenu.Root>
        <DropdownMenu.Trigger>
          {#snippet child({ props })}
            <Button {...props} variant="ghost" size="icon-xs" aria-label="More actions for {key}">
              <Ellipsis />
            </Button>
          {/snippet}
        </DropdownMenu.Trigger>
        <DropdownMenu.Content align="end" class="w-52">
          <DropdownMenu.Item onSelect={() => copyText(key, `Copied ${key}`)}>
            <Copy />Copy task key
          </DropdownMenu.Item>
          {#if branch}
            <DropdownMenu.Item onSelect={() => copyText(branch, 'Copied branch name')}>
              <GitBranch />Copy branch name
            </DropdownMenu.Item>
          {/if}
          {#if card.run}
            <DropdownMenu.Separator />
            <DropdownMenu.Item onSelect={() => onopen(card)}>
              <PanelRightOpen />Open workspace
            </DropdownMenu.Item>
          {/if}
        </DropdownMenu.Content>
      </DropdownMenu.Root>
    </span>
  </div>

  <div class="tc-title" title={card.title}>{card.title}</div>

  {#if card.run || owner || readOnly || assignees}
    <div class="tc-meta">
      {#if assignees}
        <span class="tc-owner" title="Assignee in Lark"><UserRound class="icon-sm" aria-hidden="true" />{assignees}</span>
      {/if}
      {#if card.run && !card.flag}<RunStatusBadge state={card.run.state} label={runLabel(card.run)} />{/if}
      {#if card.run}
        {@const chip = acChip(card.run.ac)}
        {#if chip}
          <span class="tc-ac" data-tone={chip.tone} title="Acceptance criteria review">
            <ListChecks class="icon-sm" aria-hidden="true" />{chip.text}
          </span>
        {/if}
        {#if card.run.cards.length > 0}
          <span class="tc-ac" title={card.start_blocked ?? 'Group run'}>
            <Layers class="icon-sm" aria-hidden="true" />{progressText(progress(card.run.cards))}
          </span>
        {/if}
        {#if card.run.ac && card.run.ac.design_gaps > 0}
          <span class="tc-ac" title="AC that differ from the design">
            <Palette class="icon-sm" aria-hidden="true" />{card.run.ac.design_gaps} design {card.run.ac.design_gaps === 1 ? 'gap' : 'gaps'}
          </span>
        {/if}
      {/if}
      {#if owner}
        <span class="tc-owner"><UserRound class="icon-sm" aria-hidden="true" />{owner}</span>
      {/if}
      {#if readOnly}
        <Badge variant="secondary"><Eye />Read-only</Badge>
      {/if}
    </div>
  {/if}

  {#if card.flag}<CardFlagBadge flag={card.flag} />{/if}

  {#if reason}
    <div class="tc-reason">
      <ReasonIcon aria-hidden="true" />
      <span>{reason}</span>
    </div>
  {/if}

  <div class="tc-foot">
    {#if branch && card.run}
      <span class="branch"><GitBranch class="icon-sm" aria-hidden="true" /> {branch}</span>
    {/if}
    <span class="actions">
      {#if flagAction === 'retry'}
        <Button variant="outline" size="xs" onclick={() => onretry(card)}><RefreshCw />Retry</Button>
      {:else if flagAction === 'close' && card.run}
        <Button variant="outline" size="xs" onclick={() => onclose(card)}>Close run</Button>
      {/if}
      {#if action === 'open'}
        <Button variant="ghost" size="xs" onclick={() => onopen(card)}>Open<ArrowRight /></Button>
      {:else if action === 'takeover'}
        <Button variant="outline" size="xs" onclick={() => ontakeover(card)}><ArrowLeftRight />Take over</Button>
      {:else if action === 'start'}
        <Button size="xs" disabled={card.start_blocked !== null} onclick={() => onstart(card)}><Play />Start</Button>
      {/if}
    </span>
  </div>
</article>
