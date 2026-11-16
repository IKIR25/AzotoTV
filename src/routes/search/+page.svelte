<script module lang="ts">
  import type { Item } from '$lib/jf';
  import type { SItem, SeerrStatus } from '$lib/seerr';

  // Kept across navigation, so "back" from a result returns to the same search.
  const memo: { q: string; lib: Item[]; req: SItem[]; status: SeerrStatus | null } = {
    q: '',
    lib: [],
    req: [],
    status: null,
  };
</script>

<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api } from '$lib/jf';
  import { MS, dlProgress, inFlight, seerr, sTitle, sYear, statusLabel, tmdbImg } from '$lib/seerr';
  import { openKeyboard, osk } from '$lib/osk.svelte';
  import { restoreFocus } from '$lib/nav';
  import Card, { type Tile } from '$lib/Card.svelte';
  import Row from '$lib/Row.svelte';

  const MIN = 2;
  let q = $state(memo.q);
  let lib = $state<Item[]>(memo.lib);
  let req = $state<SItem[]>(memo.req);
  let status = $state<SeerrStatus | null>(memo.status);
  let busy = $state(false);
  let failed = $state('');
  let input: HTMLInputElement;
  // keyboard open (docked on the left): the results go down in two columns on
  // the right, reachable from its keys (▶ off the right edge)
  const docked = $derived(osk.target?.dataset.key === 'search-input');

  /** results on their way to the library: tile key -> 0..1 (the bar under the poster) */
  let arriving = $state<Record<string, number>>({});
  const tileKey = (r: SItem) => `s-${r.mediaType}-${r.id}`;

  const tiles = $derived(
    req.map(
      (r): Tile => ({
        key: tileKey(r),
        src: tmdbImg(r.posterPath),
        title: sTitle(r),
        sub: [sYear(r), r.mediaType === 'tv' ? 'Serie' : 'Film'].filter(Boolean).join(' · '),
        badge:
          arriving[tileKey(r)] != null
            ? `In arrivo · ${Math.round(arriving[tileKey(r)] * 100)}%`
            : statusLabel(r.mediaInfo),
        progress: arriving[tileKey(r)],
        open: () => goto(`/request/${r.mediaType}/${r.id}`),
      }),
    ),
  );

  // Live search: debounced, and only the newest query's answers are shown.
  let seq = 0;
  $effect(() => {
    const query = q.trim();
    memo.q = q;
    if (query.length < MIN) {
      lib = memo.lib = [];
      req = memo.req = [];
      busy = false;
      return;
    }
    busy = true;
    const mine = ++seq;
    const t = setTimeout(async () => {
      const [l, s] = await Promise.allSettled([
        api.items({ search: query, types: 'Movie,Series', limit: 30 }),
        status?.enabled === false ? Promise.resolve(null) : seerr.search(query),
      ]);
      if (mine !== seq) return;
      lib = memo.lib = l.status === 'fulfilled' ? l.value.Items : [];
      req = memo.req =
        s.status === 'fulfilled' && s.value
          ? s.value.results.filter((r) => r.mediaType !== 'person' && r.mediaInfo?.status !== MS.AVAILABLE)
          : [];
      failed = s.status === 'rejected' ? `Seerr: ${s.reason}` : '';
      busy = false;
    }, 300);
    return () => clearTimeout(t);
  });

  // How far the requested ones are, every 5 s (a few at most: the first 6)
  $effect(() => {
    const on = req.filter((r) => inFlight(r.mediaInfo) && r.mediaType !== 'person').slice(0, 6);
    if (!on.length) return;
    let stop = false;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      const got = await Promise.all(
        on.map((r) => dlProgress(r.mediaType as 'movie' | 'tv', r.id).catch(() => null)),
      );
      if (stop) return;
      const next: Record<string, number> = {};
      got.forEach((p, i) => {
        if (p && p.stage !== 'none' && p.stage !== 'unreleased') next[tileKey(on[i])] = p.overall;
      });
      arriving = next;
      timer = setTimeout(poll, 5000);
    };
    poll();
    return () => {
      stop = true;
      clearTimeout(timer);
    };
  });

  onMount(() => {
    seerr
      .status()
      .then((s) => (status = memo.status = s))
      .catch(() => {});
    // Coming back from a result: land on it. A fresh search: start typing.
    if (memo.lib.length || memo.req.length) {
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
      placeholder="Cerca film e serie"
      aria-label="Cerca"
      autocomplete="off"
    />
  </div>

  <div class="results scroller" data-nav-peer={docked || undefined}>
    {#if lib.length}
      <Row title="Nella libreria" list={docked}>
        {#each lib as it (it.Id)}<Card list={docked} item={it} size={11} />{/each}
      </Row>
    {/if}
    {#if tiles.length}
      <Row title="Da richiedere" list={docked}>
        {#each tiles as t (t.key)}<Card list={docked} tile={t} size={11} />{/each}
      </Row>
    {/if}
    <p class="note">
      {#if q.trim().length < MIN}
        Scrivi almeno {MIN} lettere.
      {:else if busy}
        Cerco…
      {:else if !lib.length && !tiles.length}
        Nessun risultato per “{q.trim()}”.
      {/if}
      {#if failed}<br />{failed}{/if}
      {#if status && !status.enabled}<br />Seerr non è disponibile: cerco solo nella libreria.
      {:else if status && !status.user}<br />Il tuo utente non è collegato a Seerr: puoi cercare ma non richiedere.{/if}
    </p>
  </div>
</div>

<style>
  .search { position: absolute; inset: 0; }
  .bar { position: absolute; top: 10rem; left: 6rem; right: 6rem; }
  input {
    width: 100%; font: inherit; font-size: 2rem; color: var(--text); padding: 1.2rem 2rem; border-radius: 1.6rem;
    border: 0.2rem solid transparent; background: rgba(255, 255, 255, 0.1); outline: none;
  }
  input:focus { border-color: #fff; background: rgba(255, 255, 255, 0.16); }
  input::placeholder { color: var(--muted); }
  .results { top: 16.5rem; }
  /* keyboard docked on the left (see Keyboard.svelte .side): results go right */
  .docked .bar { visibility: hidden; }
  .docked .results { top: 9rem; left: 62rem; padding-right: 3rem; }
  .docked .note { margin: 1rem 0; }
  .note { margin: 1rem 6rem; font-size: 1.5rem; color: var(--muted); line-height: 1.6; }
</style>
