<script lang="ts">
  import { t } from '$lib/i18n';
  import { key } from './input.svelte';
  import { onMount } from 'svelte';
  import { modal } from './stage.svelte';

  // Side sheet with a list of choices (audio / subtitle tracks). D-pad stays
  // inside it (data-nav-trap); B closes it (modal.close, see the layout).
  let {
    title,
    options,
    value,
    onpick,
    onclose,
    light = false,
  }: {
    title: string;
    options: { key: number; main: string; sub?: string }[];
    value: number | null;
    onpick: (key: number) => void;
    onclose: () => void;
    /** white, for the white apps (Mii) */
    light?: boolean;
  } = $props();

  let list: HTMLElement;
  onMount(() => {
    modal.close = onclose;
    const sel = list.querySelector<HTMLElement>('.opt.sel') ?? list.querySelector<HTMLElement>('.opt');
    sel?.focus({ preventScroll: true });
    // centre the current choice (scrollIntoView would also scroll the page behind)
    if (sel) list.scrollTop = sel.offsetTop - list.clientHeight / 2;
    return () => {
      if (modal.close === onclose) modal.close = null;
    };
  });
</script>

<aside class="sheet glass" class:light data-nav-trap>
  <h2>{title}</h2>
  <div class="list scroller" bind:this={list}>
    {#each options as o (o.key)}
      <button class="opt" class:sel={o.key === value} data-focusable onclick={() => onpick(o.key)}>
        <span class="tick">{o.key === value ? '✓' : ''}</span>
        <span class="txt">{o.main}{#if o.sub}<small>{o.sub}</small>{/if}</span>
      </button>
    {/each}
  </div>
  <div class="hint"><kbd>{key('A')}</kbd> {t('select ·')} <kbd>{key('B')}</kbd> {t('close')}</div>
</aside>

<style>
  .sheet {
    position: absolute; z-index: 22; top: 0; right: 0; bottom: 0; width: 48rem;
    border-radius: 2.4rem 0 0 2.4rem; background: rgb(var(--panel-rgb) / 0.94);
  }
  h2 { position: absolute; top: 4rem; left: 3.4rem; font-size: 2.4rem; font-weight: 700; }
  .list { top: 9.5rem; bottom: 5rem; padding: 0.6rem 2rem 2rem; }
  .opt {
    display: flex; align-items: center; gap: 1rem; width: 100%; text-align: left;
    padding: 1.1rem 1.4rem; border-radius: 1.2rem; margin-bottom: 0.4rem;
  }
  .tick { width: 1.8rem; font-weight: 700; color: var(--accent); }
  .txt { font-size: 1.6rem; font-weight: 600; }
  .txt small { display: block; font-size: 1.15rem; font-weight: 400; color: var(--muted); margin-top: 0.15rem; }
  /* plain state change, no transition (see tween.ts) */
  .opt:focus { background: var(--focus-bg); color: var(--focus-text); }
  .opt:focus .tick { color: var(--focus-text); }
  .opt:focus small { color: rgb(var(--focus-text-rgb) / 0.65); }
  .hint { position: absolute; bottom: 1.8rem; left: 3.4rem; font-size: 1.15rem; color: var(--muted); }
  .light { background: rgba(255, 255, 255, 0.97); color: #2b2f38; box-shadow: -1rem 0 4rem rgba(30, 40, 60, 0.12); }
  .light .txt small, .light .hint { color: #7b818d; }
  .light kbd { border-color: #d5d9e0; color: #2b2f38; }
  .light .tick { color: #3aa6c9; }
  .light .opt:focus { background: #3aa6c9; color: #fff; }
  .light .opt:focus .tick, .light .opt:focus small { color: var(--fg); }
  kbd {
    font: inherit; font-size: 0.95rem; font-weight: 700; padding: 0.05rem 0.45rem;
    border: 1px solid var(--glass-edge); border-radius: 0.4rem; color: var(--text);
  }
</style>
