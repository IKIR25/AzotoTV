<script lang="ts">
  import { locale } from '$lib/i18n';
  import { onMount } from 'svelte';
  import { api, backdropUrl, logoUrl } from './jf';
  import { longDate, pic, saverPhotos } from './gallery.svelte';
  import { music } from './music.svelte';
  import { tween } from './tween';

  // Apple TV-style screensaver: library backdrops cross-fading, with the title
  // and a big clock, taking turns with family photos (from the albums the
  // profile picked in the Galleria, or from every album) with the date and
  // place (a photo that isn't wide enough to fill the screen shows whole,
  // over a blurred copy of itself). The fades are frame-driven (see tween.ts). No slow zoom:
  // it redrew the screen every frame for as long as the TV sat idle, and kept
  // the server hot and loud. A new picture only every 2 minutes: each change
  // keeps the NVIDIA GPU at full clocks (~25 W instead of ~5) for ~35 s (see
  // idle.svelte.ts). Shown by the layout while `saver.on`; any input ends it.

  const SHOW_MS = 120_000;
  const FADE_MS = 1600;

  /** one picture of the show */
  type Shot = { src: string; photo: boolean; whole: boolean; logo: string | null; title: string; sub: string };

  let shots = $state<Shot[]>([]);
  let n = $state(0);
  /** two layers: the one on screen and the one fading in over it */
  let layers = $state<[Shot | null, Shot | null]>([null, null]);
  let front = $state(0);
  const els: HTMLElement[] = [];
  let root: HTMLElement;
  let clock = $state('');
  let date = $state('');
  let artOk = $state(true);

  const shown = $derived(layers[front]);
  const song = $derived(music.active && !music.paused ? music.queue[music.index] : undefined);

  let stopFade: (() => void) | undefined;
  let stopShow: (() => void) | undefined;

  async function next() {
    if (!shots.length) return;
    const it = shots[n++ % shots.length];
    const back = 1 - front;
    const im = new Image();
    im.src = it.src;
    await im.decode().catch(() => {});
    layers[back] = it;
    await new Promise<void>((r) => requestAnimationFrame(() => requestAnimationFrame(() => r())));
    const el = els[back];
    if (!el) return;
    stopFade?.();
    el.style.opacity = '0';
    stopFade = tween(0, 1, FADE_MS, (v) => (el.style.opacity = String(v)), () => {
      // the old layer goes under and out of sight in the same frame
      const old = els[front];
      front = back;
      if (old) old.style.opacity = '0';
      artOk = true;
    });
  }

  async function loadShots(): Promise<Shot[]> {
    const [photos, films] = await Promise.all([
      saverPhotos(30).then((list) =>
        list.map((a) => ({
          src: pic(a.id, 'preview'),
          photo: true,
          // 16:10 and wider fill the screen; narrower ones would lose too much
          whole: !a.width || !a.height || a.width / a.height < 1.6,
          logo: null,
          title: longDate(a.taken),
          sub: [a.caption, a.place].filter(Boolean).join(' · '),
        })),
      ),
      api.random(24).then(
        (r) =>
          (r?.Items ?? []).flatMap((it) => {
            const src = backdropUrl(it);
            return src ? [{ src, photo: false, whole: false, logo: logoUrl(it, 30), title: it.Name, sub: '' }] : [];
          }),
        () => [] as Shot[],
      ),
    ]);
    // one of each in turn; whichever runs out first, the other goes on alone
    const out: Shot[] = [];
    for (let i = 0; i < Math.max(photos.length, films.length); i++) {
      if (i < films.length) out.push(films[i]);
      if (i < photos.length) out.push(photos[i]);
    }
    return out;
  }

  onMount(() => {
    const tick = () => {
      const d = new Date();
      clock = d.toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' });
      date = d.toLocaleDateString(locale, { weekday: 'long', day: 'numeric', month: 'long' });
    };
    tick();
    const clockTimer = setInterval(tick, 5000);
    stopShow = tween(0, 1, 900, (v) => (root.style.opacity = String(v)));
    loadShots().then((s) => {
      shots = s;
      next();
    });
    const timer = setInterval(next, SHOW_MS);
    return () => {
      clearInterval(clockTimer);
      clearInterval(timer);
      [stopShow, stopFade].forEach((s) => s?.());
    };
  });
</script>

<div class="saver" bind:this={root} aria-hidden="true">
  {#each layers as l, i (i)}
    {#if l}
      <div bind:this={els[i]} class="layer" class:front={front === i}>
        {#if l.whole}<img class="blur" src={l.src} alt="" decoding="sync" />{/if}
        <img class:whole={l.whole} src={l.src} alt="" decoding="sync" />
      </div>
    {/if}
  {/each}
  <div class="shade"></div>

  {#if shown}
    <div class="title">
      {#if shown.logo && artOk}
        <img src={shown.logo} alt={shown.title} onerror={() => (artOk = false)} />
      {:else}
        <span class:photo={shown.photo}>{shown.title}</span>
        {#if shown.sub}<small>{shown.sub}</small>{/if}
      {/if}
    </div>
  {/if}

  <div class="time">
    {#if song}<div class="song">♪ {song.Name}<small>{song.AlbumArtist ?? song.Artists?.[0] ?? ''}</small></div>{/if}
    <div class="clock">{clock}</div>
    <div class="date">{date}</div>
  </div>
</div>

<style>
  .saver { position: fixed; inset: 0; z-index: 50; background: #000; opacity: 0; overflow: hidden; }
  .layer { position: absolute; inset: 0; opacity: 0; z-index: 1; overflow: hidden; }
  .layer img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .layer img.whole { object-fit: contain; }
  /* scaled so the blur's soft edges fall outside the screen */
  .layer img.blur { filter: blur(4rem) brightness(0.55); transform: scale(1.2); }
  /* the incoming layer has an inline opacity from the tween and is stacked on top */
  .layer.front { opacity: 1; z-index: 0; }
  .shade {
    position: absolute; inset: 0; z-index: 2;
    background: linear-gradient(0deg, rgb(var(--shadow-rgb) / 0.7) 0%, rgb(var(--shadow-rgb) / 0.15) 35%, transparent 55%);
  }
  .title { position: absolute; z-index: 3; left: 6rem; bottom: 5.4rem; }
  .title img { max-width: 30rem; max-height: 9rem; object-fit: contain; object-position: left bottom; display: block; }
  .title span { font-size: 3rem; font-weight: 700; letter-spacing: -0.01em; }
  .title span.photo { font-size: 2.4rem; text-shadow: 0 0.1rem 0.8rem rgb(var(--shadow-rgb) / 0.7); }
  .title small { display: block; margin-top: 0.4rem; font-size: 1.5rem; color: rgb(var(--fg-rgb) / 0.8); }
  .time { position: absolute; z-index: 3; right: 6rem; bottom: 4.6rem; text-align: right; }
  .clock { font-size: 7rem; font-weight: 300; letter-spacing: -0.02em; line-height: 1; font-variant-numeric: tabular-nums; }
  .date { font-size: 1.7rem; color: rgb(var(--fg-rgb) / 0.75); margin-top: 0.6rem; }
  .date::first-letter { text-transform: uppercase; }
  .song { font-size: 1.5rem; font-weight: 600; margin-bottom: 1.2rem; }
  .song small { display: block; font-size: 1.25rem; font-weight: 400; color: rgb(var(--fg-rgb) / 0.7); }
</style>
