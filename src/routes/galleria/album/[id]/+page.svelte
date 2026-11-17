<script lang="ts">
  import { t } from '$lib/i18n';
  import { tick } from 'svelte';
  import { page } from '$app/state';
  import { stage } from '$lib/stage.svelte';
  import { restoreFocus, reveal } from '$lib/nav';
  import PinButton from '$lib/PinButton.svelte';
  import Viewer from '$lib/Viewer.svelte';
  import {
    albumAssets, albumCount, albumPin, clock, counts, gallery, loadGallery, monthName, moreAssets, pic, saverAlbums, span,
    toggleSaverAlbum, type Asset,
  } from '$lib/gallery.svelte';

  // An album: its pictures by month or year (a page at a time, the next one
  // when the cursor gets near the end). A opens one full screen,
  // "Presentazione" plays them all as a slideshow, "Salvaschermo" adds the
  // album to this profile's screensaver.
  const id = $derived(page.params.id ?? '');
  const album = $derived(gallery.albums.find((a) => a.id === id) ?? null);
  let list = $state<Asset[]>([]);
  let more = $state(false);
  let failed = $state(false);
  let view = $state<{ start: number; show: boolean } | null>(null);

  $effect(() => {
    const cur = id;
    list = [];
    more = false;
    failed = false;
    view = null;
    if (!gallery.loaded) loadGallery().catch(() => {});
    albumAssets(cur).then(
      async (r) => {
        if (cur !== id) return;
        list = r.assets;
        more = r.more;
        await tick();
        restoreFocus(page.url.pathname, '[data-key="show"]');
        // a phone asked for this album's slideshow ("Mostra l'album sulla TV")
        if (page.url.searchParams.has('show')) startShow();
      },
      () => cur === id && (failed = true),
    );
  });
  // after the layout's own navigation effect, which clears the backdrop
  $effect(() => {
    const cover = album?.cover;
    tick().then(() => {
      stage.backdrop = cover ? pic(cover, 'preview') : null;
      stage.ambient = true;
    });
  });

  // headings in the album's order: by month, or by year when the months
  // would hold only a picture or two each (old scans spread over decades)
  const groups = $derived.by(() => {
    const byMonth = new Set(list.map((a) => a.taken.slice(0, 7))).size <= list.length / 4;
    const out: { key: string; label: string; items: { a: Asset; i: number }[] }[] = [];
    list.forEach((a, i) => {
      const key = a.taken.slice(0, byMonth ? 7 : 4);
      if (out.at(-1)?.key !== key) out.push({ key, label: !key ? t('No date') : byMonth ? monthName(key) : key, items: [] });
      out.at(-1)!.items.push({ a, i });
    });
    return out;
  });

  let loading = false;
  async function loadMore(): Promise<boolean> {
    if (!more || loading) return false;
    loading = true;
    try {
      const got = await moreAssets(id);
      const r = await albumAssets(id);
      list = r.assets;
      more = r.more;
      return got;
    } catch {
      return false;
    } finally {
      loading = false;
    }
  }

  function startShow() {
    const first = list.findIndex((a) => !a.video);
    if (first >= 0) view = { start: first, show: true };
  }

  async function closeView(i: number) {
    const key = view?.show ? 'show' : `a:${list[i]?.id}`;
    view = null;
    await tick();
    const el = document.querySelector<HTMLElement>(`[data-key="${key}"]`);
    if (el) {
      el.focus({ preventScroll: true });
      reveal(el, false);
    }
  }
</script>

<div class="scroller">
  <header>
    <h1>{album?.name ?? ''}</h1>
    {#if album}
      <div class="meta">
        <span>{list.length && !more ? counts(list) : albumCount(album)}</span>
        {#if span(album)}<span>{span(album)}</span>{/if}
        {#if album.owner}<span>{t('Shared by {owner}', { owner: album.owner })}</span>{/if}
      </div>
      {#if album.description}<p class="desc">{album.description}</p>{/if}
    {/if}
    {#if list.length}
      <div class="row">
        <button class="btn primary" data-focusable data-key="show" data-scroll-top onclick={startShow}>
          <svg viewBox="0 0 24 24"><path d="M8 5.5v13l11-6.5z" /></svg>
          {t('Slideshow')}
        </button>
        {#if album}
          <PinButton pin={albumPin(album)} />
          <button class="btn icon" data-focusable data-key="saver" data-scroll-top onclick={() => toggleSaverAlbum(album)}>
            {saverAlbums().includes(album.id) ? t('✓ In the screensaver') : t('＋ Screensaver')}
          </button>
        {/if}
      </div>
    {/if}
  </header>

  {#if failed}
    <p class="msg">{t('Album not available.')}</p>
  {/if}
  <div class="groups" data-remember>
    {#each groups as g (g.key)}
      <section>
        <h2>{g.label}</h2>
        <div class="grid">
          {#each g.items as { a, i } (a.id)}
            <button class="ph" data-focusable data-key="a:{a.id}" onclick={() => (view = { start: i, show: false })} onfocus={() => i > list.length - 36 && loadMore()}>
              <img src={pic(a.id)} alt="" loading="lazy" decoding="async" />
              {#if a.video}<span class="dur">▶ {clock(a.seconds)}</span>{/if}
            </button>
          {/each}
        </div>
      </section>
    {/each}
  </div>
  <div class="end"></div>
</div>

{#if view}
  <Viewer assets={list} start={view.start} show={view.show} title={album?.name ?? t('Gallery')} more={loadMore} onclose={closeView} />
{/if}

<style>
  header { padding: 4.4rem 6rem 1.4rem; }
  h1 { font-size: 3.4rem; font-weight: 700; letter-spacing: -0.01em; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .meta { margin-top: 0.5rem; }
  .desc { margin-top: 0.8rem; font-size: 1.4rem; line-height: 1.5; max-width: 80rem; color: rgba(244, 245, 250, 0.85); }
  .row { display: flex; gap: 1.6rem; margin-top: 1.6rem; }
  .row svg { width: 1.6rem; height: 1.6rem; fill: currentColor; }
  section { padding: 1.2rem 6rem 0.6rem; }
  h2 { font-size: 1.6rem; font-weight: 600; margin-bottom: 1rem; text-transform: capitalize; }
  .grid { display: grid; grid-template-columns: repeat(6, 1fr); gap: 1.2rem; }
  .ph {
    position: relative; aspect-ratio: 4 / 3; border-radius: 1rem; overflow: hidden;
    /* no will-change: a layer per picture (thousands in a big album) made scrolling crawl */
    background: rgba(255, 255, 255, 0.06);
  }
  .ph:focus { z-index: 2; box-shadow: 0 0 0 0.3rem #fff, 0 1.2rem 2.4rem rgba(0, 0, 0, 0.5); }
  .ph img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .dur {
    position: absolute; right: 0.6rem; bottom: 0.6rem; padding: 0.2rem 0.7rem; border-radius: 0.6rem;
    font-size: 1.1rem; font-weight: 600; background: rgba(0, 0, 0, 0.7);
  }
  .msg { margin: 1rem 6rem; font-size: 1.6rem; color: var(--muted); }
  .end { height: 8rem; }
</style>
