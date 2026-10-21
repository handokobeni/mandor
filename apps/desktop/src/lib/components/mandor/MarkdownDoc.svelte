<script lang="ts">
  import { renderMarkdown } from '#lib/markdown.ts';

  // Markdown tidak tepercaya (agent, Lark) dirender lewat sanitizer.
  let { source }: { source: string } = $props();
</script>

<article class="prose">{@html renderMarkdown(source)}</article>
