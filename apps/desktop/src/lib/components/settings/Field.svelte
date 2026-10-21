<script lang="ts">
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import type { Snippet } from 'svelte';
  import { Label } from '#lib/components/ui/label/index.ts';

  interface Props {
    id: string;
    label: string;
    error?: string;
    help?: Snippet;
    children: Snippet;
  }
  let { id, label, error, help, children }: Props = $props();
</script>

<div class="field">
  <Label for={id}>{label}</Label>
  {@render children()}
  {#if error}
    <p class="field-error" id="{id}-err"><CircleAlert class="icon-sm" aria-hidden="true" />{error}</p>
  {:else if help}
    <p class="field-help">{@render help()}</p>
  {/if}
</div>
