<script lang="ts">
  import { tick } from 'svelte';
  import { page } from '$app/state';
  import { api, backdropUrl, cardImage, episodeLabel, img, logoUrl, progress, runtime, type Item } from '$lib/jf';
  import { refresh, stage } from '$lib/stage.svelte';
  import { restoreFocus } from '$lib/nav';
  import { play } from '$lib/player.svelte';
  import Card from '$lib/Card.svelte';
  import Row from '$lib/Row.svelte';
  import Picker from '$lib/Picker.svelte';
  import PinButton from '$lib/PinButton.svelte';
  import { itemPin } from '$lib/pins.svelte';
  import { auth } from '$lib/session.svelte';
  import { audioLabel, audioStreams, initialChoice, remember, subLabel, subStreams, type Choice } from '$lib/tracks';

  const id = $derived(page.params.id ?? '');
  let item = $state<Item | null>(null);
  let seasons = $state<Item[]>([]);
  let episodes = $state<Item[]>([]);
  let children = $state<Item[]>([]);
  let similar = $state<Item[]>([]);

  const playable = $derived(
    !!item && ['Movie', 'Episode', 'Series', 'Season', 'Audio'].includes(item.Type),
  );
  const pct = $derived(item ? progress(item) : 0);
  const people = $derived((item?.People ?? []).filter((p) => p.Type === 'Actor').slice(0, 16));
  const heading = $derived(item?.Type === 'Episode' ? (item.SeriesName ?? item.Name) : (item?.Name ?? ''));
  const logo = $derived(item && item.Type !== 'Episode' ? logoUrl(item) : null);
  const pin = $derived(item ? itemPin(item) : null);

  // audio / subtitle choice (films and episodes)
  let choice = $state<Choice | null>(null);
  let sheet = $state<'audio' | 'sub' | null>(null);
  const auds = $derived(item ? audioStreams(item) : []);
  const subs = $derived(item ? subStreams(item) : []);
  const showTracks = $derived(!!choice && (auds.length > 1 || subs.length > 0));
  const curAudio = $derived(auds.find((s) => s.Index === choice?.audio));
  const curSub = $derived(subs.find((s) => s.Index === choice?.sub));

  async function closeSheet() {
    const was = sheet;
    sheet = null;
    await tick();
    document.querySelector<HTMLElement>(`[data-key="${was}"]`)?.focus({ preventScroll: true });
  }
  function pickTrack(key: number) {
    if (!item || !choice) return;
    choice = sheet === 'audio' ? { ...choice, audio: key } : { ...choice, sub: key };
    remember(auth.id, item, choice); // episodes: the series keeps these languages
    closeSheet();
  }

  $effect(() => {
    const cur = id;
    void refresh.n;
    item = null;
    choice = null;
    sheet = null;
    seasons = [];
    episodes = [];
    children = [];
    similar = [];
    (async () => {
      const it = await api.item(cur);
      if (cur !== id) return;
      item = it;
      choice = it.Type === 'Movie' || it.Type === 'Episode' ? initialChoice(auth.id, it) : null;
      stage.backdrop = backdropUrl(it);
      const extra: Promise<void>[] = [];
      if (it.Type === 'Series') extra.push(api.seasons(cur).then((p) => void (seasons = p.Items)));
      if (it.Type === 'Season' && it.SeriesId)
        extra.push(api.episodes(it.SeriesId, cur).then((p) => void (episodes = p.Items)));
      if (it.Type === 'BoxSet' || it.Type === 'Folder')
        extra.push(api.children(cur).then((p) => void (children = p.Items)));
      if (it.Type === 'Movie' || it.Type === 'Series')
        extra.push(api.similar(cur).then((p) => void (similar = p.Items)));
      await Promise.all(extra).catch(() => {});
      await tick();
      if (cur === id) restoreFocus(page.url.pathname);
    })();
  });

  async function toggleWatched() {
    if (!item) return;
    const next = !item.UserData?.Played;
    await api.setPlayed(item.Id, next);
    item.UserData = { ...item.UserData, Played: next, PlayedPercentage: 0, PlaybackPositionTicks: 0 };
  }
  async function toggleFav() {
    if (!item) return;
    const next = !item.UserData?.IsFavorite;
    await api.setFavorite(item.Id, next);
    item.UserData = { ...item.UserData, IsFavorite: next };
  }
</script>

<div class="scroller">
  {#if item}
    <section class="info">
      {#if logo}
        <img class="logo" src={logo} alt={heading} />
      {:else}
        <h1>{heading}</h1>
      {/if}
      {#if item.Type === 'Episode'}
        <div class="sub">{episodeLabel(item)} · {item.Name}</div>
      {/if}
      <div class="meta">
        {#if item.ProductionYear}<span>{item.ProductionYear}</span>{/if}
        {#if runtime(item)}<span>{runtime(item)}</span>{/if}
        {#if item.CommunityRating}<span>★ {item.CommunityRating.toFixed(1)}</span>{/if}
        {#if item.OfficialRating}<span class="pill">{item.OfficialRating}</span>{/if}
        {#if item.Genres?.length}<span>{item.Genres.slice(0, 3).join(' · ')}</span>{/if}
      </div>
      {#if item.Overview}<p>{item.Overview}</p>{/if}
      <div class="actions">
        {#if playable}
          <button class="btn primary" data-focusable data-key="play" data-scroll-top onclick={() => play(item!, { tracks: choice ?? undefined })}>
            ▶ {pct > 0 ? 'Riprendi' : 'Riproduci'}
          </button>
        {/if}
        {#if pct > 0 && playable}
          <button class="btn icon" data-focusable data-key="restart" data-scroll-top onclick={() => play(item!, { fromStart: true, tracks: choice ?? undefined })}>
            ↺ Dall'inizio
          </button>
        {/if}
        {#if item.Type !== 'BoxSet'}
          <button class="btn icon" data-focusable data-key="watched" data-scroll-top onclick={toggleWatched}>
            {item.UserData?.Played ? '✓ Visto' : '○ Da vedere'}
          </button>
        {/if}
        <button class="btn icon" data-focusable data-key="fav" data-scroll-top onclick={toggleFav}>
          {item.UserData?.IsFavorite ? '♥ Preferito' : '♡ Preferiti'}
        </button>
        {#if pin}<PinButton {pin} />{/if}
      </div>
      {#if showTracks}
        <div class="tracks">
          <button class="btn chip" data-focusable data-key="audio" data-scroll-top onclick={() => (sheet = 'audio')}>
            <span class="k">Audio</span>
            {curAudio ? `${audioLabel(curAudio).main} · ${audioLabel(curAudio).sub}` : 'Predefinito'}
          </button>
          <button class="btn chip" data-focusable data-key="sub" data-scroll-top onclick={() => (sheet = 'sub')}>
            <span class="k">Sottotitoli</span>
            {curSub ? subLabel(curSub).main : 'Nessuno'}
          </button>
        </div>
      {/if}
    </section>

    {#if seasons.length}
      <Row title="Stagioni">
        {#each seasons as s (s.Id)}<Card item={s} />{/each}
      </Row>
    {/if}

    {#if episodes.length}
      <div class="eps">
        {#each episodes as e (e.Id)}
          <button class="ep glass" data-focusable data-key={e.Id} onclick={() => play(e)}>
            <span class="thumb">
              {#if cardImage(e, true, 24)}<img src={cardImage(e, true, 24)} alt="" loading="lazy" decoding="async" />{/if}
              {#if progress(e) > 0}<i class="bar"><b style="width:{progress(e)}%"></b></i>{/if}
            </span>
            <span class="txt">
              <strong>{e.IndexNumber ?? ''}. {e.Name}</strong>
              <small>{runtime(e)}{e.UserData?.Played ? ' · ✓ visto' : ''}</small>
              <span class="ov">{e.Overview ?? ''}</span>
            </span>
          </button>
        {/each}
      </div>
    {/if}

    {#if children.length}
      <Row title="Contenuto">
        {#each children as c (c.Id)}<Card item={c} />{/each}
      </Row>
    {/if}

    {#if people.length}
      <Row title="Cast">
        <!-- unkeyed: Jellyfin can list the same actor twice with the same role -->
        {#each people as p}
          <button class="person" data-focusable data-key={`p-${p.Id}`}>
            <span class="avatar">
              {#if p.PrimaryImageTag}<img src={img(p.Id, 'Primary', 10, p.PrimaryImageTag)} alt="" loading="lazy" decoding="async" />{:else}<em>{p.Name[0]}</em>{/if}
            </span>
            <span class="pn">{p.Name}<small>{p.Role ?? ''}</small></span>
          </button>
        {/each}
      </Row>
    {/if}

    {#if similar.length}
      <Row title="Simili">
        {#each similar as s (s.Id)}<Card item={s} />{/each}
      </Row>
    {/if}
    <div class="end"></div>
  {/if}
</div>

<!-- outside the scroller, so it stays put while the page scrolls -->
{#if sheet === 'audio'}
  <Picker
    title="Audio"
    options={auds.map((s) => ({ key: s.Index, ...audioLabel(s) }))}
    value={choice?.audio ?? null}
    onpick={pickTrack}
    onclose={closeSheet}
  />
{:else if sheet === 'sub'}
  <Picker
    title="Sottotitoli"
    options={[{ key: -1, main: 'Nessuno' }, ...subs.map((s) => ({ key: s.Index, ...subLabel(s) }))]}
    value={choice?.sub ?? -1}
    onpick={pickTrack}
    onclose={closeSheet}
  />
{/if}

<style>
  .info { padding: 17rem 6rem 3rem; width: 78rem; min-height: 56rem; }
  h1 { font-size: 5rem; font-weight: 700; letter-spacing: -0.02em; line-height: 1.05; }
  .logo { max-width: 46rem; max-height: 14rem; object-fit: contain; object-position: left bottom; display: block; }
  .sub { font-size: 1.9rem; color: var(--muted); margin-top: 0.6rem; }
  .meta { margin-top: 1.4rem; }
  p {
    margin-top: 1.4rem; font-size: 1.55rem; line-height: 1.55; color: rgba(244, 245, 250, 0.85); max-width: 66rem;
    display: -webkit-box; -webkit-line-clamp: 4; line-clamp: 4; -webkit-box-orient: vertical; overflow: hidden;
  }
  .actions { display: flex; gap: 1rem; margin-top: 2.2rem; }
  .tracks { display: flex; gap: 1rem; margin-top: 1.2rem; }
  .chip { font-size: 1.3rem; font-weight: 500; padding: 0.8rem 1.6rem; border-radius: 1.4rem; background: rgba(255, 255, 255, 0.1); }
  .chip .k { font-weight: 700; margin-right: 0.3rem; }

  .eps { display: grid; gap: 1.2rem; padding: 0 6rem 2rem; max-width: 120rem; }
  .ep { display: flex; gap: 2rem; padding: 1.2rem; border-radius: 1.6rem; text-align: left; transition: background 0.15s; }
  .ep:focus { background: rgba(255, 255, 255, 0.22); box-shadow: 0 0 0 0.25rem #fff; }
  .thumb { position: relative; flex: 0 0 22rem; aspect-ratio: 16 / 9; border-radius: 1rem; overflow: hidden; background: #1d2240; }
  .thumb img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .bar { position: absolute; left: 0; right: 0; bottom: 0; height: 0.45rem; background: rgba(0, 0, 0, 0.55); }
  .bar b { display: block; height: 100%; background: var(--accent); }
  .txt { display: grid; align-content: start; gap: 0.4rem; min-width: 0; }
  .txt strong { font-size: 1.7rem; }
  .txt small { color: var(--muted); font-size: 1.2rem; }
  .ov { font-size: 1.3rem; line-height: 1.45; color: rgba(244, 245, 250, 0.75); display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }

  .person { flex: 0 0 12rem; text-align: center; }
  .avatar { display: grid; place-items: center; width: 10rem; height: 10rem; margin: 0 auto; border-radius: 50%; overflow: hidden; background: #1d2240; position: relative; }
  .avatar img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .avatar em { font-style: normal; font-size: 4rem; font-weight: 700; opacity: 0.4; }
  .pn { display: block; margin-top: 0.7rem; font-size: 1.2rem; font-weight: 600; }
  .pn small { display: block; color: var(--muted); font-weight: 400; font-size: 1.05rem; }
  .person:focus .avatar { box-shadow: 0 0 0 0.3rem #fff; }
  .end { height: 10rem; }
</style>
