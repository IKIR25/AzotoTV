<script lang="ts">
  import { t } from '$lib/i18n';
  import { tick } from 'svelte';
  import { page } from '$app/state';
  import { api, backdropUrl, episodeLabel, runtime, type Item } from '$lib/jf';
  import { refresh, stage } from '$lib/stage.svelte';
  import { restoreFocus } from '$lib/nav';
  import Card from '$lib/Card.svelte';
  import { playTracks } from '$lib/music.svelte';
  import Row from '$lib/Row.svelte';

  type R = { key: string; title: string; items: Item[]; wide?: boolean };
  let rows = $state<R[]>([]);
  let focused = $state<Item | null>(null);
  let ready = $state(false);
  let failed = $state('');

  const current = $derived(focused ?? rows[0]?.items[0] ?? null);
  const title = $derived(current?.Type === 'Episode' ? (current.SeriesName ?? current.Name) : (current?.Name ?? ''));
  const subtitle = $derived(
    current?.Type === 'Episode'
      ? `${episodeLabel(current)} · ${current.Name}`
      : current?.Type === 'Audio'
        ? (current.AlbumArtist ?? '')
        : '',
  );

  $effect(() => {
    stage.backdrop = backdropUrl(current);
  });

  // a song on Home plays the songs of its row, starting from it
  function playRow(items: Item[], it: Item) {
    const songs = items.filter((x) => x.Type === 'Audio');
    playTracks(songs, songs.indexOf(it));
  }

  async function load() {
    try {
      const [views, resume, next, favs] = await Promise.all([
        api.views(),
        api.resume(),
        api.nextUp(),
        api.favorites().catch(() => ({ Items: [] as Item[], TotalRecordCount: 0 })),
      ]);
      const out: R[] = [];
      if (resume.Items.length) out.push({ key: 'resume', title: t('Continue watching'), items: resume.Items, wide: true });
      if (next.Items.length) out.push({ key: 'next', title: t('Next episode'), items: next.Items, wide: true });
      if (favs.Items.length) out.push({ key: 'favs', title: t('Favorites'), items: favs.Items });

      const latest = await Promise.all(
        views.Items.map((v) =>
          v.CollectionType === 'boxsets'
            ? api
                .items({ parentId: v.Id, types: 'BoxSet', sortBy: 'DateCreated', sortOrder: 'Descending', limit: 20 })
                .then((p) => p.Items)
            : api.latest(v.Id),
        ),
      );
      views.Items.forEach((v, i) => {
        if (latest[i].length)
          out.push({
            key: v.Id,
            title: v.CollectionType === 'boxsets' ? v.Name : t('{Name} · recently added', { Name: v.Name }),
            items: latest[i],
          });
      });
      rows = out;
    } catch (e) {
      failed = String(e);
    }
    ready = true;
    await tick();
    restoreFocus(page.url.pathname);
  }

  $effect(() => {
    void refresh.n;
    load();
  });
</script>

<div class="stage">
  {#if current}
    <section class="info">
      <h1>{title}</h1>
      {#if subtitle}<div class="sub">{subtitle}</div>{/if}
      <div class="meta">
        {#if current.ProductionYear}<span>{current.ProductionYear}</span>{/if}
        {#if runtime(current)}<span>{runtime(current)}</span>{/if}
        {#if current.CommunityRating}<span>★ {current.CommunityRating.toFixed(1)}</span>{/if}
        {#if current.OfficialRating}<span class="pill">{current.OfficialRating}</span>{/if}
      </div>
      {#if current.Overview}<p>{current.Overview}</p>{/if}
    </section>
  {/if}

  <div class="rows scroller" data-snap="shelf">
    {#if failed}
      <p class="err">{t('Couldn\'t load: {failed}', { failed })}</p>
    {:else if ready && !rows.length}
      <p class="err">{t('The library is empty.')}</p>
    {/if}
    {#each rows as row (row.key)}
      <Row title={row.title}>
        {#each row.items as it (it.Id)}
          <Card
            item={it}
            wide={row.wide}
            onfocus={(f) => (focused = f)}
            onopen={it.Type === 'Audio' ? () => playRow(row.items, it) : undefined}
          />
        {/each}
      </Row>
    {/each}
    <div class="end"></div>
  </div>
</div>

<style>
  .stage { position: absolute; inset: 0; }
  .info { position: absolute; top: 12rem; left: 6rem; width: 62rem; height: 24rem; overflow: hidden; }
  h1 { font-size: 4.6rem; font-weight: 700; letter-spacing: -0.02em; line-height: 1.05; }
  .sub { font-size: 1.8rem; color: var(--muted); margin-top: 0.6rem; }
  .meta { margin-top: 1.2rem; }
  p {
    margin-top: 1.2rem; font-size: 1.45rem; line-height: 1.5; color: rgb(var(--text-rgb) / 0.82);
    display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden;
  }
  .rows { top: 37rem; padding-top: 0.4rem; }
  .err { margin: 4rem 6rem; font-size: 1.8rem; color: var(--muted); }
  .end { height: 14rem; }
</style>
