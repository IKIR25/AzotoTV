<script lang="ts">
  // ‹ › on the edges of the sideways row under the mouse (rows of cards), so
  // the mouse can scroll them like a controller's left / right. One pair for
  // the whole app, placed over whichever row the mouse is on.
  import { onMount } from 'svelte';
  import { input } from './input.svelte';
  import { reachable } from './nav';

  let row: HTMLElement | null = null;
  let box = $state<{ left: number; right: number; top: number; height: number } | null>(null);
  let canLeft = $state(false);
  let canRight = $state(false);

  /** The nearest ancestor that scrolls sideways and has more than it shows. */
  function sideways(el: Element | null): HTMLElement | null {
    for (let e = el; e && e !== document.body; e = e.parentElement) {
      if (!(e instanceof HTMLElement) || e.scrollWidth <= e.clientWidth + 4) continue;
      const o = getComputedStyle(e).overflowX;
      if (o === 'auto' || o === 'scroll') return e;
    }
    return null;
  }

  function place(r: HTMLElement | null) {
    row = r;
    if (!r) return (box = null);
    const b = r.getBoundingClientRect();
    box = { left: b.left, right: b.right, top: b.top, height: b.height };
    canLeft = r.scrollLeft > 2;
    canRight = r.scrollLeft + r.clientWidth < r.scrollWidth - 2;
  }

  function scroll(d: number) {
    if (!row) return;
    row.scrollBy({ left: d * row.clientWidth * 0.8, behavior: 'smooth' });
  }

  onMount(() => {
    const onMove = (e: MouseEvent) => {
      if (!input.pointer) return;
      const over = document.elementFromPoint(e.clientX, e.clientY);
      if (over?.closest('.row-arrows')) return; // on an arrow: keep its row
      const r = sideways(over);
      place(r && reachable(r) ? r : null);
    };
    // the row scrolled (arrow, wheel, controller): update which arrows make sense
    const onScroll = (e: Event) => {
      if (row && e.target === row) place(row);
      else if (row && e.target instanceof Node && e.target.contains(row)) place(row);
    };
    window.addEventListener('mousemove', onMove, { passive: true });
    document.addEventListener('scroll', onScroll, { capture: true, passive: true });
    return () => {
      window.removeEventListener('mousemove', onMove);
      document.removeEventListener('scroll', onScroll, { capture: true });
    };
  });

  $effect(() => {
    if (!input.pointer) box = null;
  });
</script>

{#if box}
  <div class="row-arrows" style="top:{box.top}px; height:{box.height}px">
    {#if canLeft}
      <button class="arrow" style="left:{box.left + 8}px" aria-label="‹" onclick={() => scroll(-1)}>‹</button>
    {/if}
    {#if canRight}
      <button class="arrow" style="left:{box.right - 8}px; translate: -100% -50%" aria-label="›" onclick={() => scroll(1)}>›</button>
    {/if}
  </div>
{/if}

<style>
  .row-arrows { position: fixed; left: 0; right: 0; pointer-events: none; z-index: 40; }
  .arrow {
    position: absolute; top: 50%; translate: 0 -50%; pointer-events: auto;
    width: 3.6rem; height: 7rem; border-radius: 1.2rem;
    font-size: 3.4rem; line-height: 1; color: var(--text);
    background: rgb(var(--shadow-rgb) / 0.55);
    border: 1px solid var(--glass-edge);
    -webkit-backdrop-filter: blur(0.6rem); backdrop-filter: blur(0.6rem);
  }
  .arrow:hover { background: var(--focus-bg); color: var(--focus-text); }
</style>
