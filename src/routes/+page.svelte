<script module lang="ts">
  // The shelf survives a trip into an app and back (same app, same slide).
  // ('' at the start: the profile's first tile)
  const saved = { app: '', index: {} as Record<string, number> };
</script>

<script lang="ts">
  import { t } from '$lib/i18n';
  import { onMount, tick, untrack } from 'svelte';
  import { afterNavigate, goto } from '$app/navigation';
  import { APPS, appOrder, saveAppOrder, type App } from '$lib/apps';
  import { installedVersion } from '$lib/apps/installed.svelte';
  import { loadPinned, pinRows, togglePin, type PinCard } from '$lib/pins.svelte';
  import Row from '$lib/Row.svelte';
  import { auth } from '$lib/session.svelte';
  import { saver } from '$lib/idle.svelte';
  import { modal, refresh, stage } from '$lib/stage.svelte';
  import { restoreFocus, reveal } from '$lib/nav';
  import { liftOnFocus, tween } from '$lib/tween';

  // AzotoTV home, tvOS-style: a row of apps at the bottom, and above it the
  // "top shelf" of the focused app filling the screen (see $lib/apps). Up from
  // the apps focuses the shelf: left / right change the slide, A opens it.
  // Holding A on a tile picks it up: left / right move it, A or B puts it down.
  // Below, a page down: what the profile pinned (see $lib/pins.svelte), one
  // row per kind; holding A on a pin takes it off the home. Which apps a
  // profile has is its own choice (the Store).

  const SLIDE_MS = 8000;

  /** this profile's apps, in its tile order (keys) */
  let order = $state<string[]>([]);
  const pos = (key: string) => order.indexOf(key);
  let shelfApp = $state(saved.app);
  let idx = $state<Record<string, number>>({ ...saved.index });
  let focusedApp = $state('');
  let artOk = $state(true);
  let editing = $state(false);

  const app = $derived(APPS.find((a) => a.key === shelfApp && order.includes(a.key)) ?? APPS.find((a) => a.key === order[0]) ?? APPS[0]);
  const slides = $derived(app.slides());
  const index = $derived(slides.length ? (idx[shelfApp] ?? 0) % slides.length : 0);
  const cur = $derived(slides[index] ?? null);

  // pins of an app the profile removed wait for it to come back
  const rows = $derived(pinRows(pos).filter((r) => order.includes(r.app)));
  /** the pin with focus (its backdrop replaces the shelf's) */
  let pinFocus = $state('');
  const pinned = $derived(pinFocus ? rows.flatMap((r) => r.cards).find((c) => c.key === pinFocus) : undefined);

  $effect(() => {
    const bg = pinned ?? cur;
    stage.backdrop = bg?.backdrop ?? null;
    stage.ambient = !!bg?.ambient;
  });
  $effect(() => {
    void cur?.key;
    artOk = true;
  });

  // at boot Jellyfin may still be starting: try again until every shelf loads
  let retry: ReturnType<typeof setTimeout> | undefined;

  // after SvelteKit's own end-of-navigation focus reset, which would undo it
  afterNavigate(() => restoreFocus('/', `[data-key="app:${order.includes(shelfApp) ? shelfApp : order[0]}"]`));

  async function load(user: string, quiet: boolean) {
    clearTimeout(retry);
    if (!quiet) order = appOrder(user).map((a) => a.key);
    // only the apps this profile has: the others' shelves aren't shown
    const mine = appOrder(user);
    const res = await Promise.allSettled([...mine.map((a) => a.load?.(user)), loadPinned(user)]);
    if (user === auth.id && res.some((r) => r.status === 'rejected')) retry = setTimeout(() => load(user, true), 5000);
  }

  // Reload on a profile switch and after playback (resume points moved). Only
  // these are dependencies: the loaders read and write shelf state themselves.
  let loadedFor = '';
  $effect(() => {
    if (auth.state !== 'in') return;
    const id = auth.id;
    void refresh.n;
    void installedVersion();
    untrack(() => {
      const quiet = loadedFor === id;
      loadedFor = id;
      // an app added from the Store's shelf joins the row at once
      order = appOrder(id).map((a) => a.key);
      // a fresh start, or a profile without the app the shelf was on: its first tile
      if (order.length && !order.includes(shelfApp)) shelfApp = shelfTarget = saved.app = order[0];
      load(id, quiet);
    });
  });

  // ---- slideshow -----------------------------------------------------------

  let info: HTMLElement;
  let stopFade: (() => void) | undefined;
  let timer: ReturnType<typeof setInterval>;

  function restartTimer() {
    clearInterval(timer);
    // not once nobody has touched the pad for a while (nor under the screensaver,
    // nor while the shelf is scrolled away for the pins): see idle.svelte.ts
    timer = setInterval(() => saver.still || pinFocus || go(1), SLIDE_MS);
  }

  /** The text fades out, `change` swaps the content, it fades back in (the backdrop cross-fades by itself). */
  function fade(change: () => void) {
    stopFade?.();
    const from = Number(info.style.opacity || 1);
    stopFade = tween(from, 0, 160, (v) => (info.style.opacity = String(v)), () => {
      change();
      stopFade = tween(0, 1, 320, (v) => (info.style.opacity = String(v)));
    });
  }

  /** Next / previous slide of the shelf on screen. */
  function go(d: number) {
    const n = slides.length;
    if (n < 2 || editing) return;
    const key = shelfApp;
    const to = (index + d + n) % n;
    fade(() => (idx[key] = saved.index[key] = to));
  }

  /** the app the shelf shows, or is fading to (the switch lands after the fade-out) */
  let shelfTarget = saved.app;

  /** A tile got focus: the shelf shows that app. */
  function showApp(key: string) {
    focusedApp = key;
    // compare with where the shelf is going, not where it is: on a quick
    // TV -> Diretta TV -> TV the second move would otherwise be dropped and
    // the pending switch would land on the wrong app
    if (key === shelfTarget) return;
    shelfTarget = key;
    fade(() => (shelfApp = saved.app = key));
    restartTimer();
  }

  // ---- rearranging -----------------------------------------------------------

  const slotEls: Record<string, HTMLElement> = {};

  function startEdit() {
    editing = true;
    modal.close = endEdit; // B puts the tile down too
  }

  function endEdit() {
    editing = false;
    modal.close = null;
    saveAppOrder(
      auth.id,
      order.flatMap((k) => APPS.find((a) => a.key === k) ?? []),
    );
  }

  /** Swap the picked tile with its neighbour; both slide to their new place. */
  async function shift(key: string, d: number) {
    const i = pos(key);
    const j = i + d;
    if (j < 0 || j >= order.length) return;
    const other = order[j];
    const before = [key, other].map((k) => slotEls[k].getBoundingClientRect().left);
    const next = [...order];
    [next[i], next[j]] = [next[j], next[i]];
    order = next;
    await tick();
    [key, other].forEach((k, n) => {
      const el = slotEls[k];
      const dx = before[n] - el.getBoundingClientRect().left;
      tween(dx, 0, 220, (v) => (el.style.transform = v ? `translateX(${v}px)` : ''));
    });
  }

  /** Tile: holding A picks it up; while picked, left / right move it and up / down do nothing. */
  function tile(node: HTMLElement, a: App) {
    const onHold = (e: Event) => {
      e.preventDefault();
      if (!editing) startEdit();
    };
    const onMove = (e: Event) => {
      if (!editing) return;
      e.preventDefault();
      const d = (e as CustomEvent<string>).detail;
      if (d === 'left' || d === 'right') shift(a.key, d === 'left' ? -1 : 1);
    };
    node.addEventListener('navhold', onHold);
    node.addEventListener('navmove', onMove);
    return {
      destroy() {
        node.removeEventListener('navhold', onHold);
        node.removeEventListener('navmove', onMove);
      },
    };
  }

  // ---- pins ------------------------------------------------------------------

  /** Pin card: holding A takes it off the home; focus moves to its neighbour, else the next row, else up. */
  function pinHold(node: HTMLElement, c: PinCard) {
    let cur = c;
    const onHold = async (e: Event) => {
      e.preventDefault();
      const r = rows.findIndex((x) => x.cards.some((y) => y.key === cur.key));
      const row = rows[r]?.cards ?? [];
      const i = row.findIndex((x) => x.key === cur.key);
      const to = row[i + 1]?.key ?? row[i - 1]?.key ?? rows[r + 1]?.cards[0]?.key ?? rows[r - 1]?.cards[0]?.key;
      togglePin(cur.pin);
      await tick();
      const el = (to && document.querySelector<HTMLElement>(`[data-key="${to}"]`)) || document.querySelector<HTMLElement>(`[data-key="app:${shelfApp}"]`);
      el?.focus({ preventScroll: true });
      if (el) reveal(el, true);
    };
    node.addEventListener('navhold', onHold);
    return {
      update(n: PinCard) {
        cur = n;
      },
      destroy() {
        node.removeEventListener('navhold', onHold);
      },
    };
  }

  let shelf: HTMLElement;
  onMount(() => {
    restartTimer();
    const onMove = (e: Event) => {
      const dir = (e as CustomEvent<string>).detail;
      if (dir !== 'left' && dir !== 'right') return;
      e.preventDefault();
      go(dir === 'left' ? -1 : 1);
      restartTimer();
    };
    shelf.addEventListener('navmove', onMove);
    return () => {
      clearInterval(timer);
      clearTimeout(retry);
      stopFade?.();
      if (editing) endEdit();
      shelf.removeEventListener('navmove', onMove);
    };
  });
</script>

<div class="home scroller">
  <!-- the first screen; with pins, a little shorter so their first row's title peeks out at the bottom -->
  <div class="top" class:peek={rows.length > 0}>
  <button class="shelf" data-focusable data-scroll-top data-key="shelf" bind:this={shelf} onclick={() => cur?.open()}>
    <div class="info" bind:this={info}>
      {#if cur}
        {#if cur.cover}<img class="cover" src={cur.cover} alt="" />{/if}
        {#if cur.kicker}
          <div class="kicker">
            {#if cur.kicker.icon}<span class="chlogo"><img src={cur.kicker.icon} alt="" /></span>{/if}
            <span>{cur.kicker.text}</span>
            {#if cur.live}<span class="onair">{t('● Live')}</span>{/if}
          </div>
        {/if}
        {#if cur.art && artOk}
          <img class="art" src={cur.art} alt={cur.title} onerror={() => (artOk = false)} />
        {:else}
          <h1 class:small={!!cur.cover}>{cur.title}</h1>
        {/if}
        <div class="meta">
          {#each cur.meta as m}<span>{m}</span>{/each}
          {#if cur.pill}<span class="pill">{cur.pill}</span>{/if}
        </div>
        {#if cur.progress != null}<i class="bar" class:live={cur.live}><em style="transform: scaleX({cur.progress})"></em></i>{/if}
        {#if cur.overview}<p>{cur.overview}</p>{/if}
      {/if}
    </div>
    {#if cur}
      <div class="nav">
        <span class="open">{cur.action}</span>
        {#if slides.length > 16}
          <span class="count">{index + 1} / {slides.length}</span>
        {:else if slides.length > 1}
          <span class="dots">
            {#each slides as s, i (s.key)}<i class:on={i === index}></i>{/each}
          </span>
        {/if}
      </div>
    {/if}
  </button>

  <div class="apps" class:editing style="--n: {order.length}">
    {#each APPS.filter((a) => order.includes(a.key)) as a, n (a.key)}
      {@const picked = editing && focusedApp === a.key}
      <div class="slot" style="order: {pos(a.key)}" bind:this={slotEls[a.key]}>
        <div class="wob" class:still={picked} style="animation-delay: {-n * 0.11}s">
          <button
            class="app"
            class:picked
            style="background: {a.color}"
            data-focusable
            data-scroll-top
            data-key="app:{a.key}"
            aria-label={a.name}
            use:liftOnFocus={1.12}
            use:tile={a}
            onfocus={() => showApp(a.key)}
            onblur={() => focusedApp === a.key && (focusedApp = '')}
            onclick={() => (editing ? endEdit() : goto(a.route))}
          >
            <svg viewBox="0 0 48 48" aria-hidden="true">{@html a.icon}</svg>
          </button>
          <span class="label" class:on={focusedApp === a.key}>{picked ? t('◀ ▶ move · A done') : a.name}</span>
        </div>
      </div>
    {/each}
  </div>
  </div>

  {#each rows as r (r.key)}
    <Row title={r.title}>
      {#each r.cards as c (c.key)}
        <button
          class="pin"
          data-focusable
          data-key={c.key}
          use:liftOnFocus={1.06}
          use:pinHold={c}
          onfocus={() => (pinFocus = c.key)}
          onblur={() => pinFocus === c.key && (pinFocus = '')}
          onclick={c.open}
        >
          <span class="pic">
            {#if c.art}
              {#if c.art.fit === 'contain' || c.art.fit === 'pixelated'}<img class="blur" src={c.art.src} alt="" />{/if}
              <img class={c.art.fit} src={c.art.src} alt="" loading="lazy" decoding="async" />
              {#if c.art.logo}<img class="title" src={c.art.logo} alt="" loading="lazy" decoding="async" />{/if}
            {:else}
              <em>{c.title[0]}</em>
            {/if}
            {#if c.progress}<i class="pbar"><b style="width: {c.progress * 100}%"></b></i>{/if}
          </span>
          <span class="cap">{c.title}<small>{c.sub}</small></span>
        </button>
      {/each}
    </Row>
  {/each}
  {#if rows.length}<div class="end"></div>{/if}
</div>

<style>
  .top { position: relative; height: 100%; }
  .top.peek { height: calc(100% - 3.4rem); }

  /* ---- top shelf ---- */
  .shelf {
    position: absolute; top: 11rem; left: 4.6rem; width: 66rem;
    padding: 1.4rem; border-radius: 2rem; text-align: left;
  }
  .shelf:focus { background: rgb(var(--fg-rgb) / 0.06); }
  .info { height: 25rem; display: flex; flex-direction: column; justify-content: flex-end; }
  .cover {
    width: 12rem; height: 12rem; object-fit: cover; border-radius: 1.2rem; margin-bottom: 1.4rem;
    box-shadow: 0 1.4rem 3rem rgb(var(--shadow-rgb) / 0.5); flex: none;
  }
  .kicker { display: flex; align-items: center; gap: 1.2rem; font-size: 1.5rem; font-weight: 600; margin-bottom: 0.8rem; }
  .chlogo {
    width: 6rem; height: 3.6rem; border-radius: 0.7rem; display: grid; place-items: center;
    background: rgb(var(--fg-rgb) / 0.12);
  }
  .chlogo img { max-width: 85%; max-height: 80%; object-fit: contain; }
  .onair { color: var(--danger); }
  .art { max-width: 40rem; max-height: 12rem; object-fit: contain; object-position: left bottom; align-self: flex-start; }
  h1 {
    font-size: 4.6rem; font-weight: 700; letter-spacing: -0.02em; line-height: 1.05; flex: none;
    display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
  }
  h1.small { font-size: 3.4rem; -webkit-line-clamp: 1; line-clamp: 1; }
  .meta { margin-top: 1.4rem; }
  .bar { display: block; width: 32rem; height: 0.5rem; border-radius: 0.25rem; margin-top: 1.2rem; background: rgb(var(--fg-rgb) / 0.2); overflow: hidden; flex: none; }
  .bar em { display: block; height: 100%; background: var(--fg); transform-origin: left; }
  .bar.live em { background: var(--danger); }
  p {
    margin-top: 1.2rem; font-size: 1.45rem; line-height: 1.5; color: rgb(var(--text-rgb) / 0.82);
    display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden;
  }
  .nav { display: flex; align-items: center; gap: 1.6rem; margin-top: 1.8rem; height: 3.4rem; }
  /* shown only while the shelf has focus: plain state change, no transition (see tween.ts) */
  .open {
    display: none; font-size: 1.4rem; font-weight: 700; padding: 0.7rem 1.8rem;
    border-radius: 1.4rem; background: var(--fg); color: var(--focus-text);
  }
  .shelf:focus .open { display: inline-block; }
  .dots { display: flex; gap: 0.6rem; }
  .dots i { width: 0.7rem; height: 0.7rem; border-radius: 50%; background: rgb(var(--fg-rgb) / 0.3); }
  .dots i.on { background: var(--fg); width: 2rem; border-radius: 0.35rem; }
  .count { font-size: 1.3rem; color: var(--muted); font-variant-numeric: tabular-nums; }

  /* ---- apps ---- */
  .apps { position: absolute; left: 6rem; right: 6rem; bottom: 6rem; display: flex; gap: 3.2rem; }
  /* the tiles stay where they are on a shorter first screen */
  .peek .apps { bottom: 2.6rem; }
  .wob { display: flex; flex-direction: column; align-items: center; }
  /* 20rem wide while they fit the row, narrower when more apps come */
  .app {
    width: min(20rem, calc((108rem - (var(--n) - 1) * 3.2rem) / var(--n))); aspect-ratio: 5 / 3; border-radius: 2.2rem; display: grid; place-items: center;
    box-shadow: inset 0 1px 0 rgb(var(--fg-rgb) / 0.35), 0 1rem 2.4rem rgb(var(--shadow-rgb) / 0.4);
  }
  .app:focus {
    box-shadow: inset 0 1px 0 rgb(var(--fg-rgb) / 0.5), 0 0 0 0.3rem var(--ring), 0 2.4rem 4.4rem rgb(var(--shadow-rgb) / 0.6);
  }
  .app.picked { box-shadow: inset 0 1px 0 rgb(var(--fg-rgb) / 0.5), 0 0 0 0.5rem var(--ring), 0 3.4rem 6rem rgb(var(--shadow-rgb) / 0.75); }
  .app :global(svg) { width: 7.4rem; height: 7.4rem; fill: none; stroke: var(--on-accent); stroke-width: 2.6; stroke-linecap: round; stroke-linejoin: round; }
  .app :global(svg .fill) { fill: var(--on-accent); stroke-width: 1.6; }
  .label { margin-top: 2.2rem; font-size: 1.6rem; font-weight: 600; visibility: hidden; }
  .label.on { visibility: visible; }

  /* rearranging: the other tiles jiggle (an endless keyframe animation, not a
     transition, so the WebKit end-of-transition glitch in tween.ts doesn't apply) */
  .editing .wob { animation: jiggle 0.26s ease-in-out infinite alternate; }
  .editing .wob.still { animation: none; }
  @keyframes jiggle { from { transform: rotate(-1.1deg); } to { transform: rotate(1.1deg); } }
  .label { white-space: nowrap; }

  /* ---- pins ---- */
  .pin { flex: 0 0 24rem; width: 24rem; text-align: left; }
  .pic {
    position: relative; display: grid; place-items: center; overflow: hidden;
    aspect-ratio: 16 / 9; border-radius: 1.4rem;
    background: var(--placeholder-grad); box-shadow: 0 0.8rem 1.6rem rgb(var(--shadow-rgb) / 0.4);
  }
  /* plain state change, no transition (see tween.ts) */
  .pin:focus .pic { box-shadow: 0 0 0 0.3rem var(--ring), 0 1.6rem 3rem rgb(var(--shadow-rgb) / 0.55); }
  .pic img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .pic img.blur { filter: blur(1.4rem) brightness(0.7); transform: scale(1.3); }
  .pic img.contain, .pic img.pixelated { object-fit: contain; }
  .pic img.pixelated { image-rendering: pixelated; }
  /* a channel logo: whole, with room around it */
  .pic img.logo { inset: 18% 15%; width: 70%; height: 64%; object-fit: contain; }
  /* title art over a picture without one */
  .pic img.title {
    inset: auto auto 1rem 1.2rem; width: 55%; height: 38%; object-fit: contain; object-position: left bottom;
    filter: drop-shadow(0 0.2rem 0.6rem rgb(var(--shadow-rgb) / 0.6));
  }
  .pic em { font-style: normal; font-size: 5rem; font-weight: 800; opacity: 0.3; }
  .pbar { position: absolute; left: 0; right: 0; bottom: 0; height: 0.5rem; background: rgb(var(--shadow-rgb) / 0.55); }
  .pbar b { display: block; height: 100%; background: var(--accent); }
  .cap { display: block; margin-top: 0.8rem; font-size: 1.35rem; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .cap small { display: block; font-weight: 400; color: var(--muted); font-size: 1.1rem; margin-top: 0.1rem; overflow: hidden; text-overflow: ellipsis; }
  .end { height: 8rem; }
</style>
