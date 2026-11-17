<script lang="ts">
  import { t, tn } from '$lib/i18n';
  import { tick } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { auth } from '$lib/session.svelte';
  import { refresh, shoulder } from '$lib/stage.svelte';
  import { restoreFocus } from '$lib/nav';
  import { key } from '$lib/input.svelte';
  import Card from '$lib/Card.svelte';
  import {
    artUrl, byRecent, giochiTab, heroUrl, lastPlayed, launch, library, loadGames, platform, PLATFORMS, scanGames, worldMode,
    type Game, type Platform,
  } from '$lib/games.svelte';
  import { gamePin, worldPin } from '$lib/pins.svelte';
  import { admin, checkAdmin } from '$lib/admin.svelte';

  // The Giochi app: a tab per console (PC, Mobile, Switch, 3DS, Wii, N64, NES, Arcade; only those
  // with games), switched with LB / RB from anywhere on the page or with A on
  // a tab, like the Jellyfin client. Each tab has its games as covers, a random
  // pick and two orders. A opens the game's page (Gioca is there, with the
  // trailer). Minecraft sits under PC in a section of its own: the game, then
  // each of this profile's worlds (A goes straight in).
  const SORTS: { label: string; order: (gs: Game[]) => Game[] }[] = [
    { label: t('Recent'), order: byRecent },
    { label: t('Name'), order: (gs) => [...gs].sort((a, b) => a.name.localeCompare(b.name)) },
  ];
  let sort = $state(0);
  let failed = $state(false);
  const tabs = $derived(PLATFORMS.filter((p) => library.games.some((g) => platform(g) === p)));
  const tab = $derived(tabs.includes(giochiTab.platform) ? giochiTab.platform : (tabs[0] ?? 'PC'));
  const inTab = $derived(library.games.filter((g) => platform(g) === tab));
  const games = $derived(SORTS[sort].order(inTab.filter((g) => g.type !== 'minecraft')));
  const minecraft = $derived(inTab.filter((g) => g.type === 'minecraft'));

  /** Opens tab `p`. From the covers, focus goes to the new tab's first cover; else it stays on the tabs. */
  async function open(p: Platform) {
    if (p === tab) return;
    const inGrid = !!document.activeElement?.closest('.grid');
    giochiTab.platform = p;
    await tick();
    const el = document.querySelector<HTMLElement>(inGrid ? '.grid [data-focusable]' : `[data-key="tab-${p}"]`);
    el?.focus({ preventScroll: true });
    if (inGrid) document.querySelector('.scroller')?.scrollTo({ top: 0 });
  }

  // LB / RB (and the remote's CH+ / CH-): previous / next tab, no wrap-around,
  // spaced out because the shoulder buttons auto-repeat while held
  let lastShoulder = 0;
  $effect(() => {
    shoulder.tab = (d) => {
      const next = tabs[tabs.indexOf(tab) + d];
      if (!next || performance.now() - lastShoulder < 350) return;
      lastShoulder = performance.now();
      void open(next);
    };
    return () => (shoulder.tab = null);
  });

  // ROMs dropped in a ROM folder since the last look show up now
  $effect(() => {
    void auth.id;
    checkAdmin();
    void scanGames();
  });

  $effect(() => {
    const user = auth.id;
    void refresh.n;
    loadGames(user).then(
      async () => {
        failed = false;
        await tick();
        restoreFocus(page.url.pathname, '.grid [data-focusable]');
      },
      () => (failed = true),
    );
  });

  function random() {
    const gs = inTab;
    if (gs.length) goto(`/game/${gs[Math.floor(Math.random() * gs.length)].id}`);
  }

  const sub = (g: Game) =>
    g.played ? t('Played {lastPlayed}', { lastPlayed: lastPlayed(g.played.last) }) : (g.genres[0] ?? g.year ?? '');
</script>

<div class="scroller">
  <header>
    <h1>{t('Games')}</h1>
    <span class="count">{inTab.length ? tn(inTab.length, '{n} game', '{n} games', { n: inTab.length }) : ''}</span>
    {#if tabs.length > 1}
      <div class="tabs">
        <kbd>{key('LB')}</kbd>
        {#each tabs as p}
          <button class="tab" class:on={p === tab} data-focusable data-key="tab-{p}" data-scroll-top onclick={() => open(p)}>{p}</button>
        {/each}
        <kbd>{key('RB')}</kbd>
      </div>
    {/if}
    <div class="row">
      <button class="btn primary" data-focusable data-key="random" data-scroll-top onclick={random} disabled={!library.games.length}>
        <svg viewBox="0 0 24 24"
          ><path
            d="M19 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V5a2 2 0 0 0-2-2zM7.5 18A1.5 1.5 0 1 1 9 16.5 1.5 1.5 0 0 1 7.5 18zm0-9A1.5 1.5 0 1 1 9 7.5 1.5 1.5 0 0 1 7.5 9zm4.5 4.5a1.5 1.5 0 1 1 1.5-1.5 1.5 1.5 0 0 1-1.5 1.5zm4.5 4.5a1.5 1.5 0 1 1 1.5-1.5 1.5 1.5 0 0 1-1.5 1.5zm0-9A1.5 1.5 0 1 1 18 7.5 1.5 1.5 0 0 1 16.5 9z"
          /></svg
        > {t('Random game')}
      </button>
      <div class="sorts">
        {#each SORTS as s, i}
          <button class="btn chip" class:sel={sort === i} data-focusable data-scroll-top onclick={() => (sort = i)}>{s.label}</button>
        {/each}
      </div>
      {#if admin.is}
        <div class="manage">
          <button class="btn chip" data-focusable data-key="add" data-scroll-top onclick={() => goto('/giochi/add')}>{t('＋ Add a game')}</button>
          <button class="btn chip" data-focusable data-key="folders" data-scroll-top onclick={() => goto('/giochi/folders')}>{t('Game folders')}</button>
        </div>
      {/if}
    </div>
  </header>

  {#if failed && !library.games.length}
    <p class="msg">{t('Couldn\'t read the game library.')}</p>
  {:else if library.user === auth.id && !library.games.length}
    <p class="msg">{admin.is ? t('No games yet: add one, or put Switch, Wii and 3DS files in a ROM folder (Game folders).') : t('No games installed.')}</p>
  {/if}

  {#if games.length}
    <div class="grid">
      {#each games as g (g.id)}
        <Card
          tile={{ key: `g-${g.id}`, src: artUrl(g, 'cover'), title: g.name, sub: sub(g), pin: gamePin(g), open: () => goto(`/game/${g.id}`) }}
        />
      {/each}
    </div>
  {/if}

  {#each minecraft as g (g.id)}
    <section>
      <h2>{g.name}</h2>
      <div class="grid wide">
        <Card
          wide
          tile={{
            key: `g-${g.id}`,
            src: artUrl(g, 'header') ?? heroUrl(g),
            title: g.name,
            sub: g.worlds.length ? t('New world · trailer') : t('Create your first world: it will show up here'),
            pin: gamePin(g),
            open: () => goto(`/game/${g.id}`),
          }}
        />
        {#each g.worlds as w (w.id)}
          <Card
            wide
            tile={{
              key: `w-${g.id}-${w.id}`,
              src: w.icon ?? heroUrl(g),
              pixelated: !!w.icon,
              title: w.name,
              sub: t('Played {lastPlayed}', { lastPlayed: lastPlayed(w.last) }),
              badge: worldMode(w),
              pin: worldPin(g, w),
              open: () => launch(g.id, null, w),
            }}
          />
        {/each}
      </div>
    </section>
  {/each}
  <div class="end"></div>
</div>

<style>
  /* own page (bare): the title sits level with the clock corner */
  header { padding: 3.8rem 6rem 1rem; display: flex; align-items: baseline; gap: 2rem; flex-wrap: wrap; }
  h1 { font-size: 3.6rem; font-weight: 700; letter-spacing: -0.02em; }
  .count { color: var(--muted); font-size: 1.5rem; }
  .tabs { flex-basis: 100%; display: flex; align-items: center; gap: 0.4rem; }
  .tab {
    font-size: 1.6rem; padding: 0.9rem 2.2rem; border-radius: 2rem; color: var(--muted);
    transition: background 0.15s, color 0.15s;
  }
  .tab.on { color: var(--text); background: rgb(var(--fg-rgb) / 0.14); }
  .tab:focus { background: rgb(var(--fg-rgb) / 0.92); color: var(--focus-text); }
  kbd {
    font: inherit; font-size: 1.1rem; font-weight: 600; color: var(--muted); padding: 0.2rem 0.6rem;
    border: 1px solid rgb(var(--fg-rgb) / 0.3); border-radius: 0.5rem; margin: 0 0.6rem;
  }
  .row { flex-basis: 100%; display: flex; align-items: center; gap: 1.6rem; margin-top: 0.6rem; }
  .row .btn.primary { font-size: 1.3rem; padding: 0.7rem 1.6rem; }
  .row svg { width: 1.6rem; height: 1.6rem; fill: currentColor; }
  .sorts, .manage { display: flex; gap: 0.8rem; }
  .manage { margin-left: auto; margin-right: 6rem; }
  .chip { font-size: 1.3rem; padding: 0.7rem 1.5rem; border-radius: 1.4rem; }
  .chip.sel { background: rgb(var(--fg-rgb) / 0.34); }
  .chip.sel:focus { background: var(--focus-bg); }
  .msg { padding: 2rem 6rem; font-size: 1.8rem; color: var(--muted); }
  h2 { font-size: 1.9rem; font-weight: 600; margin: 1.6rem 6rem 0; }
  .grid {
    display: grid; grid-template-columns: repeat(7, 14rem); gap: 2.2rem 1.4rem;
    padding: 1.4rem 6rem 2rem; justify-content: start;
  }
  .grid.wide { grid-template-columns: repeat(4, 24rem); }
  .end { height: 10rem; }
</style>
