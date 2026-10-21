// Ikon Lucide untuk state run dan flag kartu (handoff desain, handoff_icons.txt).

import type { Component } from 'svelte';
import CircleCheck from '@lucide/svelte/icons/circle-check';
import CirclePause from '@lucide/svelte/icons/circle-pause';
import CircleX from '@lucide/svelte/icons/circle-x';
import FolderGit2 from '@lucide/svelte/icons/folder-git-2';
import GitPullRequest from '@lucide/svelte/icons/git-pull-request';
import GitPullRequestArrow from '@lucide/svelte/icons/git-pull-request-arrow';
import Hand from '@lucide/svelte/icons/hand';
import LoaderCircle from '@lucide/svelte/icons/loader-circle';
import MessageSquareDashed from '@lucide/svelte/icons/message-square-dashed';
import RefreshCwOff from '@lucide/svelte/icons/refresh-cw-off';
import SquareParking from '@lucide/svelte/icons/square-parking';
import Timer from '@lucide/svelte/icons/timer';
import Trash from '@lucide/svelte/icons/trash';
import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
import UserX from '@lucide/svelte/icons/user-x';
import Wrench from '@lucide/svelte/icons/wrench';

export type IconComponent = Component<{ class?: string; 'aria-hidden'?: boolean | 'true' }>;

const RUN_ICONS: Record<string, IconComponent> = {
  preparing: FolderGit2,
  running: LoaderCircle,
  awaiting_dev: Hand,
  publishing: GitPullRequestArrow,
  in_review_loop: GitPullRequest,
  wf_awaiting_comment: MessageSquareDashed,
  wf_quiet: Timer,
  wf_running: Wrench,
  failed: CircleX,
  orphaned: SquareParking,
  stopped: CirclePause
};

const FLAG_ICONS: Record<string, IconComponent> = {
  overwritten: TriangleAlert,
  sync_failed: RefreshCwOff,
  deleted: Trash,
  unassigned: UserX
};

export function runIcon(state: string): IconComponent {
  return RUN_ICONS[state] ?? CircleCheck;
}

export function flagIcon(kind: string): IconComponent {
  return FLAG_ICONS[kind] ?? TriangleAlert;
}
