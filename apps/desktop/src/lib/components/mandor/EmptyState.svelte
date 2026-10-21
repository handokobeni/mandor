<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { IconComponent } from './icons.ts';

  interface Props {
    icon: IconComponent;
    title: string;
    children?: Snippet;
    actions?: Snippet;
  }
  let { icon: Icon, title, children, actions }: Props = $props();
</script>

<div class="empty">
  <span class="ico"><Icon aria-hidden="true" /></span>
  <h3>{title}</h3>
  {#if children}<p>{@render children()}</p>{/if}
  {@render actions?.()}
</div>
