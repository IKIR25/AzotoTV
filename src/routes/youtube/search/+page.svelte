<script module lang="ts">
  import type { SearchResult } from '$lib/youtube.svelte';

  // Kept across navigation, so "back" from a result returns to the same search.
  const memo: { q: string; res: SearchResult | null; res_q: string } = { q: '', res: null, res_q: '' };
</script>

<script lang="ts">
  import { t } from '$lib/i18n';
  import { onMount, tick } from 'svelte';
  import { page } from '$app/state';
  import { openKeyboard, osk } from '$lib/osk.svelte';
  import { restoreFocus } from '$lib/nav';
  import Card from '$lib/Card.svelte';
  import Row from '$lib/Row.svelte';
  import type { Tile } from '$lib/Card.svelte';
  import { channelTile, noteSearch, shortsLeft, shortTile, videoTile, yt } from '$lib/youtube.svelte';

  // YouTube search: channels, videos, and (last, within today's allowance) Shorts.
  const MIN = 2;
  let q = $state(memo.q);
  let res = $state<SearchResult | null>(memo.res);
  let busy = $state(false);
  let failed = $state(false);
  let input: HTMLInputElement;
  // keyboard open (docked on the left): the results go down in two columns on
  // the right, reachable from its keys (▶ off the right edge)
  const docked = $derived(osk.target?.dataset.key === 'search-input');
  const shorts = $derived(res ? res.shorts.slice(0, Math.min(shortsLeft(), 8)) : []);
  // a search that led to a video or a channel says what the profile likes (the recommendations use it)
  const followed = (t: Tile): Tile => ({ ...t, open: () => (noteSearch(memo.res_q), t.open()) });

  // Typing pauses longer than for the library: every query goes to YouTube.
  let seq = 0;
  $effect(() => {
    const query = q.trim();
    memo.q = q;
    if (query.length < MIN) {
      res = memo.res = null;
      busy = false;
      return;
    }
    if (memo.res && query === memo.res_q) return;
    busy = true;
    const mine = ++seq;
    const t = setTimeout(async () => {
      try {
        const r = await yt.search(query);
        if (mine !== seq) return;
        res = memo.res = r;
        memo.res_q = query;
        failed = false;
      } catch {
        if (mine === seq) failed = true;
      }
      if (mine === seq) busy = false;
    }, 600);
    return () => clearTimeout(t);
  });

  onMount(() => {
    if (memo.res) {
      tick().then(() => restoreFocus(page.url.pathname, '.results [data-focusable]'));
    } else {
      input.focus({ preventScroll: true });
      openKeyboard(input);
    }
  });
</script>

<div class="search" class:docked>
  <div class="bar">
    <input
      bind:this={input}
      bind:value={q}
      data-focusable
      data-key="search-input"
      data-osk="side"
      placeholder={t('Search YouTube')}
      aria-label={t('Search')}
      autocomplete="off"
    />
  </div>

  <div class="results scroller" data-nav-peer={docked || undefined}>
    {#if res?.channels.length}
      <Row title={t('Channels')} list={docked}>
        {#each res.channels as c (c.id)}<Card list={docked} size={10} tile={followed(channelTile(c, 'sc'))} />{/each}
      </Row>
    {/if}
    {#if res?.videos.length}
      <Row title="Video" list={docked}>
        {#each res.videos as v (v.id)}<Card list={docked} wide size={20} tile={followed(videoTile(v, 'sv'))} />{/each}
      </Row>
    {/if}
    {#if shorts.length}
      <Row title="Shorts" list={docked}>
        {#each shorts as v (v.id)}<Card list={docked} size={10} tile={shortTile(v, 'ss')} />{/each}
      </Row>
    {/if}
    <p class="note">
      {#if q.trim().length < MIN}
        {t('Type at least {MIN} letters.', { MIN })}
      {:else if busy}
        {t('Searching…')}
      {:else if failed}
        {t('YouTube isn\'t answering. Try again in a moment.')}
      {:else if res && !res.videos.length && !res.channels.length}
        {t('No results for “{a}”.', { a: q.trim() })}
      {/if}
    </p>
  </div>
</div>

<style>
  .search { position: absolute; inset: 0; }
  .bar { position: absolute; top: 4rem; left: 6rem; right: 22rem; }
  input {
    width: 100%; font: inherit; font-size: 2rem; color: var(--text); padding: 1.2rem 2rem; border-radius: 1.6rem;
    border: 0.2rem solid transparent; background: rgba(255, 255, 255, 0.1); outline: none;
  }
  input:focus { border-color: #fff; background: rgba(255, 255, 255, 0.16); }
  input::placeholder { color: var(--muted); }
  .results { top: 11rem; }
  /* keyboard docked on the left (see Keyboard.svelte .side): results go right */
  .docked .bar { visibility: hidden; }
  .docked .results { top: 9rem; left: 62rem; padding-right: 3rem; }
  .docked .note { margin: 1rem 0; }
  .note { margin: 1rem 6rem; font-size: 1.5rem; color: var(--muted); line-height: 1.6; }
</style>
