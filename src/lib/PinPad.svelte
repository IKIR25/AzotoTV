<script lang="ts">
  import { onMount } from 'svelte';
  import { modal } from './stage.svelte';
  import { key } from './input.svelte';

  // A 4-digit PIN: dots + a phone-style keypad. A types the focused digit, the
  // keyboard's digits work too; B deletes one, and goes back when there's none.
  // `onfull` gets the PIN once the 4th digit is in (the page clears `value`).
  let {
    value = $bindable(''),
    onfull,
  }: { value?: string; onfull: (pin: string) => void } = $props();

  const KEYS = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '', '0', 'del'];
  let pad: HTMLElement;

  function press(k: string) {
    if (k === 'del') value = value.slice(0, -1);
    else if (value.length < 4) {
      value += k;
      if (value.length === 4) onfull(value);
    }
  }

  const back = () => (value ? (value = value.slice(0, -1)) : history.back());

  onMount(() => {
    pad.querySelector<HTMLElement>('[data-focusable]')?.focus({ preventScroll: true });
    modal.close = back;
    const digits = (e: KeyboardEvent) => {
      if (/^[0-9]$/.test(e.key)) {
        e.preventDefault();
        press(e.key);
      }
    };
    window.addEventListener('keydown', digits);
    return () => {
      window.removeEventListener('keydown', digits);
      if (modal.close === back) modal.close = null;
    };
  });
</script>

<div class="dots" aria-label="PIN">
  {#each [0, 1, 2, 3] as i (i)}<i class:on={i < value.length}></i>{/each}
</div>

<div class="pad" bind:this={pad}>
  {#each KEYS as k, i (i)}
    {#if k}
      <button class="key" class:del={k === 'del'} data-focusable onclick={() => press(k)}>
        {#if k === 'del'}
          <svg viewBox="0 0 24 24" aria-label="Cancella"><path d="M9 5h11v14H9l-6-7z" /><path d="M12.5 9.5l5 5M17.5 9.5l-5 5" /></svg>
        {:else}{k}{/if}
      </button>
    {:else}
      <span></span>
    {/if}
  {/each}
</div>

<div class="hint"><kbd>{key('A')}</kbd> scrivi · <kbd>{key('B')}</kbd> cancella</div>

<style>
  .dots { display: flex; gap: 1.6rem; justify-content: center; margin: 0.6rem 0 1rem; }
  .dots i { width: 1.6rem; height: 1.6rem; border-radius: 50%; border: 0.2rem solid rgba(255, 255, 255, 0.6); }
  .dots i.on { background: #fff; border-color: #fff; }
  .pad { display: grid; grid-template-columns: repeat(3, 7rem); gap: 1.2rem; justify-content: center; }
  .key {
    width: 7rem; height: 7rem; border-radius: 50%; display: grid; place-items: center;
    font-size: 2.6rem; font-weight: 600; background: rgba(255, 255, 255, 0.12);
  }
  .key.del { background: none; }
  /* plain state change, no transition (see tween.ts) */
  .key:focus { background: #fff; color: #10121c; }
  svg { width: 2.8rem; height: 2.8rem; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
  .hint { font-size: 1.15rem; color: var(--muted); text-align: center; }
  kbd {
    font: inherit; font-size: 0.95rem; font-weight: 700; padding: 0.05rem 0.45rem;
    border: 1px solid var(--glass-edge); border-radius: 0.4rem; color: var(--text);
  }
</style>
