<script module lang="ts">
  import type { Item } from '$lib/jf';
  import type { Song } from '$lib/songs.svelte';

  // Kept across navigation, so "back" from the player returns to the same search.
  const memo: { q: string; lib: Item[]; yt: Song[]; res_q: string } = { q: '', lib: [], yt: [], res_q: '' };
</script>

<script lang="ts">
  import { t } from '$lib/i18n';
  import { onMount, tick } from 'svelte';
  import { page } from '$app/state';
  import { api } from '$lib/jf';
  import { openKeyboard, osk } from '$lib/osk.svelte';
  import { restoreFocus } from '$lib/nav';
  import { clock, playTracks } from '$lib/music.svelte';
  import PlaylistSheet from '$lib/PlaylistSheet.svelte';
  import { addToPlaylist, download, playSong, search, songCover, songItem, songs, songState, STATE_LABEL } from '$lib/songs.svelte';

  // Musica's search: songs already in the library, then songs on YouTube Music
  // (A plays them; "Scarica" puts them in the library; "＋ Playlist" too, then
  // into a playlist). From a playlist's "Aggiungi brani" (`?playlist=<id>&name=`)
  // every song gets a single "＋ Aggiungi" to that playlist.
  const MIN = 2;
  const target = $derived(page.url.searchParams.get('playlist'));
  const targetName = $derived(page.url.searchParams.get('name') ?? 'playlist');
  // `?q=`: start with a search already typed
  const given = page.url.searchParams.get('q');
  let q = $state(given ?? memo.q);
  let lib = $state<Item[]>(memo.lib);
  let yt = $state<Song[]>(memo.yt);
  let busy = $state(false);
  let failed = $state(false);
  let input: HTMLInputElement;
  // keyboard open (docked on the left): the results go down in two columns on
  // the right, reachable from its keys (▶ off the right edge); the buttons
  // shrink to their symbol
  const docked = $derived(osk.target?.dataset.key === 'search-input');
  /** what "＋ Playlist" was pressed on */
  let picking = $state<{ what: Item | Song; back: string } | null>(null);
  // songs added to the target playlist from this page (the button turns into a tick)
  let added = $state<Record<string, boolean>>({});

  let seq = 0;
  $effect(() => {
    const query = q.trim();
    memo.q = q;
    if (query.length < MIN) {
      lib = memo.lib = [];
      yt = memo.yt = [];
      memo.res_q = '';
      busy = false;
      return;
    }
    if (query === memo.res_q) return;
    busy = true;
    const mine = ++seq;
    const t = setTimeout(async () => {
      const [l, y] = await Promise.allSettled([api.searchSongs(query), search(query)]);
      if (mine !== seq) return;
      lib = memo.lib = l.status === 'fulfilled' ? l.value.Items : [];
      yt = memo.yt = y.status === 'fulfilled' ? y.value.filter((s) => !songs.library[s.id] || !lib.some((x) => x.Id === songs.library[s.id])) : [];
      failed = y.status === 'rejected';
      memo.res_q = query;
      busy = false;
    }, 600);
    return () => clearTimeout(t);
  });

  onMount(() => {
    if (memo.res_q || given) {
      tick().then(() => restoreFocus(page.url.pathname, '.results [data-focusable]'));
    } else {
      input.focus({ preventScroll: true });
      openKeyboard(input);
    }
  });

  const artistOf = (it: Item) => it.AlbumArtist ?? it.Artists?.[0] ?? '';

  async function addHere(key: string, what: Item | Song) {
    if (!target) return;
    await addToPlaylist(target, what);
    added[key] = true;
  }

  async function picked(id: string) {
    const p = picking;
    picking = null;
    if (p) await addToPlaylist(id, p.what);
    await tick();
    document.querySelector<HTMLElement>(`[data-key="${p?.back}"]`)?.focus({ preventScroll: true });
  }

  async function closeSheet() {
    const back = picking?.back;
    picking = null;
    await tick();
    document.querySelector<HTMLElement>(`[data-key="${back}"]`)?.focus({ preventScroll: true });
  }
</script>

<div class="search" class:docked>
  <div class="bar">
    <input
      bind:this={input}
      bind:value={q}
      data-focusable
      data-key="search-input"
      data-osk="side"
      placeholder={target ? t('Search songs to add to “{targetName}”', { targetName }) : t('Search for a song or an artist')}
      aria-label={t('Search')}
      autocomplete="off"
    />
  </div>

  <div class="results scroller" data-remember data-nav-peer={docked || undefined}>
    {#if lib.length}
      <h2>{t('In the library')}</h2>
      <div class="list">
        {#each lib as it, i (it.Id)}
          <div class="row">
            <button class="song" data-focusable data-key="l:{it.Id}" onclick={() => playTracks(lib, i)}>
              <span class="thumb">{#if songCover(it, 7)}<img src={songCover(it, 7)} alt="" loading="lazy" />{/if}</span>
              <span class="txt">{it.Name}<small>{[artistOf(it), it.Album].filter(Boolean).join(' · ')}</small></span>
              <span class="dur">{clock((it.RunTimeTicks ?? 0) / 1e7)}</span>
            </button>
            {#if target}
              <button class="btn act" data-focusable data-key="la:{it.Id}" onclick={() => addHere(`l:${it.Id}`, it)}>
                {added[`l:${it.Id}`] ? (docked ? '✓' : t('✓ Added')) : docked ? '＋' : t('＋ Add')}
              </button>
            {:else}
              <button class="btn act" data-focusable data-key="lp:{it.Id}" onclick={() => (picking = { what: it, back: `lp:${it.Id}` })}>{docked ? '≡+' : t('＋ Playlist')}</button>
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    {#if yt.length}
      <h2>{t('From YouTube Music')}</h2>
      <div class="list">
        {#each yt as s (s.id)}
          {@const st = songState(s.id)}
          <div class="row">
            <button class="song" data-focusable data-key="y:{s.id}" onclick={() => playSong(s)}>
              <span class="thumb sq">{#if s.cover}<img src={songCover(songItem(s), 7)} alt="" loading="lazy" />{/if}</span>
              <span class="txt">{s.title}<small>{[s.artist, s.album].filter(Boolean).join(' · ')}</small></span>
              {#if st}<span class="state" class:ok={st === 'library'} class:bad={st === 'error'}>{STATE_LABEL[st]}</span>{/if}
              <span class="dur">{clock(s.length)}</span>
            </button>
            {#if target}
              <button class="btn act" data-focusable data-key="ya:{s.id}" onclick={() => addHere(`y:${s.id}`, s)}>
                {added[`y:${s.id}`] ? (docked ? '✓' : t('✓ Added')) : docked ? '＋' : t('＋ Add')}
              </button>
            {:else}
              {#if songs.enabled && (!st || st === 'error')}
                <button class="btn act" data-focusable data-key="yd:{s.id}" onclick={() => download(s)}>{docked ? '↓' : t('↓ Download')}</button>
              {/if}
              <button class="btn act" data-focusable data-key="yp:{s.id}" onclick={() => (picking = { what: s, back: `yp:${s.id}` })}>{docked ? '≡+' : t('＋ Playlist')}</button>
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    <p class="note">
      {#if q.trim().length < MIN}
        {t('Type at least {MIN} letters.', { MIN })}
      {:else if busy}
        {t('Searching…')}
      {:else if failed && !lib.length}
        {t('YouTube Music isn\'t answering. Try again in a moment.')}
      {:else if !lib.length && !yt.length}
        {t('No songs for “{a}”.', { a: q.trim() })}
      {:else if yt.length}
        {t('YouTube songs play right away; downloaded, they stay in the library for everyone.')}
        {#if docked}<br />{t('↓ download · ≡+ to a playlist')}{/if}
      {/if}
    </p>
  </div>
</div>

{#if picking}
  <PlaylistSheet onpick={picked} onclose={closeSheet} />
{/if}

<style>
  .search { position: absolute; inset: 0; }
  .bar { position: absolute; top: 4rem; left: 6rem; right: 22rem; }
  input {
    width: 100%; font: inherit; font-size: 2rem; color: var(--text); padding: 1.2rem 2rem; border-radius: 1.6rem;
    border: 0.2rem solid transparent; background: rgba(255, 255, 255, 0.1); outline: none;
  }
  input:focus { border-color: #fff; background: rgba(255, 255, 255, 0.16); }
  input::placeholder { color: var(--muted); }
  .results { top: 11rem; padding: 0 6rem 8rem; }
  .docked .bar { visibility: hidden; }
  .docked .results { top: 9rem; left: 62rem; padding: 0 3rem 8rem 0; }
  .docked .list { display: grid; grid-template-columns: 1fr 1fr; gap: 0.4rem 1.2rem; }
  .docked .row { gap: 0.5rem; min-width: 0; }
  .docked .song { gap: 1rem; padding: 0.6rem 0.8rem; }
  .docked .dur { display: none; }
  .docked .act { padding: 0.7rem 0.9rem; }
  h2 { font-size: 1.7rem; font-weight: 600; margin: 1.4rem 0 0.6rem; }
  .row { display: flex; align-items: center; gap: 1rem; }
  .song {
    flex: 1; min-width: 0; display: flex; align-items: center; gap: 1.4rem; padding: 0.6rem 1.2rem;
    border-radius: 1.2rem; text-align: left;
  }
  .song:focus { background: rgba(255, 255, 255, 0.92); color: #10121c; }
  .song:focus small, .song:focus .dur { color: rgba(16, 18, 28, 0.7); }
  .thumb { flex: 0 0 4.6rem; height: 4.6rem; border-radius: 0.6rem; overflow: hidden; background: linear-gradient(160deg, #2a3157, #151833); }
  .thumb img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .txt { flex: 1; min-width: 0; font-size: 1.4rem; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .txt small { display: block; font-size: 1.15rem; font-weight: 400; color: var(--muted); overflow: hidden; text-overflow: ellipsis; }
  .dur { font-size: 1.2rem; color: var(--muted); font-variant-numeric: tabular-nums; }
  .state { font-size: 1.1rem; font-weight: 600; padding: 0.2rem 0.8rem; border-radius: 0.8rem; background: rgba(255, 255, 255, 0.14); white-space: nowrap; }
  .state.ok { background: rgba(80, 200, 120, 0.3); }
  .state.bad { background: rgba(224, 22, 43, 0.5); }
  .song:focus .state { background: rgba(16, 18, 28, 0.12); }
  .act { flex: 0 0 auto; font-size: 1.2rem; padding: 0.7rem 1.3rem; }
  .note { margin-top: 1.6rem; font-size: 1.3rem; color: var(--muted); }
</style>
