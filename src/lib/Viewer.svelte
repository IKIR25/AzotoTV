<script lang="ts">
  import { key } from './input.svelte';
  import { onMount, untrack } from 'svelte';
  import { modal } from './stage.svelte';
  import { player } from './player.svelte';
  import { tween } from './tween';
  import {
    clock, fullDate, pic, playVideo, slideshow, slideshowMusic, toggleWallpaper, wallpaper, type Asset,
  } from './gallery.svelte';

  // Full screen photos: ◀ ▶ previous / next, A shows or hides the details
  // (on a video: plays it), holding A makes the photo the profile's wallpaper
  // (or not any more), B closes. As a slideshow (`show`) it moves on by
  // itself every few seconds, skipping videos and starting over at the end,
  // with the "Musica per Foto" playlist; A pauses it. `more()` fetches the
  // next page of a big album at the end.
  let {
    assets,
    start = 0,
    show = false,
    title,
    more,
    onclose,
  }: {
    assets: Asset[];
    start?: number;
    show?: boolean;
    /** the album's name: the video's title in the player */
    title: string;
    more?: () => Promise<boolean>;
    onclose: (index: number) => void;
  } = $props();

  const STEP_MS = 7000;

  let i = $state(untrack(() => Math.min(start, assets.length - 1)));
  let running = $state(untrack(() => show));
  let details = $state(untrack(() => !show));
  const cur = $derived(assets[i]);

  // two stacked pictures: the new one is decoded, then fades in over the old
  // (frame-driven like the home's backdrop, see tween.ts)
  let layers = $state<[string | null, string | null]>([null, null]);
  let front = $state(0);
  const els: HTMLImageElement[] = [];
  let stopFade: (() => void) | undefined;
  let seq = 0;

  async function showPic(url: string) {
    const mine = ++seq;
    const back = 1 - front;
    if (!layers[front]) {
      layers[front] = url;
      return;
    }
    layers[back] = url;
    await new Promise((r) => requestAnimationFrame(r));
    await els[back]?.decode().catch(() => {});
    if (mine !== seq) return;
    stopFade?.();
    els[back].style.opacity = '0';
    els[back].style.zIndex = '2';
    els[front].style.zIndex = '1';
    stopFade = tween(0, 1, running ? 900 : 250, (v) => (els[back].style.opacity = String(v)), () => {
      front = back;
    });
  }

  $effect(() => {
    const a = cur;
    if (!a) return;
    untrack(() => showPic(pic(a.id, 'preview')));
    // the neighbours are ready before they are asked for
    for (const n of [assets[i + 1], assets[i - 1]]) if (n) new Image().src = pic(n.id, 'preview');
  });

  let loading = false;
  async function step(d: number) {
    let n = i + d;
    if (n >= assets.length && more && !loading) {
      loading = true;
      const got = await more().catch(() => false);
      loading = false;
      if (got && n < assets.length) return void (i = n);
    }
    if (n < 0) n = 0;
    if (n >= assets.length) n = running ? 0 : assets.length - 1;
    i = n;
  }

  // the slideshow: photos only, a new one every few seconds
  const nextPhoto = (from: number) => {
    for (let k = 1; k <= assets.length; k++) {
      const n = (from + k) % assets.length;
      if (!assets[n].video) return n;
    }
    return -1;
  };
  let timer: ReturnType<typeof setTimeout>;
  $effect(() => {
    void i;
    clearTimeout(timer);
    if (!running || player.active) return;
    timer = setTimeout(async () => {
      let n = nextPhoto(i);
      // at the end of what's loaded: carry on with the album's next page, if any
      if ((n === -1 || n <= i) && more) {
        const before = assets.length;
        if ((await more().catch(() => false)) && assets.length > before) n = nextPhoto(before - 1);
      }
      if (n === -1) running = false; // only videos
      else i = n;
    }, STEP_MS);
    return () => clearTimeout(timer);
  });
  $effect(() => {
    slideshow.on = running;
    return () => (slideshow.on = false);
  });

  function onmove(e: Event) {
    const dir = (e as CustomEvent<string>).detail;
    e.preventDefault();
    if (dir === 'left') step(-1);
    else if (dir === 'right') step(1);
    else details = dir === 'up';
  }

  function press() {
    if (!cur) return;
    if (cur.video) {
      running = false;
      return playVideo(cur, title);
    }
    if (running) {
      running = false;
      details = true;
    } else details = !details;
  }

  const isWall = $derived(!!cur && wallpaper()?.id === cur.id);

  function onhold(e: Event) {
    e.preventDefault();
    if (cur && !cur.video) toggleWallpaper(cur);
  }

  let stage: HTMLButtonElement;
  const close = () => onclose(i);
  onMount(() => {
    modal.close = close;
    stage.addEventListener('navmove', onmove);
    stage.addEventListener('navhold', onhold);
    stage.focus({ preventScroll: true });
    const stopMusic = untrack(() => show) ? slideshowMusic() : undefined;
    return () => {
      stopFade?.();
      stopMusic?.();
      stage.removeEventListener('navmove', onmove);
      stage.removeEventListener('navhold', onhold);
      if (modal.close === close) modal.close = null;
    };
  });
  // back from the player: the viewer takes the pad again
  $effect(() => {
    if (!player.active) stage?.focus({ preventScroll: true });
  });
</script>

<div class="viewer" data-nav-trap>
  <button class="stage" data-focusable bind:this={stage} onclick={press}>
    {#each layers as src, k (k)}
      {#if src}<img bind:this={els[k]} {src} alt="" class:front={k === front} />{/if}
    {/each}
    {#if cur?.video}
      <span class="play">
        <svg viewBox="0 0 24 24"><path d="M8 5.5v13l11-6.5z" /></svg>
        <small>{clock(cur.seconds)}</small>
      </span>
    {/if}
  </button>

  {#if cur && details}
    <div class="info">
      <div class="txt">
        <strong>{fullDate(cur.taken) || 'Data sconosciuta'}{#if isWall}<em class="wall">Sfondo</em>{/if}</strong>
        {#if cur.place || cur.caption}<span>{[cur.caption, cur.place].filter(Boolean).join(' · ')}</span>{/if}
      </div>
      <div class="hint">
        <span>{i + 1} di {assets.length}</span>
        <span><kbd>◀</kbd><kbd>▶</kbd> sfoglia</span>
        <span><kbd>{key('A')}</kbd> {cur.video ? 'guarda il video' : 'nascondi'}</span>
        {#if !cur.video}<span><kbd>{key('A')}</kbd> tieni premuto: {isWall ? 'togli lo sfondo' : 'usa come sfondo'}</span>{/if}
        <span><kbd>{key('B')}</kbd> chiudi</span>
      </div>
    </div>
  {:else if cur && running}
    <div class="corner">{fullDate(cur.taken).replace(/ · .*/, '')}</div>
  {/if}
</div>

<style>
  .viewer { position: fixed; inset: 0; z-index: 30; background: #000; }
  .stage { position: absolute; inset: 0; width: 100%; height: 100%; display: block; }
  img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: contain; z-index: 1; }
  img.front { z-index: 1; }
  .play {
    position: absolute; z-index: 3; left: 50%; top: 50%; transform: translate(-50%, -50%);
    width: 11rem; height: 11rem; border-radius: 50%; display: grid; place-items: center; align-content: center;
    background: rgba(0, 0, 0, 0.55); box-shadow: 0 0 0 0.3rem rgba(255, 255, 255, 0.85);
  }
  .play svg { width: 5rem; height: 5rem; fill: #fff; margin-left: 0.6rem; }
  .play small { font-size: 1.3rem; font-weight: 600; }
  .info {
    position: absolute; z-index: 4; left: 0; right: 0; bottom: 0; padding: 8rem 6rem 3.4rem;
    display: flex; align-items: flex-end; justify-content: space-between; gap: 4rem;
    background: linear-gradient(transparent, rgba(0, 0, 0, 0.8));
  }
  .txt { display: grid; gap: 0.5rem; min-width: 0; }
  .txt strong { font-size: 2.2rem; font-weight: 700; }
  .txt span { font-size: 1.5rem; color: rgba(255, 255, 255, 0.8); }
  .wall {
    font-style: normal; font-size: 1.1rem; font-weight: 700; vertical-align: middle;
    margin-left: 1.2rem; padding: 0.2rem 0.8rem; border-radius: 0.6rem; background: rgba(255, 255, 255, 0.2);
  }
  .hint { display: flex; gap: 2.2rem; font-size: 1.3rem; color: rgba(255, 255, 255, 0.75); white-space: nowrap; }
  kbd {
    font: inherit; font-weight: 700; font-size: 1.1rem; padding: 0.1rem 0.6rem; margin-right: 0.3rem;
    border-radius: 0.5rem; background: rgba(255, 255, 255, 0.18);
  }
  .corner {
    position: absolute; z-index: 4; right: 4rem; bottom: 3rem; font-size: 1.5rem; font-weight: 600;
    color: rgba(255, 255, 255, 0.85); text-shadow: 0 0.1rem 0.6rem rgba(0, 0, 0, 0.8);
  }
</style>
