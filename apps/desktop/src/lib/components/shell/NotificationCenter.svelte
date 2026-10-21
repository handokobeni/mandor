<script lang="ts">
  import Bell from '@lucide/svelte/icons/bell';
  import BellOff from '@lucide/svelte/icons/bell-off';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import GitPullRequest from '@lucide/svelte/icons/git-pull-request';
  import Hand from '@lucide/svelte/icons/hand';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import Wrench from '@lucide/svelte/icons/wrench';
  import { relTime } from '#lib/board.ts';
  import { Button } from '#lib/components/ui/button/index.ts';
  import * as Popover from '#lib/components/ui/popover/index.ts';
  import type { NoticeView } from '#lib/protocol/NoticeView.ts';
  import { nav } from '#lib/stores/nav.svelte.ts';
  import { notifications } from '#lib/stores/notifications.svelte.ts';

  let open = $state(false);
  let now = $state(Date.now());

  const KINDS: Record<string, { icon: typeof Bell; kind: string }> = {
    awaiting_answer: { icon: Hand, kind: 'waiting' },
    failed: { icon: CircleX, kind: 'failed' },
    needs_attention: { icon: TriangleAlert, kind: 'attention' },
    waiting_fix: { icon: Wrench, kind: 'fix' },
    pr_created: { icon: GitPullRequest, kind: 'pr' }
  };

  function select(n: NoticeView): void {
    open = false;
    void notifications.markRead([n.id]);
    if (n.run_id) nav.run(n.run_id);
    else nav.board();
  }
</script>

<Popover.Root
  bind:open
  onOpenChange={(o) => {
    if (o) now = Date.now();
  }}
>
  <Popover.Trigger>
    {#snippet child({ props })}
      <Button
        {...props}
        variant="ghost"
        size="icon-sm"
        class="bell"
        aria-label="Notifications{notifications.unread ? `, ${notifications.unread} unread` : ''}"
        title="Notifications"
      >
        <Bell />
        {#if notifications.unread > 0}
          <span class="bell-count mono" aria-hidden="true">{notifications.unread > 99 ? '99+' : notifications.unread}</span>
        {/if}
      </Button>
    {/snippet}
  </Popover.Trigger>
  <Popover.Content align="end" class="notif">
    <div class="notif-head">
      <b>Notifications</b>
      <span class="muted">{notifications.unread ? `${notifications.unread} unread` : 'All caught up'}</span>
      {#if notifications.unread}
        <Button variant="ghost" size="xs" class="ml-auto" onclick={() => notifications.markAllRead()}>Mark all as read</Button>
      {/if}
    </div>
    <div class="notif-list scroll-area">
      {#each notifications.items as n (n.id)}
        {@const k = KINDS[n.kind] ?? { icon: Bell, kind: '' }}
        {@const Icon = k.icon}
        <button type="button" class="notif-item" data-unread={n.read ? undefined : 'true'} onclick={() => select(n)}>
          <span class="ni-ico" data-kind={k.kind}><Icon class="icon-sm" aria-hidden="true" /></span>
          <span class="ni-txt"><span class="ni-title">{n.title}</span><span class="ni-body">{n.body}</span></span>
          <span class="ni-meta">
            <span class="mono">{relTime(n.created_at_ms, now)}</span>
            {#if !n.read}<span class="ni-dot" aria-label="Unread"></span>{/if}
          </span>
        </button>
      {:else}
        <div class="empty" style="padding:24px">
          <span class="ico"><BellOff aria-hidden="true" /></span>
          <p>No notifications yet.</p>
        </div>
      {/each}
    </div>
  </Popover.Content>
</Popover.Root>
