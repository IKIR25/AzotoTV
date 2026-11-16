<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { tween } from './tween';

  /** `ambient`: blurred and evenly darkened (music player) instead of a sharp backdrop */
  let { url, ambient = false }: { url: string | null; ambient?: boolean } = $props();

  // Two stacked <img> layers. The layer being shown stays FULLY opaque and the
  // incoming one fades in on top of it. (Fading both at once dips the combined
  // coverage to 75% at the midpoint and flashes the dark background through.)
  // The incoming image is decoded before its fade starts, and changes are
  // debounced so scrolling past cards doesn't start a load+fade for each one.
  // The fade is frame-driven, not a CSS transition: see tween.ts.
  let urls = $state<[string | null, string | null]>([null, null]);
  // per layer, fixed when its image is shown: an image keeps its look while it
  // fades out (switching the blur off mid-fade would flash it sharp)
  let blurred = $state<[boolean, boolean]>([false, false]);
  let front = $state(0);
  let fading = $state(false);
  const els: HTMLImageElement[] = [];

  // the whole backdrop fades in / out (frame-driven too)
  let root: HTMLElement;
  let visible = false;
  let shown = 0;
  let stopShow: (() => void) | undefined;
  function setVisible(on: boolean) {
    if (on === visible) return;
    visible = on;
    stopShow?.();
    stopShow = tween(shown, on ? 1 : 0, 350, (v) => (root.style.opacity = String((shown = v))));
  }

  const FADE_MS = 450;
  const SETTLE_MS = 180;
  let seq = 0;
  let stopFade: (() => void) | undefined;
  let settleTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    const u = url;
    clearTimeout(settleTimer);
    // first image shows immediately; later changes wait for focus to settle
    const delay = untrack(() => (urls[front] ? SETTLE_MS : 0));
    settleTimer = setTimeout(() => untrack(() => show(u)), delay);
    return () => clearTimeout(settleTimer);
  });

  async function show(u: string | null) {
    const mine = ++seq;
    if (!u) {
      setVisible(false); // whole backdrop fades out; the image stays for a quick return
      return;
    }
    setVisible(true);
    // an interrupted fade is committed, so at most two layers are ever involved
    if (fading) {
      stopFade?.();
      commit();
    }
    if (u === urls[front] && blurred[front] === ambient) return;

    const back = 1 - front;
    try {
      const im = new Image();
      im.src = u;
      await im.decode();
    } catch {
      /* show it anyway */
    }
    if (mine !== seq) return;

    urls[back] = u;
    blurred[back] = untrack(() => ambient);
    await tick();
    await new Promise<void>((r) => requestAnimationFrame(() => requestAnimationFrame(() => r())));
    if (mine !== seq) return;

    const el = els[back];
    el.style.opacity = '0';
    fading = true;
    stopFade = tween(0, 1, FADE_MS, (v) => (el.style.opacity = String(v)), commit);
  }

  /** The incoming layer becomes the front one. The inline opacity is dropped in
   *  the same task as the class swap, so no frame is painted in between. */
  function commit() {
    const el = els[1 - front];
    front = 1 - front;
    fading = false;
    el.style.opacity = '';
  }
</script>

<div class="bd" class:ambient={blurred[front]} bind:this={root} aria-hidden="true">
  {#each [0, 1] as i}
    {#if urls[i]}
      <img
        bind:this={els[i]}
        class="layer"
        class:front={front === i}
        class:incoming={front !== i && fading}
        class:blur={blurred[i]}
        src={urls[i]}
        alt=""
        decoding="sync"
      />
    {/if}
  {/each}
  <div class="shade"></div>
</div>

<style>
  /* isolation: the layers' z-indexes must stay inside this box, below the UI */
  .bd { position: absolute; inset: 0; z-index: 0; isolation: isolate; pointer-events: none; opacity: 0; }
  .layer {
    position: absolute; inset: 0; width: 100%; height: 100%;
    object-fit: cover; object-position: center top;
    opacity: 0; z-index: 0;
  }
  .layer.front { opacity: 1; z-index: 1; }
  /* opacity comes from the inline style the tween writes */
  .layer.incoming { z-index: 2; }
  /* scaled so the blur's soft edges fall outside the screen */
  .layer.blur { filter: blur(3.5rem) saturate(1.4); transform: scale(1.15); object-position: center; }
  .ambient .shade {
    background:
      radial-gradient(ellipse at 30% 45%, rgba(7, 8, 15, 0.35), rgba(7, 8, 15, 0.7) 75%),
      linear-gradient(0deg, rgba(7, 8, 15, 0.6), transparent 40%);
  }
  /* keep text legible: darken left side and bottom */
  .shade {
    position: absolute; inset: 0; z-index: 3;
    background:
      linear-gradient(90deg, rgba(7, 8, 15, 0.92) 0%, rgba(7, 8, 15, 0.55) 38%, rgba(7, 8, 15, 0.1) 70%),
      linear-gradient(0deg, rgba(7, 8, 15, 0.96) 0%, rgba(7, 8, 15, 0.5) 32%, transparent 60%);
  }
</style>
