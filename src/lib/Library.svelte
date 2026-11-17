<script lang="ts">
  import { t } from '$lib/i18n';
  import { tick } from 'svelte';
  import { page } from '$app/state';
  import { api, type Item } from '$lib/jf';
  import { refresh } from '$lib/stage.svelte';
  import { restoreFocus } from '$lib/nav';
  import Card from '$lib/Card.svelte';
  import Row from '$lib/Row.svelte';
  import { goto } from '$app/navigation';
  import { playTracks } from '$lib/music.svelte';
  import PlaylistSheet from '$lib/PlaylistSheet.svelte';

  const TYPES: Record<string, string> = { movies: 'Movie', tvshows: 'Series', music: 'Audio', boxsets: 'BoxSet' };
  const SORTS = [
    { label: t('Added'), by: 'DateCreated', order: 'Descending' },
    { label: t('Name'), by: 'SortName', order: 'Ascending' },
    { label: t('Year'), by: 'ProductionYear,SortName', order: 'Descending' },
    { label: t('Rating'), by: 'CommunityRating', order: 'Descending' },
  ];
  const PAGE = 60;

  // A library grid: a TV app tab (/library/<id>) or the Musica app (/musica,
  // `app`: its own page, so the title sits level with the clock corner).
  let { id, app = false }: { id: string; app?: boolean } = $props();
  let view = $state<Item | null>(null);
  let items = $state<Item[]>([]);
  let total = $state(0);
  let sort = $state(0);
  let loading = false;
  const isMusic = $derived(view?.CollectionType === 'music');
  let playlists = $state<Item[]>([]);
  /** "＋ Nuova playlist": the sheet (picking an existing one just opens it) */
  let newList = $state(false);
  // Bumped on every reload: responses for an older library/sort are dropped.
  let gen = 0;

  async function fetchPage(start: number) {
    const s = SORTS[sort];
    return api.items({
      parentId: id,
      types: TYPES[view?.CollectionType ?? ''],
      sortBy: s.by,
      sortOrder: s.order,
      start,
      limit: PAGE,
    });
  }

  async function reload(restore: boolean) {
    const mine = ++gen;
    loading = true;
    const p = await fetchPage(0);
    if (mine !== gen) return;
    items = p.Items;
    total = p.TotalRecordCount;
    loading = false;
    if (restore) {
      await tick();
      restoreFocus(page.url.pathname, '.grid [data-focusable]');
    }
  }

  async function more(i: number) {
    if (loading || items.length >= total || i < items.length - 14) return;
    const mine = gen;
    loading = true;
    const p = await fetchPage(items.length);
    if (mine !== gen) return;
    // items added on the server between pages shift the offsets: skip repeats
    // (a duplicate key would also crash the keyed #each)
    const seen = new Set(items.map((x) => x.Id));
    items = [...items, ...p.Items.filter((x) => !seen.has(x.Id))];
    loading = false;
  }

  $effect(() => {
    const vid = id;
    void refresh.n;
    items = [];
    playlists = [];
    api.view(vid).then(async (v) => {
      if (vid !== id) return;
      view = v;
      if (view?.CollectionType === 'music')
        api.playlists().then((p) => vid === id && (playlists = p), () => {});
      await reload(true);
    });
  });

  // Music: a song plays the whole library (in the order shown) starting from it.
  async function playFrom(songId: string | null, shuffle = false) {
    const s = SORTS[sort];
    const all = (await api.tracks(id, s.by, s.order)).Items;
    const i = songId ? Math.max(0, all.findIndex((x) => x.Id === songId)) : 0;
    await playTracks(all, i, { shuffle });
  }
</script>

<div class="scroller">
  <header class:app>
    <h1>{view?.Name ?? ''}</h1>
    <span class="count">{total ? (isMusic ? t('{n} songs', { n: total }) : t('{n} items', { n: total })) : ''}</span>
    {#if isMusic && total}
      <div class="play">
        <button class="btn primary" data-focusable data-key="playall" data-scroll-top onclick={() => playFrom(null)}>
          <svg viewBox="0 0 24 24"><path d="M8 5v14l11-7z" /></svg> {t('Play all')}
        </button>
        <button class="btn" data-focusable data-key="shuffleall" data-scroll-top onclick={() => playFrom(null, true)}>
          <svg viewBox="0 0 24 24"
            ><path
              d="M10.59 9.17 5.41 4 4 5.41l5.17 5.17zM14.5 4l2.04 2.04L4 18.59 5.41 20 17.96 7.46 20 9.5V4zm.33 9.41-1.41 1.41 3.13 3.13L14.5 20H20v-5.5l-2.04 2.04z"
            /></svg
          > {t('Shuffle')}
        </button>
      </div>
    {/if}
    {#if isMusic && app}
      <div class="play">
        <button class="btn" data-focusable data-key="msearch" data-scroll-top onclick={() => goto('/musica/cerca')}>
          <svg viewBox="0 0 24 24"><path d="M10 3a7 7 0 0 1 5.6 11.2l5.1 5.1-1.4 1.4-5.1-5.1A7 7 0 1 1 10 3zm0 2a5 5 0 1 0 0 10 5 5 0 0 0 0-10z" /></svg>
          {t('Search')}
        </button>
        <button class="btn" data-focusable data-key="newlist" data-scroll-top onclick={() => (newList = true)}>{t('＋ New playlist')}</button>
      </div>
    {/if}
    <div class="sorts">
      {#each SORTS as s, i}
        <button
          class="btn chip"
          class:sel={sort === i}
          data-focusable
          data-scroll-top
          onclick={async () => {
            sort = i;
            await reload(false);
          }}>{s.label}</button
        >
      {/each}
    </div>
  </header>

  {#if playlists.length}
    <Row title={t('Your playlists')}>
      {#each playlists as p (p.Id)}<Card item={p} />{/each}
    </Row>
    <h2 class="sub">{t('All songs')}</h2>
  {/if}

  <div class="grid">
    {#each items as it, i (it.Id)}
      <Card item={it} onfocus={() => more(i)} onopen={isMusic ? () => playFrom(it.Id) : undefined} />
    {/each}
  </div>
  <div class="end"></div>
</div>

{#if newList}
  <PlaylistSheet
    title={t('Your playlists')}
    onpick={(pid) => {
      newList = false;
      goto(`/tracks/${pid}`);
    }}
    onclose={async () => {
      newList = false;
      await tick();
      document.querySelector<HTMLElement>('[data-key="newlist"]')?.focus({ preventScroll: true });
    }}
  />
{/if}

<style>
  header { padding: 10.5rem 6rem 1rem; display: flex; align-items: baseline; gap: 2rem; flex-wrap: wrap; }
  /* the sort chips would collide with the clock corner: they wrap below the title */
  header.app { padding-top: 3.8rem; }
  header.app h1 { flex-basis: 100%; }
  header.app .play { margin-left: 0; }
  h1 { font-size: 3.6rem; font-weight: 700; letter-spacing: -0.02em; }
  .count { color: var(--muted); font-size: 1.5rem; }
  .sorts { margin-left: auto; display: flex; gap: 0.8rem; }
  .play { display: flex; gap: 0.8rem; margin-left: 1rem; align-self: center; }
  .play .btn { font-size: 1.3rem; padding: 0.7rem 1.6rem; }
  .play svg { width: 1.6rem; height: 1.6rem; fill: currentColor; }
  .sub { font-size: 1.9rem; font-weight: 600; margin: 0.4rem 6rem 0; }
  .chip { font-size: 1.3rem; padding: 0.7rem 1.5rem; border-radius: 1.4rem; }
  .chip.sel { background: rgb(var(--fg-rgb) / 0.34); }
  .chip.sel:focus { background: var(--focus-bg); }
  .grid {
    display: grid; grid-template-columns: repeat(7, 14rem); gap: 2.2rem 1.4rem;
    padding: 2rem 6rem 2rem; justify-content: start;
  }
  .end { height: 10rem; }
</style>
