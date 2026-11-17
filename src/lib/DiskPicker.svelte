<script lang="ts">
  import { t } from '$lib/i18n';
  import { key } from './input.svelte';
  import { onMount, tick } from 'svelte';
  import { modal } from './stage.svelte';
  import { browse, type Listing } from './games.svelte';

  // Side sheet to pick a folder or a game's file on this machine (Add a game,
  // Game folders). Folders open with A; in `file` mode only what looks
  // startable is listed (an .exe, a Linux program, a ROM). D-pad stays inside
  // (data-nav-trap); B closes it.
  let {
    mode,
    title,
    start = null,
    onpick,
    onclose,
  }: {
    mode: 'dir' | 'file';
    title: string;
    start?: string | null;
    onpick: (path: string) => void;
    onclose: () => void;
  } = $props();

  let listing = $state<Listing | null>(null);
  let error = $state('');
  let list: HTMLElement;
  const shown = $derived(listing?.entries.filter((e) => e.dir || (mode === 'file' && e.game)) ?? []);

  async function open(path: string | null) {
    try {
      listing = await browse(path);
      error = '';
    } catch (e) {
      error = String(e);
      if (!listing) listing = await browse(null).catch(() => null);
    }
    await tick();
    list?.scrollTo({ top: 0 });
    list?.querySelector<HTMLElement>('[data-focusable]')?.focus({ preventScroll: true });
  }

  onMount(() => {
    modal.close = onclose;
    void open(start);
    return () => {
      if (modal.close === onclose) modal.close = null;
    };
  });
</script>

<aside class="sheet glass" data-nav-trap>
  <h2>{title}</h2>
  <p class="path"><bdi>{listing?.path ?? ''}</bdi></p>
  <div class="places">
    {#each listing?.places ?? [] as [name, path] (path)}
      <button class="btn chip" class:sel={listing?.path === path} data-focusable onclick={() => open(path)}>{name}</button>
    {/each}
  </div>
  <div class="list scroller" bind:this={list}>
    {#if error}<p class="err">{error}</p>{/if}
    {#if mode === 'dir' && listing}
      <button class="opt use" data-focusable onclick={() => onpick(listing!.path)}>
        <span class="ico">✓</span><span class="txt">{t('Use this folder')}</span>
      </button>
    {/if}
    {#if listing?.parent}
      <button class="opt" data-focusable onclick={() => open(listing!.parent)}>
        <span class="ico">↑</span><span class="txt">{t('Up a folder')}</span>
      </button>
    {/if}
    {#each shown as e (e.path)}
      <button class="opt" class:game={e.game} data-focusable onclick={() => (e.dir ? open(e.path) : onpick(e.path))}>
        <span class="ico" aria-hidden="true">
          {#if e.dir}
            <svg viewBox="0 0 24 24"><path d="M3 6.5A1.5 1.5 0 0 1 4.5 5H9l2 2.5h8.5A1.5 1.5 0 0 1 21 9v8.5a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5z" /></svg>
          {:else}
            <svg viewBox="0 0 24 24"><path d="M8 5.5v13l10-6.5z" /></svg>
          {/if}
        </span>
        <span class="txt">{e.name}</span>
      </button>
    {/each}
    {#if listing && !shown.length && !error}
      <p class="empty">{mode === 'file' ? t('Nothing here looks like a game') : t('No folders in here')}</p>
    {/if}
  </div>
  <div class="hint"><kbd>{key('A')}</kbd> {t('open ·')} <kbd>{key('B')}</kbd> {t('close')}</div>
</aside>

<style>
  .sheet {
    position: absolute; z-index: 22; top: 0; right: 0; bottom: 0; width: 64rem;
    border-radius: 2.4rem 0 0 2.4rem; background: rgba(16, 18, 32, 0.96);
  }
  h2 { position: absolute; top: 3.4rem; left: 3.4rem; font-size: 2.4rem; font-weight: 700; }
  .path { position: absolute; top: 7.2rem; left: 3.4rem; right: 3rem; font-size: 1.2rem; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left; }
  .places { position: absolute; top: 9.6rem; left: 3rem; right: 3rem; display: flex; gap: 0.8rem; flex-wrap: wrap; max-height: 4.6rem; overflow: hidden; }
  .chip { font-size: 1.2rem; padding: 0.6rem 1.3rem; border-radius: 1.4rem; }
  .chip.sel { background: rgba(255, 255, 255, 0.3); }
  .list { top: 15rem; bottom: 5rem; padding: 0.6rem 2rem 2rem; }
  .opt {
    display: flex; align-items: center; gap: 1.2rem; width: 100%; text-align: left;
    padding: 0.9rem 1.4rem; border-radius: 1.2rem; margin-bottom: 0.3rem;
  }
  .ico { width: 2rem; height: 2rem; flex: none; display: grid; place-items: center; font-weight: 700; color: var(--muted); }
  .ico svg { width: 100%; height: 100%; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linejoin: round; }
  .game .ico { color: #6fe3a1; }
  .use .ico { color: #6fe3a1; }
  .txt { font-size: 1.5rem; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .opt:focus { background: #fff; color: #10121c; }
  .opt:focus .ico { color: #10121c; }
  .err { color: #ff8a8a; font-size: 1.3rem; padding: 0.4rem 1.4rem 1rem; }
  .empty { color: var(--muted); font-size: 1.4rem; padding: 1rem 1.4rem; }
  .hint { position: absolute; bottom: 1.8rem; left: 3.4rem; font-size: 1.15rem; color: var(--muted); }
  kbd {
    font: inherit; font-size: 0.95rem; font-weight: 700; padding: 0.05rem 0.45rem;
    border: 1px solid var(--glass-edge); border-radius: 0.4rem; color: var(--text);
  }
</style>
