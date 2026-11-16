<script lang="ts">
  import type { Snippet } from 'svelte';
  // `list`: the cards (as list rows, see Card) go down in two columns instead
  // of sideways (search results next to the keyboard)
  let { title, list = false, children }: { title: string; list?: boolean; children: Snippet } = $props();
</script>

<section class="shelf" class:list>
  <h2>{title}</h2>
  <div class={list ? 'cols' : 'row'}>{@render children()}</div>
</section>

<style>
  .shelf { margin-bottom: 1.4rem; }
  h2 { font-size: 1.9rem; font-weight: 600; margin: 0 6rem 0.2rem; }
  .row {
    display: flex; gap: 1.4rem; padding: 1.4rem 6rem 2.2rem;
    overflow-x: auto; scrollbar-width: none; scroll-padding: 0 6rem;
  }
  .row::-webkit-scrollbar { display: none; }
  .list h2 { margin: 0 0 0.4rem; }
  .cols { display: grid; grid-template-columns: 1fr 1fr; gap: 0.4rem 1.2rem; }
</style>
