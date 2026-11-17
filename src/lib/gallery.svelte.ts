// Galleria: the family photos and videos in Immich (a container on the
// server). AzotoTV is Immich's "TV" user, so it sees exactly the albums
// someone shared with "TV" from the Immich app (phone or browser) and nothing
// else. The same for every profile: what a profile keeps for itself is only
// its pins.
import { invoke } from '@tauri-apps/api/core';
import { goto } from '$app/navigation';
import { api } from './jf';
import { music, playTracks, stopMusic } from './music.svelte';
import { player } from './player.svelte';
import { auth } from './session.svelte';
import { showToast } from './stage.svelte';
import type { Tile } from './Card.svelte';
import type { Pin } from './pins.svelte';
import { MONTHS, WEEKDAYS, t } from '$lib/i18n';

export interface Album {
  id: string;
  name: string;
  description: string;
  count: number;
  /** asset id of the cover */
  cover: string;
  /** YYYY-MM-DD of the oldest and newest picture */
  start: string;
  end: string;
  owner: string;
  newest_first: boolean;
}

export interface Asset {
  id: string;
  video: boolean;
  /** video length, 0 for photos */
  seconds: number;
  /** local time it was taken, YYYY-MM-DDTHH:MM:SS */
  taken: string;
  width: number;
  height: number;
  place: string;
  caption: string;
}

export interface Memory {
  year: number;
  assets: Asset[];
}

/** thumbnail: ~250 px (grids); preview: ~1440 px (full screen, backdrops) */
export const pic = (id: string, size: 'thumbnail' | 'preview' = 'thumbnail') => `im://localhost/${id}/${size}`;

export const gallery = $state({
  /** false: no `immich.json` on this device */
  enabled: true,
  loaded: false,
  albums: [] as Album[],
  /** "Accadde oggi", newest year first */
  memories: [] as Memory[],
});

/** A slideshow is running: the screensaver stays away (see the layout). */
export const slideshow = $state({ on: false });

/** The TV's date, YYYY-MM-DD. */
export function today(d = new Date()) {
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/** Albums and "Accadde oggi" for the Galleria and the home's shelf. Throws when the server doesn't answer. */
export async function loadGallery(fresh = false) {
  gallery.enabled = await invoke<boolean>('im_enabled').catch(() => false);
  if (!gallery.enabled) return;
  const albums = await invoke<Album[]>('im_albums', { fresh });
  // a slow "on this day" never holds the albums back
  Object.assign(gallery, { albums, loaded: true });
  gallery.memories = await invoke<Memory[]>('im_on_this_day', { today: today() }).catch(() => gallery.memories);
}

// ---- an album's pictures, a page at a time ----

const pages = new Map<string, { assets: Asset[]; next: number | null; at: number }>();
const PAGE_TTL = 10 * 60_000;

/** The album's pictures loaded so far (the first page if none), and whether there are more. */
export async function albumAssets(id: string): Promise<{ assets: Asset[]; more: boolean }> {
  let p = pages.get(id);
  if (!p || Date.now() - p.at > PAGE_TTL) {
    const r = await invoke<{ assets: Asset[]; next: number | null }>('im_assets', { album: id, page: 1 });
    p = { assets: r.assets, next: r.next, at: Date.now() };
    pages.set(id, p);
  }
  return { assets: p.assets, more: p.next !== null };
}

/** The next page of an album; false when there was nothing more. */
export async function moreAssets(id: string): Promise<boolean> {
  const p = pages.get(id);
  if (!p || p.next === null) return false;
  const r = await invoke<{ assets: Asset[]; next: number | null }>('im_assets', { album: id, page: p.next });
  const seen = new Set(p.assets.map((a) => a.id));
  p.assets = [...p.assets, ...r.assets.filter((a) => !seen.has(a.id))];
  p.next = r.next;
  return true;
}

// ---- words ----

const DAYS = WEEKDAYS;

function parts(iso: string) {
  const [y, m, d] = iso.slice(0, 10).split('-').map(Number);
  return { y, m, d, hm: iso.length >= 16 ? iso.slice(11, 16) : '' };
}

/** "18 ottobre 2014" */
export function longDate(iso: string) {
  if (!iso) return '';
  const { y, m, d } = parts(iso);
  return `${d} ${MONTHS[m - 1]} ${y}`;
}

/** "sabato 18 ottobre 2014 · 15:30" (midnight usually means "time unknown": left out) */
export function fullDate(iso: string) {
  if (!iso) return '';
  const { y, m, d, hm } = parts(iso);
  const wd = DAYS[new Date(y, m - 1, d).getDay()];
  return `${wd} ${d} ${MONTHS[m - 1]} ${y}${hm && hm !== '00:00' ? ` · ${hm}` : ''}`;
}

/** "ottobre 2014" */
export const monthName = (ym: string) => `${MONTHS[Number(ym.slice(5, 7)) - 1]} ${ym.slice(0, 4)}`;

/** "2009 – 2025", "2014", or "ottobre 2014" when it all fits in a month */
export function span(a: Album) {
  if (!a.start) return '';
  if (a.start.slice(0, 7) === a.end.slice(0, 7)) return monthName(a.start);
  const y1 = a.start.slice(0, 4);
  const y2 = a.end.slice(0, 4);
  return y1 === y2 ? y1 : `${y1} – ${y2}`;
}

export const yearsAgo = (year: number) => {
  const n = new Date().getFullYear() - year;
  return n === 1 ? t('A year ago') : t('{n} years ago', { n });
};

/** "12 foto · 2 video" */
export function counts(list: Asset[]) {
  const v = list.filter((a) => a.video).length;
  const photos = list.length - v;
  return [photos ? `${photos} foto` : '', v ? `${v} video` : ''].filter(Boolean).join(' · ');
}

export const albumCount = (a: Album) => (a.count === 1 ? '1 elemento' : `${a.count} elementi`);

export function clock(s: number) {
  const t = Math.round(s);
  return `${Math.floor(t / 60)}:${String(t % 60).padStart(2, '0')}`;
}

// ---- this profile's wallpaper and screensaver ----

export interface Wallpaper {
  id: string;
  width: number;
  height: number;
}

interface Prefs {
  /** the photo behind the pages that have no backdrop of their own */
  wallpaper: Wallpaper | null;
  /** albums the screensaver takes photos from (none: every album) */
  saver: string[];
}

const prefsKey = (user: string) => `azoto.galleria.${user}`;
// same cache + version scheme as pins.svelte.ts: reading never writes state
const prefsVersion = $state({ n: 0 });
let prefsCache = { user: '', prefs: { wallpaper: null, saver: [] } as Prefs };

function prefs(): Prefs {
  void prefsVersion.n;
  const user = auth.id;
  if (prefsCache.user !== user) {
    let p: Prefs = { wallpaper: null, saver: [] };
    try {
      const v = JSON.parse(localStorage.getItem(prefsKey(user)) ?? 'null');
      if (v && typeof v === 'object') {
        if (typeof v.wallpaper?.id === 'string') p.wallpaper = v.wallpaper;
        if (Array.isArray(v.saver)) p.saver = v.saver.filter((x: unknown) => typeof x === 'string');
      }
    } catch {}
    prefsCache = { user, prefs: p };
  }
  return prefsCache.prefs;
}

function savePrefs(p: Prefs) {
  prefsCache = { user: auth.id, prefs: p };
  try {
    localStorage.setItem(prefsKey(auth.id), JSON.stringify(p));
  } catch {}
  prefsVersion.n++;
}

export const wallpaper = () => prefs().wallpaper;

/** The photo as this profile's wallpaper, or no wallpaper if it already was (with a toast). */
export function toggleWallpaper(a: Asset) {
  const on = wallpaper()?.id === a.id;
  savePrefs({ ...prefs(), wallpaper: on ? null : { id: a.id, width: a.width, height: a.height } });
  showToast(on ? t('Wallpaper removed') : t('Photo set as wallpaper'));
}

export function clearWallpaper() {
  savePrefs({ ...prefs(), wallpaper: null });
  showToast(t('Wallpaper removed'));
}

/** Albums of this profile's screensaver. */
export const saverAlbums = () => prefs().saver;

export function toggleSaverAlbum(a: Album) {
  const list = saverAlbums();
  const on = list.includes(a.id);
  savePrefs({ ...prefs(), saver: on ? list.filter((x) => x !== a.id) : [...list, a.id] });
  const left = on ? list.length - 1 : list.length + 1;
  showToast(
    on
      ? left
        ? t('“{name}” removed from the screensaver', { name: a.name })
        : t('The screensaver will show photos from every album')
      : t('The screensaver will show the photos of “{name}”', { name: a.name }),
  );
}

/**
 * Photos for the screensaver (mixed with the films): from this profile's
 * albums, or from every album when it picked none. Empty without Immich.
 */
export async function saverPhotos(n: number): Promise<Asset[]> {
  const albums = saverAlbums();
  if (!(await invoke<boolean>('im_enabled').catch(() => false))) return [];
  return invoke<Asset[]>('im_random', { n, albums: albums.length ? albums : null }).catch(() => []);
}

// ---- music for the slideshow ----

/** The playlist a slideshow plays, if the profile made one with this name. */
const SHOW_PLAYLIST = 'musica per foto';

/**
 * A slideshow starts: the profile's "Musica per Foto" playlist plays on
 * shuffle, unless music is already playing (that one goes on). Returns what
 * to do when the slideshow closes: stop the music it started.
 */
export function slideshowMusic(): () => void {
  let closed = false;
  let started: Set<string> | null = null;
  if (!(music.active && !music.paused)) {
    api
      .playlists()
      .then(async (lists) => {
        const pl = lists.find((p) => p.Name.trim().toLowerCase() === SHOW_PLAYLIST);
        if (!pl || closed) return;
        const songs = (await api.playlistItems(pl.Id)).Items;
        if (!songs.length || closed || (music.active && !music.paused)) return;
        await playTracks(songs, 0, { shuffle: true, open: false });
        started = new Set(music.queue.map((x) => x.Id));
      })
      .catch(() => {});
  }
  return () => {
    closed = true;
    // only if it's still the slideshow's songs (not something picked meanwhile)
    const mine = started;
    if (mine && music.active && music.queue.length === mine.size && music.queue.every((x) => mine.has(x.Id))) stopMusic();
  };
}

// ---- playing a video ----

/** A family video in the AzotoTV player (mpv), from the start. */
export async function playVideo(a: Asset, title: string) {
  if (player.active) return;
  player.active = true;
  try {
    await invoke('im_play', { id: a.id, title, subtitle: longDate(a.taken) });
  } catch (e) {
    player.active = false;
    showToast(t('Couldn\'t play: {e}', { e }));
  }
}

// ---- cards and pins ----

export const albumPin = (a: Album): Pin => ({
  kind: 'album',
  id: a.id,
  name: a.name,
  sub: albumCount(a),
  art: a.cover ? { src: pic(a.cover, 'preview'), fit: 'cover' } : null,
});

export function albumTile(a: Album, prefix = 'al'): Tile {
  return {
    key: `${prefix}:${a.id}`,
    src: a.cover ? pic(a.cover, 'preview') : null,
    title: a.name,
    sub: [albumCount(a), span(a)].filter(Boolean).join(' · '),
    pin: albumPin(a),
    open: () => goto(`/galleria/album/${a.id}`),
  };
}

export const memoryRoute = (m: Memory) => `/galleria?oggi=${m.year}`;
