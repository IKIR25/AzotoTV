<script lang="ts">
  import { tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { page } from '$app/state';
  import { refresh, showToast, stage } from '$lib/stage.svelte';
  import { restoreFocus } from '$lib/nav';
  import Card from '$lib/Card.svelte';
  import Row from '$lib/Row.svelte';
  import Viewer from '$lib/Viewer.svelte';
  import {
    albumTile, clearWallpaper, counts, gallery, loadGallery, longDate, pic, wallpaper, yearsAgo, type Asset, type Memory,
  } from '$lib/gallery.svelte';

  // The Galleria: "Accadde oggi" (this date in earlier years), then every album
  // shared with the TV. "Presentazione" mixes pictures from all of them.
  // "Togli lo sfondo" when this profile has a photo as its wallpaper.
  // `?oggi=<year>` (from the home's shelf) opens that year's memory at once.
  let failed = $state(false);
  let view = $state<{ assets: Asset[]; title: string; show: boolean; back: string } | null>(null);
  // ?oggi= opens its memory once, not again when the page reloads after a video
  let oggiDone = false;

  $effect(() => {
    void refresh.n;
    stage.backdrop = null;
    loadGallery().then(
      async () => {
        failed = false;
        await tick();
        const y = Number(page.url.searchParams.get('oggi'));
        const m = y && !oggiDone ? gallery.memories.find((x) => x.year === y) : undefined;
        oggiDone = true;
        if (m && !view) return openMemory(m);
        if (!view) restoreFocus(page.url.pathname, '.content [data-focusable]');
      },
      () => (failed = true),
    );
  });

  const memoryTile = (m: Memory) => {
    const first = m.assets.find((a) => !a.video) ?? m.assets[0];
    return {
      key: `m:${m.year}`,
      src: pic(first.id, 'preview'),
      title: yearsAgo(m.year),
      sub: `${longDate(first.taken)} · ${counts(m.assets)}`,
      open: () => openMemory(m),
    };
  };

  function openMemory(m: Memory) {
    view = { assets: m.assets, title: `${yearsAgo(m.year)} · ${longDate(m.assets[0].taken)}`, show: false, back: `m:${m.year}` };
  }

  async function slideshowAll() {
    try {
      const assets = await invoke<Asset[]>('im_random', { n: 150 });
      if (!assets.length) return showToast('Nessuna foto da mostrare');
      view = { assets, title: 'Galleria', show: true, back: 'show' };
    } catch {
      showToast('Il server delle foto non risponde');
    }
  }

  async function closeView() {
    const key = view?.back;
    view = null;
    await tick();
    document.querySelector<HTMLElement>(`[data-key="${key}"]`)?.focus({ preventScroll: true });
  }
</script>

<div class="scroller">
  <header>
    <h1>Galleria</h1>
    {#if gallery.albums.length}
      <div class="row">
        <button class="btn primary" data-focusable data-key="show" data-scroll-top onclick={slideshowAll}>
          <svg viewBox="0 0 24 24"><path d="M8 5.5v13l11-6.5z" /></svg>
          Presentazione
        </button>
        {#if wallpaper()}
          <button class="btn" data-focusable data-key="wall" data-scroll-top onclick={clearWallpaper}>✕ Togli lo sfondo</button>
        {/if}
      </div>
    {/if}
  </header>

  <div class="content" data-remember>
    {#if !gallery.enabled}
      <p class="msg">La Galleria non è ancora collegata al server delle foto.</p>
    {:else if failed && !gallery.loaded}
      <p class="msg">Il server delle foto non risponde. Riprova tra poco.</p>
    {:else if gallery.loaded && !gallery.albums.length}
      <p class="msg">
        Ancora nessun album. Dall'app Immich (sul telefono o sul computer) condividi un album con l'utente «TV»: comparirà qui, per
        tutti i profili.
      </p>
    {:else}
      {#if gallery.memories.length}
        <Row title="Accadde oggi">
          {#each gallery.memories as m (m.year)}<Card wide size={24} tile={memoryTile(m)} />{/each}
        </Row>
      {/if}
      {#if gallery.albums.length}
        <h2>Album</h2>
        <div class="grid">
          {#each gallery.albums as a (a.id)}<Card wide size={24} tile={albumTile(a)} />{/each}
        </div>
      {/if}
    {/if}
    <div class="end"></div>
  </div>
</div>

{#if view}
  <Viewer assets={view.assets} show={view.show} title={view.title} onclose={closeView} />
{/if}

<style>
  header { padding: 3.8rem 6rem 1rem; }
  h1 { font-size: 3.6rem; font-weight: 700; letter-spacing: -0.02em; }
  .row { display: flex; gap: 1.6rem; margin-top: 1.2rem; }
  .row .btn { font-size: 1.3rem; padding: 0.7rem 1.6rem; }
  .row svg { width: 1.6rem; height: 1.6rem; fill: currentColor; }
  .content { padding-top: 1.4rem; }
  h2 { font-size: 1.9rem; font-weight: 600; margin: 0 6rem 0.2rem; }
  .grid { display: grid; grid-template-columns: repeat(4, 24rem); gap: 2.4rem 1.4rem; padding: 1.4rem 6rem 2rem; }
  .msg { margin: 1rem 6rem; font-size: 1.6rem; color: var(--muted); max-width: 80rem; line-height: 1.5; }
  .end { height: 8rem; }
</style>
