<script lang="ts">
  // Checklist kesiapan sebelum review dengan browser (KTD-39): item yang kurang
  // menjelaskan cara memperbaikinya; "Run review anyway" menjalankan review dan AC
  // tampilan menjadi Can't verify.
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import CircleDashed from '@lucide/svelte/icons/circle-dashed';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import Play from '@lucide/svelte/icons/play';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import ScanSearch from '@lucide/svelte/icons/scan-search';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import { cantVerifyReason, missingCount, readinessItems } from '#lib/app-run.ts';
  import { Button } from '#lib/components/ui/button/index.ts';
  import { Checkbox } from '#lib/components/ui/checkbox/index.ts';
  import * as Dialog from '#lib/components/ui/dialog/index.ts';
  import type { ReviewReadinessView } from '#lib/protocol/ReviewReadinessView.ts';

  interface Props {
    taskKey: string;
    trigger: string;
    readiness: ReviewReadinessView;
    ownerLabel: string | null;
    checking: boolean;
    oncancel: () => void;
    /** Konfirmasi database/login disimpan di state app lalu review dimulai. */
    onstart: (confirmed: { database: boolean; login: boolean }) => void;
    onanyway: () => void;
    onrunapp: () => void;
    onrecheck: () => void;
  }
  let { taskKey, trigger, readiness, ownerLabel, checking, oncancel, onstart, onanyway, onrunapp, onrecheck }: Props =
    $props();

  let database = $state(false);
  let login = $state(false);
  $effect(() => {
    database = readiness.confirmed.database;
    login = readiness.confirmed.login;
  });
  const items = $derived(readinessItems(readiness, { database, login }, ownerLabel));
  const missing = $derived(missingCount(items));
  const reason = $derived(cantVerifyReason(items));
</script>

<Dialog.Root open onOpenChange={(o) => !o && oncancel()}>
  <Dialog.Content class="sm:max-w-[600px]">
    <Dialog.Header>
      <span class="mono muted">{taskKey} · {trigger}</span>
      <Dialog.Title class="text-[length:var(--text-md)]">Ready for a browser review?</Dialog.Title>
      <Dialog.Description>
        The reviewer checks UI AC in your running app. Anything missing here makes those AC Can't verify.
      </Dialog.Description>
    </Dialog.Header>
    <ul class="ready-list">
      {#each items as it (it.key)}
        <li class="ready-item" data-state={it.state}>
          <span class="ri-ico" data-tone={it.state === 'ok' ? 'ok' : it.state === 'missing' ? 'failed' : it.state === 'warn' ? 'warning' : 'muted'}>
            {#if it.state === 'ok'}<CircleCheck />{:else if it.state === 'missing'}<CircleX />{:else if it.state === 'warn'}<TriangleAlert />{:else}<CircleDashed />{/if}
          </span>
          <div class="ri-txt">
            <div class="ri-title">{it.title}</div>
            <div class="muted">{it.help}</div>
          </div>
          <span class="ri-act">
            {#if it.key === 'database'}
              <label class="ri-check"><Checkbox checked={database} onCheckedChange={(v) => (database = v === true)} /> Confirmed</label>
            {:else if it.key === 'login'}
              <label class="ri-check"><Checkbox checked={login} onCheckedChange={(v) => (login = v === true)} /> I'm logged in</label>
            {:else if it.action === 'run_app'}
              <Button variant="outline" size="xs" onclick={onrunapp}><Play />Run app</Button>
            {:else if it.action === 'restart_app'}
              <Button variant="outline" size="xs" onclick={onrunapp}><RotateCcw />Restart app</Button>
            {:else if it.action === 'recheck'}
              <Button variant="outline" size="xs" disabled={checking} onclick={onrecheck}>
                {#if checking}<LoaderCircle class="spin" />Checking…{:else}<RefreshCw />Check again{/if}
              </Button>
            {/if}
          </span>
        </li>
      {/each}
    </ul>
    <Dialog.Footer class="items-center">
      <span class="muted mr-auto text-[length:var(--text-sm)]">
        {#if missing === 0}All set.{:else}{missing} item{missing === 1 ? '' : 's'} missing. Without them, UI AC become Can't verify ({reason}).{/if}
      </span>
      <Button variant="ghost" onclick={oncancel}>Cancel</Button>
      <Button variant="outline" onclick={onanyway}>Run review anyway</Button>
      <Button disabled={missing > 0} onclick={() => onstart({ database, login })}><ScanSearch />Start review</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
