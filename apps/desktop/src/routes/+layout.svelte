<script lang="ts">
  import '@fontsource-variable/space-grotesk';
  import '@fontsource-variable/jetbrains-mono';
  import '../app.css';
  import '#lib/styles/mandor.css';
  import { onMount } from 'svelte';
  import { theme } from '#lib/stores/theme.svelte.ts';

  let { children } = $props();

  onMount(() => theme.start());

  // Kelas `dark` di <html> mengikuti preferensi tema.
  $effect(() => {
    document.documentElement.classList.toggle('dark', theme.resolved === 'dark');
  });
</script>

{@render children()}
