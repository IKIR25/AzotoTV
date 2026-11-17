<script module lang="ts">
  import type { Pin } from './pins.svelte';

  /** A card that isn't a Jellyfin item (e.g. a Seerr search result). */
  export type Tile = {
    key: string;
    src: string | null;
    title: string;
    sub: string;
    badge?: string;
    /** pixel art (a Minecraft world's icon): scaled up with sharp pixels */
    pixelated?: boolean;
    /** what holding A puts on the home (a game, a world) */
    pin?: Pin;
    /** 0..1 watched (the bar along the bottom) */
    progress?: number;
    /** a round picture (a YouTube channel's avatar) */
    round?: boolean;
    /** a portrait 9:16 picture (a YouTube Short) */
    tall?: boolean;
    open: () => void;
    /** called when the card gets focus */
    focused?: () => void;
  };
</script>

<script lang="ts">
  import { t } from '$lib/i18n';
  import { goto } from '$app/navigation';
  import { cardImage, episodeLabel, progress, type Item } from './jf';
  import { playTracks } from './music.svelte';
  import { itemPin, pinOnHold } from './pins.svelte';
  import { tween } from './tween';

  let {
    item,
    tile,
    wide = false,
    size,
    list = false,
    onfocus,
    onopen,
  }: {
    item?: Item;
    tile?: Tile;
    wide?: boolean;
    size?: number;
    /** a list row (small picture, then the text) instead of a card: search results next to the keyboard */
    list?: boolean;
    onfocus?: (it: Item) => void;
    /** replaces the default action (e.g. a song plays its whole list, not just itself) */
    onopen?: () => void;
  } = $props();

  const square = $derived(
    !!tile?.round ||
    item?.Type === 'Audio' || item?.Type === 'MusicAlbum' || item?.Type === 'MusicArtist' || item?.Type === 'Playlist',
  );
  const rem = $derived(size ?? (wide ? 24 : 14));
  const src = $derived(tile ? tile.src : cardImage(item!, wide, rem));
  const pct = $derived(item ? progress(item) : (tile?.progress ?? 0) * 100);
  // holding A: "Aggiungi alla Home" (films, series, collections, music playlists, games…)
  const pin = $derived(tile ? (tile.pin ?? null) : itemPin(item!));
  const unplayed = $derived(item?.UserData?.UnplayedItemCount ?? 0);
  // (songs count as "played" as soon as they start: no tick for music)
  const played = $derived(!!item?.UserData?.Played && item.Type !== 'Series' && item.Type !== 'Audio');
  const title = $derived(
    tile ? tile.title : item!.Type === 'Episode' ? (item!.SeriesName ?? item!.Name) : item!.Name,
  );
  const sub = $derived.by(() => {
    if (tile) return tile.sub;
    const it = item!;
    if (it.Type === 'Episode') return `${episodeLabel(it)} · ${it.Name}`;
    if (it.Type === 'Audio') return it.AlbumArtist ?? '';
    if (it.Type === 'Playlist') return it.ChildCount ? t('{ChildCount} songs', { ChildCount: it.ChildCount }) : 'Playlist';
    return it.ProductionYear?.toString() ?? '';
  });

  // Focus lift + ring are frame-driven, not CSS transitions (see tween.ts).
  // (only the card has them, a list row doesn't lift)
  let btn = $state<HTMLButtonElement>();
  let ring = $state<HTMLElement>();
  let lift = 0;
  let stop: (() => void) | undefined;
  function liftTo(to: number) {
    stop?.();
    if (!btn || !ring) return;
    const [b, r] = [btn, ring];
    if (!to) r.style.opacity = '0'; // the old ring must vanish at once
    stop = tween(lift, to, 180, (v) => {
      lift = v;
      b.style.transform = v ? `scale(${1 + 0.08 * v}) translateY(${-0.3 * v}rem)` : '';
      if (to) r.style.opacity = String(Math.min(1, v * 1.5));
    });
  }
  $effect(() => () => stop?.());

  function open() {
    if (onopen) onopen();
    else if (tile) tile.open();
    else if (item!.Type === 'Audio') playTracks([item!]);
    else if (item!.Type === 'Playlist' && item!.MediaType === 'Audio') goto(`/tracks/${item!.Id}`);
    else goto(`/item/${item!.Id}`);
  }
</script>

{#if list}
  <button
    class="item"
    class:wide
    class:square
    class:round={tile?.round}
    class:tall={tile?.tall}
    data-focusable
    data-key={tile ? tile.key : item!.Id}
    use:pinOnHold={pin}
    onclick={open}
    onfocus={() => (item ? onfocus?.(item) : tile?.focused?.())}
  >
    <span class="pic">
      {#if src}<img class:pixelated={tile?.pixelated} {src} alt="" loading="lazy" decoding="async" />{:else}<em>{title[0]}</em>{/if}
      {#if pct > 0}<i class="bar"><b style="width:{pct}%"></b></i>{/if}
      <!-- a 16:9 picture has room for the badge (a video's length), saving the text's -->
      {#if wide && tile?.badge}<span class="tag">{tile.badge}</span>{/if}
    </span>
    <span class="txt"><b>{title}</b><small>{sub}</small></span>
    {#if wide && tile?.badge}<!-- on the picture -->
    {:else if tile?.badge}<span class="pill">{tile.badge}</span>
    {:else if unplayed > 0}<span class="pill new">{unplayed}</span>
    {:else if played}<span class="pill">✓</span>{/if}
  </button>
{:else}
  <button
    class="card"
    class:wide
    class:square
    class:round={tile?.round}
    class:tall={tile?.tall}
    data-focusable
    data-key={tile ? tile.key : item!.Id}
    style="--w:{rem}rem"
    bind:this={btn}
    use:pinOnHold={pin}
    onclick={open}
    onfocus={() => {
      liftTo(1);
      if (item) onfocus?.(item);
      else tile?.focused?.();
    }}
    onblur={() => liftTo(0)}
  >
    <span class="art">
      <span class="face">
        {#if src}
          {#if tile?.pixelated}
            <!-- a small square picture: whole and sharp, over a blurred copy filling the card -->
            <img class="fill" {src} alt="" decoding="async" />
            <img class="pixelated" {src} alt="" decoding="async" />
          {:else}
            <img {src} alt="" loading="lazy" decoding="async" />
          {/if}
        {:else}
          <em>{title[0]}</em>
        {/if}
        {#if pct > 0}<i class="bar"><b style="width:{pct}%"></b></i>{/if}
        {#if tile?.badge}<span class="badge tag">{tile.badge}</span>
        {:else if unplayed > 0}<span class="badge">{unplayed}</span>
        {:else if played}<span class="badge ok">✓</span>{/if}
      </span>
      <i class="ring" bind:this={ring}></i>
    </span>
    <span class="cap">{title}<small>{sub}</small></span>
  </button>
{/if}

<style>
  .card { flex: 0 0 var(--w); width: var(--w); text-align: left; will-change: transform; }
  .art { position: relative; display: block; width: var(--w); }
  .face {
    position: relative; display: grid; place-items: center; overflow: hidden;
    width: var(--w); aspect-ratio: 2 / 3; border-radius: 1.2rem;
    background: var(--placeholder-grad);
    box-shadow: 0 0.8rem 1.6rem rgb(var(--shadow-rgb) / 0.4);
  }
  /* Focus ring + lift shadow live on their own element and only fade (opacity is
     compositor-friendly; animating a big box-shadow repaints every frame).
     Its opacity is set inline by liftTo(). */
  .ring {
    position: absolute; inset: 0; border-radius: 1.2rem; pointer-events: none;
    box-shadow: 0 0 0 0.3rem var(--ring), 0 1.6rem 3rem rgb(var(--shadow-rgb) / 0.55);
    opacity: 0;
  }
  .wide .face { aspect-ratio: 16 / 9; }
  .square .face { aspect-ratio: 1; }
  .tall .face { aspect-ratio: 9 / 16; }
  .round .face, .round .ring { border-radius: 50%; }
  .round .cap { text-align: center; }
  img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  img.fill { filter: blur(1.4rem) brightness(0.75); transform: scale(1.3); }
  img.pixelated { image-rendering: pixelated; object-fit: contain; }
  em { font-style: normal; font-size: 6rem; font-weight: 800; opacity: 0.3; }
  .bar { position: absolute; left: 0; right: 0; bottom: 0; height: 0.5rem; background: rgb(var(--shadow-rgb) / 0.55); }
  .bar b { display: block; height: 100%; background: var(--accent); }
  .badge {
    position: absolute; top: 0.7rem; right: 0.7rem; min-width: 2.4rem; height: 2.4rem; padding: 0 0.6rem;
    border-radius: 1.2rem; display: grid; place-items: center; font-size: 1.2rem; font-weight: 700;
    background: var(--accent); color: var(--on-accent);
  }
  .badge.ok { background: rgb(var(--shadow-rgb) / 0.6); }
  .badge.tag { font-size: 1rem; font-weight: 600; padding: 0 0.8rem; background: rgb(var(--scrim-rgb) / 0.78); }
  .cap { display: block; margin-top: 0.8rem; font-size: 1.25rem; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .cap small { display: block; font-weight: 400; color: var(--muted); font-size: 1.05rem; margin-top: 0.1rem; }
  .card:focus { z-index: 2; }

  .item {
    display: flex; align-items: center; gap: 1.2rem; min-width: 0; padding: 0.6rem 1rem; border-radius: 1.2rem; text-align: left;
  }
  /* no transitions here: plain state changes only (see tween.ts) */
  .item:focus { background: rgb(var(--fg-rgb) / 0.92); color: var(--focus-text); }
  .item:focus small { color: rgb(var(--focus-text-rgb) / 0.7); }
  .pic {
    position: relative; flex: 0 0 auto; height: 6rem; aspect-ratio: 2 / 3; border-radius: 0.7rem; overflow: hidden;
    display: grid; place-items: center; background: var(--placeholder-grad);
  }
  .wide .pic { height: 5.4rem; aspect-ratio: 16 / 9; }
  .pic .tag {
    position: absolute; right: 0.3rem; bottom: 0.5rem; font-size: 0.9rem; font-weight: 600; padding: 0 0.4rem;
    border-radius: 0.4rem; background: rgb(var(--scrim-rgb) / 0.8); color: var(--fg);
  }
  .square .pic { aspect-ratio: 1; }
  .tall .pic { aspect-ratio: 9 / 16; }
  .round .pic { border-radius: 50%; }
  .pic em { font-size: 2.4rem; }
  .pic .bar { height: 0.35rem; }
  .txt { flex: 1; min-width: 0; font-size: 1.3rem; line-height: 1.3; }
  .txt b { font-weight: 600; display: -webkit-box; -webkit-box-orient: vertical; -webkit-line-clamp: 2; line-clamp: 2; overflow: hidden; }
  .txt small { display: block; font-size: 1.1rem; font-weight: 400; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .pill {
    flex: 0 0 auto; font-size: 1.05rem; font-weight: 700; padding: 0.15rem 0.7rem; border-radius: 0.8rem;
    background: rgb(var(--fg-rgb) / 0.14); white-space: nowrap;
  }
  .pill.new { background: var(--accent); color: var(--on-accent); }
  .item:focus .pill { background: rgb(var(--focus-text-rgb) / 0.12); }
</style>
