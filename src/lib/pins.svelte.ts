// Pins: films, series, collections, music playlists, channels, games,
// Minecraft worlds, YouTube videos and channels, photo albums a profile put on the home (hold A on one, or "＋ Home" on
// its page; hold A again takes it off). The home lists them under the apps, one
// row per kind (`pinRows`). Kept on this device per Jellyfin user, like the
// tile order.
import { goto } from '$app/navigation';
import { key as btnName } from './input.svelte';
import { api, backdropUrl, episodeLabel, img, logoUrl, progress, runtime, type Item } from './jf';
import { auth } from './session.svelte';
import { hhmm, live, nowNext, onAir, watch } from './live.svelte';
import { artUrl, findGame, heroUrl, lastPlayed, launch, library, worldMode, type Game, type World } from './games.svelte';
import { showToast } from './stage.svelte';
import { progressOf, thumb } from './youtube.svelte';
import { albumCount, gallery, pic, span } from './gallery.svelte';
import { key } from './input.svelte';

export interface Pin {
  kind: 'item' | 'channel' | 'game' | 'world' | 'video' | 'ytchannel' | 'album';
  /** Jellyfin item or channel id, game id, a world's folder, a YouTube video or channel id, an Immich album id */
  id: string;
  /** a world's game */
  game?: string;
  /** Jellyfin type of an item (Movie, Series, BoxSet, Playlist) */
  type?: string;
  name: string;
  /** YouTube, albums: the line under the name (channel, subscribers; how many pictures), as it was when pinned */
  sub?: string;
  /** the picture when it was pinned: shown until the server answers */
  art?: TileArt | null;
}

/** A pin's picture on the home. */
export type TileArt = {
  src: string;
  /** cover: fills the card; contain: whole, over a blurred copy (square covers); pixelated: the same with
   *  sharp pixels (a Minecraft world's icon); logo: whole and padded on a plain card (a channel logo) */
  fit: 'cover' | 'contain' | 'pixelated' | 'logo';
  /** title art drawn over a `cover` picture that has none of its own */
  logo?: string | null;
};

export const pinKey = (p: Pin) => `pin:${p.kind}:${p.game ? `${p.game}/` : ''}${p.id}`;

/** enough for a home that stays a home */
const MAX = 24;
const storeKey = (user: string) => `azoto.pins.${user}`;

// Read from localStorage the first time a profile asks; `version` makes the
// readers reactive (the cache itself is a plain variable, so reading never
// writes state, which Svelte forbids inside templates and deriveds).
const version = $state({ n: 0 });
let cache = { user: '', list: [] as Pin[] };

function read(user: string): Pin[] {
  try {
    const v = JSON.parse(localStorage.getItem(storeKey(user)) ?? '[]');
    if (Array.isArray(v)) return v.filter((p) => p && typeof p.id === 'string' && typeof p.kind === 'string');
  } catch {}
  return [];
}

/** This profile's pins, in the order they were added. */
export function pinList(): Pin[] {
  void version.n;
  const user = auth.id;
  if (cache.user !== user) cache = { user, list: read(user) };
  return cache.list;
}

function write(list: Pin[]) {
  cache = { user: auth.id, list };
  try {
    localStorage.setItem(storeKey(auth.id), JSON.stringify(list));
  } catch {}
  version.n++;
}

export const isPinned = (p: Pin) => pinList().some((x) => pinKey(x) === pinKey(p));

/** On the home if it wasn't, off it if it was (with a toast either way). */
export function togglePin(p: Pin) {
  const key = pinKey(p);
  const list = pinList();
  if (list.some((x) => pinKey(x) === key)) {
    write(list.filter((x) => pinKey(x) !== key));
    showToast(`${p.name}: tolto dalla Home`);
  } else if (list.length >= MAX) {
    showToast(`Nella Home ci sono già ${MAX} elementi: togline uno (tieni premuto ${btnName('A')})`, 4000);
  } else {
    // (a Minecraft icon is a data: URL: not worth keeping, the library has it)
    write([...list, { ...p, art: p.art?.src.startsWith('data:') ? null : (p.art ?? null) }]);
    showToast(`${p.name}: aggiunto alla Home`);
  }
}

/** Svelte action: holding A on this element puts `pin` on the home or takes it off (null: it stays a plain A). */
export function pinOnHold(node: HTMLElement, pin: Pin | null) {
  let cur = pin;
  const onHold = (e: Event) => {
    if (!cur) return;
    e.preventDefault();
    togglePin(cur);
  };
  node.addEventListener('navhold', onHold);
  return {
    update(p: Pin | null) {
      cur = p;
    },
    destroy() {
      node.removeEventListener('navhold', onHold);
    },
  };
}

// ---- what can be pinned -------------------------------------------------------

/** The pin for a Jellyfin item, or null if it can't go on the home (episodes, songs, albums…). */
export function itemPin(it: Item): Pin | null {
  if (it.Type === 'TvChannel') return { kind: 'channel', id: it.Id, name: it.Name, art: channelArt(it) };
  const playlist = it.Type === 'Playlist' && it.MediaType === 'Audio';
  if (!playlist && !['Movie', 'Series', 'BoxSet'].includes(it.Type)) return null;
  return { kind: 'item', id: it.Id, type: it.Type, name: it.Name, art: itemArt(it) };
}

export const gamePin = (g: Game): Pin => ({ kind: 'game', id: g.id, name: g.name });
export const worldPin = (g: Game, w: World): Pin => ({ kind: 'world', id: w.id, game: g.id, name: w.name });

// ---- pictures -------------------------------------------------------------------

/** rem: a bit over the card width, so the focus lift stays sharp */
const CARD = 26;

function itemArt(it: Item): TileArt | null {
  const t = it.ImageTags ?? {};
  if (it.Type === 'Playlist') return t.Primary ? { src: img(it.Id, 'Primary', CARD, t.Primary), fit: 'contain' } : null;
  // a Thumb is the landscape poster, title included
  if (t.Thumb) return { src: img(it.Id, 'Thumb', CARD, t.Thumb), fit: 'cover' };
  const bd = it.BackdropImageTags?.[0];
  if (bd) return { src: img(it.Id, 'Backdrop', CARD, bd), fit: 'cover', logo: logoUrl(it, 14) };
  if (t.Primary) return { src: img(it.Id, 'Primary', CARD, t.Primary), fit: 'contain' };
  return null;
}

const channelArt = (ch: Item): TileArt | null =>
  ch.ImageTags?.Primary ? { src: img(ch.Id, 'Primary', 14, ch.ImageTags.Primary), fit: 'logo' } : null;

function gameArt(g: Game): TileArt | null {
  // the store header has the logo in it
  const header = artUrl(g, 'header');
  if (header) return { src: header, fit: 'cover' };
  const hero = artUrl(g, 'hero');
  if (hero) return { src: hero, fit: 'cover', logo: artUrl(g, 'logo') };
  const cover = artUrl(g, 'cover');
  return cover ? { src: cover, fit: 'contain' } : null;
}

// ---- the home's rows --------------------------------------------------------------

/** One pin on the home. A does what A on it does elsewhere: opens its page, or starts a channel / world. */
export interface PinCard {
  key: string;
  pin: Pin;
  title: string;
  sub: string;
  art: TileArt | null;
  /** 0..1: a film part-watched, a series' episode, the programme on air */
  progress?: number;
  /** behind the home while the card has focus */
  backdrop: string | null;
  ambient?: boolean;
  open: () => void;
}

export interface PinRow {
  key: string;
  title: string;
  /** the app tile the row belongs to: rows follow the tiles' order */
  app: string;
  cards: PinCard[];
}

const ROWS: Omit<PinRow, 'cards'>[] = [
  { key: 'film', title: 'Film', app: 'tv' },
  { key: 'serie', title: 'Serie TV', app: 'tv' },
  { key: 'box', title: 'Collezioni', app: 'tv' },
  { key: 'playlist', title: 'Playlist', app: 'music' },
  { key: 'canali', title: 'Canali', app: 'live' },
  { key: 'giochi', title: 'Giochi', app: 'giochi' },
  { key: 'youtube', title: 'YouTube', app: 'youtube' },
  { key: 'album', title: 'Album di foto', app: 'galleria' },
];

function rowOf(p: Pin): string {
  if (p.kind === 'channel') return 'canali';
  if (p.kind === 'game' || p.kind === 'world') return 'giochi';
  if (p.kind === 'video' || p.kind === 'ytchannel') return 'youtube';
  if (p.kind === 'album') return 'album';
  return { Movie: 'film', Series: 'serie', BoxSet: 'box', Playlist: 'playlist' }[p.type ?? ''] ?? 'film';
}

/** Jellyfin items of the pins (`loadPinned`), and the next episode of each series. */
const items = $state({
  user: '',
  loaded: false,
  byId: {} as Record<string, Item>,
  next: {} as Record<string, Item | null>,
});

const minsLeft = (pct: number, ticks: number) => `ancora ${Math.max(1, Math.round(((1 - pct) * ticks) / 600_000_000))} min`;

function itemCard(p: Pin, key: string): PinCard {
  const route = p.type === 'Playlist' ? `/tracks/${p.id}` : `/item/${p.id}`;
  const base = { key, pin: p, title: p.name, sub: '', art: p.art ?? null, backdrop: null, open: () => goto(route) };
  const x = items.user === auth.id ? items.byId[p.id] : undefined;
  if (!x) return { ...base, sub: items.user === auth.id && items.loaded ? 'Non più disponibile' : '' };
  const card: PinCard = { ...base, title: x.Name, art: itemArt(x) ?? base.art, backdrop: backdropUrl(x) };
  if (x.Type === 'Movie') {
    const pct = progress(x) / 100;
    card.progress = pct || undefined;
    card.sub = pct && x.RunTimeTicks ? minsLeft(pct, x.RunTimeTicks) : [x.ProductionYear, runtime(x)].filter(Boolean).join(' · ');
  } else if (x.Type === 'Series') {
    const ep = items.next[x.Id];
    card.sub = ep ? `${episodeLabel(ep)} · ${ep.Name}` : 'Serie TV';
    card.progress = ep ? progress(ep) / 100 || undefined : undefined;
  } else if (x.Type === 'BoxSet') {
    card.sub = x.ChildCount ? `${x.ChildCount} titoli` : 'Collezione';
  } else {
    card.sub = x.ChildCount ? `${x.ChildCount} brani` : 'Playlist';
    card.backdrop = x.ImageTags?.Primary ? img(x.Id, 'Primary', 40, x.ImageTags.Primary) : null;
    card.ambient = true;
  }
  return card;
}

function channelCard(p: Pin, key: string): PinCard {
  const ch = live.channels.find((c) => c.Id === p.id);
  if (!ch) return { key, pin: p, title: p.name, sub: '', art: p.art ?? null, backdrop: null, open: () => goto('/live') };
  const now = nowNext(ch, live.clock).now;
  const logo = ch.ImageTags?.Primary ? img(ch.Id, 'Primary', 40, ch.ImageTags.Primary) : null;
  const pic = now?.ImageTags?.Primary ? img(now.Id, 'Primary', 120, now.ImageTags.Primary) : null;
  return {
    key,
    pin: p,
    title: ch.Name,
    sub: now ? `${now.Name} · fino alle ${hhmm(now.EndDate)}` : 'In diretta',
    art: channelArt(ch) ?? p.art ?? null,
    progress: now ? onAir(now, live.clock) : undefined,
    backdrop: pic ?? logo,
    ambient: !pic,
    open: () => watch(ch),
  };
}

function gameCard(p: Pin, key: string): PinCard {
  const route = `/game/${p.id}`;
  const g = library.user === auth.id ? findGame(p.id) : null;
  if (!g) return { key, pin: p, title: p.name, sub: '', art: null, backdrop: null, open: () => goto(route) };
  return {
    key,
    pin: p,
    title: g.name,
    sub: g.played ? `Giocato ${lastPlayed(g.played.last)}` : (g.genres[0] ?? g.year ?? ''),
    art: gameArt(g),
    backdrop: heroUrl(g),
    open: () => goto(route),
  };
}

function worldCard(p: Pin, key: string): PinCard {
  const g = library.user === auth.id ? findGame(p.game ?? '') : null;
  const w = g?.worlds.find((x) => x.id === p.id);
  if (!g || !w) return { key, pin: p, title: p.name, sub: '', art: null, backdrop: null, open: () => goto(`/game/${p.game}`) };
  const hero = heroUrl(g);
  return {
    key,
    pin: p,
    title: w.name,
    sub: `${g.name} · ${worldMode(w)}`,
    art: w.icon ? { src: w.icon, fit: 'pixelated' } : hero ? { src: hero, fit: 'cover' } : null,
    backdrop: hero,
    // straight into the world, like its card in Giochi
    open: () => launch(g.id, null, w),
  };
}

/** YouTube: what was saved with the pin is all there is (no server to ask). */
function youtubeCard(p: Pin, key: string): PinCard {
  if (p.kind === 'ytchannel') {
    return { key, pin: p, title: p.name, sub: p.sub ?? 'Canale', art: p.art ?? null, backdrop: p.art?.src ?? null, ambient: true, open: () => goto(`/youtube/channel/${p.id}`) };
  }
  const pct = progressOf(p.id);
  return {
    key,
    pin: p,
    title: p.name,
    sub: p.sub ?? '',
    art: p.art ?? { src: thumb(p.id), fit: 'cover' },
    progress: pct > 0 && pct < 0.97 ? pct : undefined,
    backdrop: thumb(p.id, 'big'),
    open: () => goto(`/youtube/watch/${p.id}`),
  };
}

/** A photo album: as it is now if the Galleria has loaded, else as it was when pinned. */
function albumCard(p: Pin, key: string): PinCard {
  const a = gallery.albums.find((x) => x.id === p.id);
  const src = a?.cover ? pic(a.cover, 'preview') : (p.art?.src ?? null);
  return {
    key,
    pin: p,
    title: a?.name ?? p.name,
    sub: a ? [albumCount(a), span(a)].filter(Boolean).join(' · ') : gallery.loaded ? 'Non più condiviso' : (p.sub ?? ''),
    art: src ? { src, fit: 'cover' } : null,
    backdrop: src,
    open: () => goto(`/galleria/album/${p.id}`),
  };
}

function card(p: Pin): PinCard {
  const key = pinKey(p);
  if (p.kind === 'album') return albumCard(p, key);
  if (p.kind === 'video' || p.kind === 'ytchannel') return youtubeCard(p, key);
  if (p.kind === 'channel') return channelCard(p, key);
  if (p.kind === 'game') return gameCard(p, key);
  if (p.kind === 'world') return worldCard(p, key);
  return itemCard(p, key);
}

/** The pins in rows of one kind each, rows in the order of their app's tile (`pos`). */
export function pinRows(pos: (app: string) => number): PinRow[] {
  const list = pinList();
  return ROWS.map((r) => ({ ...r, cards: list.filter((p) => rowOf(p) === r.key).map(card) }))
    .filter((r) => r.cards.length)
    .sort((a, b) => pos(a.app) - pos(b.app));
}

/**
 * Fetch the pinned Jellyfin items (and each series' next episode). Channels and
 * games come from what the Diretta TV and Giochi apps load for the home anyway.
 * Throws when the server isn't reachable: the home retries.
 */
export async function loadPinned(user: string) {
  const ids = pinList()
    .filter((p) => p.kind === 'item')
    .map((p) => p.id);
  const byId: Record<string, Item> = {};
  const next: Record<string, Item | null> = {};
  if (ids.length) {
    const r = await api.byIds(ids);
    for (const x of r.Items) byId[x.Id] = x;
    await Promise.all(
      r.Items.filter((x) => x.Type === 'Series').map(async (s) => {
        next[s.Id] = (await api.nextUpFor(s.Id)).Items[0] ?? null;
      }),
    );
  }
  if (user !== auth.id) return;
  Object.assign(items, { user, loaded: true, byId, next });
}
