<script lang="ts" module>
  export interface Step {
    label: string;
    s: 'todo' | 'active' | 'done' | 'error';
    err?: string;
  }
</script>

<script lang="ts">
  import Circle from '@lucide/svelte/icons/circle';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';

  let { steps }: { steps: Step[] } = $props();
</script>

<div class="steps" aria-live="polite">
  {#each steps as step (step.label)}
    <div class="step" data-s={step.s}>
      {#if step.s === 'done'}
        <CircleCheck aria-hidden="true" />
      {:else if step.s === 'active'}
        <LoaderCircle class="spin" aria-hidden="true" />
      {:else if step.s === 'error'}
        <CircleX aria-hidden="true" />
      {:else}
        <Circle aria-hidden="true" />
      {/if}
      <span>
        {step.label}
        {#if step.err}<span class="err">{step.err}</span>{/if}
      </span>
    </div>
  {/each}
</div>
