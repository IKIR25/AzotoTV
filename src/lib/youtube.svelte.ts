// The YouTube app's data. YouTube itself is only asked for public things
// (youtube.rs: search, channels, a video's related videos, channel feeds); what
// makes it personal lives here, per Jellyfin profile, with no Google account:
// subscriptions, history (resume points), likes, "Guarda più tardi", and the
// recommended feed built from them.
//
// A profile with a PIN may also link its own Google account (ytaccount.rs):
// then its YouTube home and subscriptions come first. The account is only
// read; everything above keeps going on AzotoTV alone, so the local feed is
// ready whenever the account isn't.
//
// The recommendations themselves are worked out in ytfeed.ts.
//
// Shorts are kept on a short leash (the user asked for it: no endless scroll):
// never in the feeds or on the home, only in a Shorts row of their own that
// holds as many as are left for today (SHORTS_PER_DAY, set by an admin profile),
// and a Short never leads straight into the next one.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { goto } from '$app/navigation';
import type { Tile } from './Card.svelte';
import type { Pin } from './pins.svelte';
import { player } from './player.svelte';
import { showToast } from './stage.svelte';
import { buildFeed, SHOWN_DAYS, type Shelf } from './ytfeed';
import { t } from '$lib/i18n';

export interface Video {
  id: string;
  title: string;
  channel: string;
  channel_id: string;
  /** seconds, 0 = unknown */
  length: number;
  views: string;
  /** "3 giorni fa" (or empty: see ts) */
  published: string;
  /** unix seconds (channel feeds) */
  ts: number;
  short: boolean;
  live: boolean;
}

export interface Channel {
  id: string;
  name: string;
  avatar: string | null;
  subs: string;
}

export interface VideoPage {
  video: Video;
  description: string;
  date: string;
  owner: Channel;
  related: Video[];
  shorts: Video[];
  /** first page of the comments (`yt.comments`), null = comments are off */
  comments: string | null;
}

export interface Comment {
  id: string;
  author: string;
  avatar: string | null;
  text: string;
  published: string;
  likes: string;
  replies: string;
  creator: boolean;
}

export interface Comments {
  count: string;
  comments: Comment[];
  next: string | null;
}

export interface ChannelPage {
  channel: Channel;
  banner: string | null;
  description: string;
  videos: Video[];
}

export interface SearchResult {
  videos: Video[];
  shorts: Video[];
  channels: Channel[];
}

// ---- pictures (through the yt:// disk cache) ----

export const thumb = (id: string, size: 'card' | 'big' | 'short' = 'card') =>
  `yt://localhost/vi/${id}/${size === 'big' ? 'maxresdefault' : size === 'short' ? 'oardefault' : 'mqdefault'}.jpg`;
export const ytImage = (url: string | null | undefined) => (url ? `yt://localhost/av/${encodeURIComponent(url)}` : null);

// ---- YouTube itself ----

const pages = new Map<string, { t: number; p: VideoPage }>();
const searches = new Map<string, { t: number; r: SearchResult }>();
const PAGE_TTL = 30 * 60_000;

export const yt = {
  async search(query: string, recent = false): Promise<SearchResult> {
    const key = `${recent ? 'w' : 'a'}:${query}`;
    const hit = searches.get(key);
    if (hit && Date.now() - hit.t < PAGE_TTL) return hit.r;
    const r = await invoke<SearchResult>('yt_search', { query, recent });
    if (searches.size > 60) searches.delete(searches.keys().next().value!);
    searches.set(key, { t: Date.now(), r });
    return r;
  },
  async video(id: string): Promise<VideoPage> {
    const hit = pages.get(id);
    if (hit && Date.now() - hit.t < PAGE_TTL) return hit.p;
    const p = await invoke<VideoPage>('yt_video', { id });
    if (pages.size > 120) pages.delete(pages.keys().next().value!);
    pages.set(id, { t: Date.now(), p });
    return p;
  },
  channel: (id: string) => invoke<ChannelPage>('yt_channel', { id }),
  comments: (token: string) => invoke<Comments>('yt_comments', { token }),
  feed: (channels: string[], shorts: boolean) =>
    channels.length ? invoke<Video[]>('yt_feed', { channels, shorts }) : Promise.resolve([] as Video[]),
};

// ---- the profile's own data ----

export interface Watched {
  v: Video;
  /** when, ms */
  at: number;
  /** seconds reached, and the video's length as the player saw it */
  pos: number;
  dur: number;
}

interface Store {
  subs: Channel[];
  history: Watched[];
  later: Video[];
  liked: Video[];
  /** searches that led somewhere (a result was opened), newest first */
  searches: { q: string; at: number }[];
  /** "Non mi interessa", newest first */
  nope: { v: Video; at: number }[];
  /** recommendations looked at and passed over: id → [times, last time (ms)] */
  shown: Record<string, [number, number]>;
  /** Shorts played today, and the ones seen lately (not offered again) */
  shorts: { day: string; n: number; seen: string[] };
}

const empty = (): Store => ({
  subs: [],
  history: [],
  later: [],
  liked: [],
  searches: [],
  nope: [],
  shown: {},
  shorts: { day: '', n: 0, seen: [] },
});
const MAX_HISTORY = 300;

export const me = $state<{ user: string } & Store>({ user: '', ...empty() });

const storeKey = (user: string) => `azoto.yt.${user}`;

/** Switch to (or reload) a profile's data. */
export function useProfile(user: string) {
  if (me.user === user) return;
  save(); // what the last profile did a moment ago (`saveSoon`)
  let s = empty();
  try {
    s = { ...s, ...JSON.parse(localStorage.getItem(storeKey(user)) ?? '{}') };
  } catch {}
  Object.assign(me, { user, ...s });
}

let saveTimer: ReturnType<typeof setTimeout>;

function save() {
  clearTimeout(saveTimer);
  if (!me.user) return;
  const { user, ...s } = me;
  try {
    localStorage.setItem(storeKey(user), JSON.stringify(s));
  } catch {}
}

/** For what changes while scrolling (see `noteShown`): written a moment later, once. */
function saveSoon() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(save, 3000);
}

export const isSubscribed = (id: string) => me.subs.some((c) => c.id === id);
export const isLiked = (id: string) => me.liked.some((v) => v.id === id);
export const isLater = (id: string) => me.later.some((v) => v.id === id);
export const isNope = (id: string) => me.nope.some((n) => n.v.id === id);

export function toggleSub(ch: Channel) {
  if (isSubscribed(ch.id)) {
    me.subs = me.subs.filter((c) => c.id !== ch.id);
    showToast(t('Unsubscribed from {name}', { name: ch.name }));
  } else {
    me.subs = [{ ...ch }, ...me.subs];
    showToast(t('Subscribed to {name}', { name: ch.name }));
  }
  save();
  feed.at = 0; // the subscriptions row changes
}

export function toggleLike(v: Video) {
  me.liked = isLiked(v.id) ? me.liked.filter((x) => x.id !== v.id) : [{ ...v }, ...me.liked].slice(0, 100);
  save();
}

/** "Non mi interessa": the video is never recommended again, its channel less. */
export function toggleNope(v: Video) {
  if (isNope(v.id)) {
    me.nope = me.nope.filter((n) => n.v.id !== v.id);
    showToast(t('Back among the recommendations'));
  } else {
    me.nope = [{ v: { ...v }, at: Date.now() }, ...me.nope].slice(0, 200);
    showToast(v.channel ? t('I won\'t recommend it again, and fewer videos from {channel}', { channel: v.channel }) : t('I won\'t recommend it again'));
    // gone at once from what's on screen, before the feed is rebuilt
    feed.recommended = feed.recommended.filter((x) => x.id !== v.id);
    feed.more = feed.more.map((m) => ({ ...m, videos: m.videos.filter((x) => x.id !== v.id) }));
  }
  save();
}

/** A search the profile followed (opened one of its results): it tells what the profile likes. */
export function noteSearch(q: string) {
  const query = q.trim().toLowerCase().replace(/\s+/g, ' ');
  if (query.length < 2) return;
  me.searches = [{ q: query, at: Date.now() }, ...me.searches.filter((s) => s.q !== query)].slice(0, 15);
  save();
}

/** ids already counted for the feed on screen (once per build: coming back to the page isn't seeing it again) */
let counted = { at: -1, ids: new Set<string>() };

/** A recommendation got focus: looked at. Passed over a few times, it sinks, then goes (ytfeed.ts). */
export function noteShown(id: string) {
  if (counted.at !== feed.at) counted = { at: feed.at, ids: new Set() };
  if (counted.ids.has(id)) return;
  counted.ids.add(id);
  const [n, last] = me.shown[id] ?? [0, 0];
  const stale = Date.now() - last > SHOWN_DAYS * 86_400_000;
  me.shown[id] = [stale ? 1 : n + 1, Date.now()];
  saveSoon();
}

/** Opened: not passed over after all. */
function noteOpened(id: string) {
  if (!me.shown[id]) return;
  delete me.shown[id];
  saveSoon();
}

/** what `shown` keeps: the last few days, and not forever many */
function pruneShown() {
  const since = Date.now() - SHOWN_DAYS * 86_400_000;
  const kept = Object.entries(me.shown)
    .filter(([, [, last]]) => last > since)
    .sort((a, b) => b[1][1] - a[1][1])
    .slice(0, 600);
  me.shown = Object.fromEntries(kept);
}

export function toggleLater(v: Video) {
  if (isLater(v.id)) {
    me.later = me.later.filter((x) => x.id !== v.id);
    showToast(t('Removed from Watch later'));
  } else {
    me.later = [...me.later, { ...v }];
    showToast(t('Added to Watch later'));
  }
  save();
}

/** Watched to (nearly) the end. */
const finished = (w: Watched) => w.dur > 0 && w.pos >= w.dur - Math.min(30, w.dur * 0.1);
export const watched = (id: string) => me.history.find((w) => w.v.id === id) ?? null;
/** Where to pick a video up again (0 = from the start). */
export function resumeAt(id: string): number {
  const w = watched(id);
  return w && !finished(w) && w.pos > 20 ? w.pos : 0;
}
/** 0..1 for the card's bar (0 = never started) */
export function progressOf(id: string): number {
  const w = watched(id);
  return w && w.dur > 0 ? Math.min(1, w.pos / w.dur) : 0;
}
/**
 * Start this profile's YouTube over: subscriptions, history (and so the
 * recommendations), likes, searches, "Non mi interessa" and Guarda più tardi.
 * Today's Shorts count stays: starting over isn't a way to get more of them.
 */
export function resetProfile() {
  me.subs = [];
  me.history = [];
  me.liked = [];
  me.later = [];
  me.searches = [];
  me.nope = [];
  me.shown = {};
  me.shorts = { ...me.shorts, seen: [] };
  save();
  feed.at = 0;
  showToast(t('YouTube starts from scratch'));
}

export const continueWatching = () => me.history.filter((w) => !finished(w) && w.pos > 20).map((w) => w.v);

// ---- Shorts budget ----

/** Shorts last about a minute: the default 60 is about an hour a day */
export const SHORTS_CHOICES = [0, 10, 20, 30, 60];
export const SHORTS_DEFAULT = 60;
const LIMIT_KEY = 'azoto.yt.shortsPerDay';
const today = () => new Date().toLocaleDateString('sv'); // 2026-10-18, local time

/** Shorts a profile may play per day (the same for everyone on this TV; admins change it). */
export const shortsLimit = $state({
  n: (() => {
    try {
      const v = Number(localStorage.getItem(LIMIT_KEY));
      return localStorage.getItem(LIMIT_KEY) !== null && Number.isFinite(v) ? v : SHORTS_DEFAULT;
    } catch {
      return SHORTS_DEFAULT;
    }
  })(),
});

export function setShortsLimit(n: number) {
  shortsLimit.n = n;
  try {
    localStorage.setItem(LIMIT_KEY, String(n));
  } catch {}
}

export function shortsLeft(): number {
  const used = me.shorts.day === today() ? me.shorts.n : 0;
  return Math.max(0, shortsLimit.n - used);
}

// ---- playback ----

let current: Video | null = null;
/** where `current` started, and whether it is already a second try */
let started = { at: 0, retried: false };

/** Play a video in the AzotoTV player, from where this profile left it. */
export async function watch(v: Video, opts: { fromStart?: boolean } = {}) {
  if (player.active) return;
  if (v.short) return playShort(v);
  const start = opts.fromStart ? 0 : resumeAt(v.id);
  await start_(v, start);
}

/** A Short, if any are left for today; it counts as soon as it starts. */
export async function playShort(v: Video) {
  if (player.active) return;
  if (shortsLeft() <= 0) return showToast(t('No more Shorts for today'));
  const day = today();
  me.shorts = {
    day,
    n: (me.shorts.day === day ? me.shorts.n : 0) + 1,
    seen: [v.id, ...me.shorts.seen.filter((id) => id !== v.id)].slice(0, 300),
  };
  save();
  await start_(v, 0);
}

async function start_(v: Video, start: number, retry = false) {
  player.active = true;
  current = v;
  started = { at: start, retried: retry };
  try {
    await invoke('yt_play', {
      id: v.id,
      title: v.title,
      subtitle: [v.channel, v.short ? 'Short' : ''].filter(Boolean).join(' · '),
      start,
    });
  } catch (e) {
    player.active = false;
    current = null;
    showToast(t('Couldn\'t play: {e}', { e }));
  }
}

interface Ended {
  item_id: string;
  reason: string;
  position_ticks: number;
  duration_ticks: number;
}

/** Call once at startup: note how far each video got. (player.svelte.ts clears `player.active`.) */
export function initYoutube() {
  const un = listen<Ended>('player-ended', (e) => {
    const v = current;
    if (!v || e.payload.item_id !== `yt:${v.id}`) return;
    current = null;
    // YouTube now and then refuses a stream link it just handed out: try once more
    if (e.payload.reason === 'error' && e.payload.position_ticks / 1e7 <= started.at + 1 && !started.retried) {
      showToast(t('Trying again…'));
      return void start_(v, started.at, true);
    }
    if (v.short) return; // Shorts stay out of the history (and so out of the recommendations)
    const pos = e.payload.position_ticks / 1e7;
    const dur = e.payload.duration_ticks / 1e7 || v.length;
    if (e.payload.reason === 'error' || pos < 5) return;
    me.history = [{ v: { ...v, length: v.length || Math.round(dur) }, at: Date.now(), pos, dur }, ...me.history.filter((w) => w.v.id !== v.id)].slice(
      0,
      MAX_HISTORY,
    );
    save();
  });
  return () => void un.then((f) => f());
}

// ---- the feeds ----

/** What the YouTube page and the home's shelf show, for `user`. */
export const feed = $state({
  user: '',
  /** newest normal videos of the subscriptions */
  subs: [] as Video[],
  recommended: [] as Video[],
  /** "Perché hai guardato «…»" rows */
  more: [] as Shelf[],
  /** Shorts to offer (the row shows only as many as are left today) */
  shorts: [] as Video[],
  /** when it was built (ms), and on what the profile had done by then */
  at: 0,
  sig: '',
  loading: false,
});

const FEED_TTL = 30 * 60_000;

/** what the feed is built on, short: a new watch, like, search or "Non mi interessa" rebuilds it */
const signature = () => [me.history[0]?.at ?? 0, me.history.length, me.liked.length, me.nope.length, me.searches[0]?.at ?? 0].join();

/**
 * Build the profile's feeds (ytfeed.ts), and fetch its Google account's, if
 * linked. Never throws (the home retries loaders that do, and YouTube being
 * away isn't worth a retry loop).
 */
export async function loadYoutube(user: string, force = false) {
  useProfile(user);
  await Promise.all([buildLocal(user, force), loadAccount(user, force)]);
}

async function buildLocal(user: string, force: boolean) {
  const fresh = feed.user === user && Date.now() - feed.at < FEED_TTL && feed.sig === signature();
  if ((fresh && !force) || feed.loading) return;
  feed.loading = true;
  try {
    pruneShown();
    const sig = signature();
    const built = await buildFeed($state.snapshot(me), yt);
    if (me.user !== user) return; // switched profile meanwhile
    Object.assign(feed, { user, ...built, at: Date.now(), sig });
  } finally {
    feed.loading = false;
  }
}

// ---- the profile's Google account (optional) ----

export interface AccountInfo {
  name: string;
  /** "@nome" */
  handle: string;
  photo: string | null;
}

/** The active profile's linked account and what it brings (all empty when there's none). */
export const account = $state({
  user: '',
  info: null as AccountInfo | null,
  /** its YouTube home, shelf by shelf (no Shorts) */
  home: [] as Shelf[],
  /** newest videos of its subscriptions */
  subs: [] as Video[],
  /** the Shorts of its home: offered first, within today's Shorts */
  shorts: [] as Video[],
  at: 0,
  /** the account is linked but didn't answer */
  error: '',
  loading: false,
});

const noAccount = { info: null, home: [], subs: [], shorts: [], at: 0, error: '' };

async function loadAccount(user: string, force = false) {
  if (account.user !== user) Object.assign(account, { user, ...noAccount });
  const info = await invoke<AccountInfo | null>('yt_account').catch(() => null);
  if (account.user !== user) return;
  if (!info) return void Object.assign(account, noAccount);
  account.info = info;
  if ((!force && Date.now() - account.at < FEED_TTL) || account.loading) return;
  account.loading = true;
  try {
    const [home, subs] = await Promise.all([
      invoke<{ shelves: Shelf[]; shorts: Video[] }>('yt_account_home'),
      invoke<Video[]>('yt_account_subs'),
    ]);
    if (account.user !== user) return;
    Object.assign(account, { home: home.shelves, shorts: home.shorts, subs, at: Date.now(), error: '' });
  } catch (e) {
    if (account.user !== user) return;
    if (String(e) === 'revoked') {
      // removed from the Google account's side: the profile is back to AzotoTV's own feed
      Object.assign(account, noAccount);
      showToast(t('The Google account no longer gives access: link it again from YouTube › Account'), 6000);
    } else {
      account.error = String(e);
    }
  } finally {
    account.loading = false;
  }
}

/** Unlink the active profile's account (Google forgets the access too). */
export async function unlinkAccount() {
  await invoke('yt_unlink');
  Object.assign(account, noAccount);
  feed.at = 0;
}

/** Linked just now (the sign-in page got the `yt-account` event). */
export async function accountLinked(user: string) {
  account.at = 0;
  await loadAccount(user, true);
}

/**
 * "Nuovi dalle iscrizioni": the account's, then the channels followed only on
 * AzotoTV (their videos the account's list doesn't have).
 */
export function newFromSubs(): Video[] {
  if (!account.info || !account.subs.length) return feed.subs;
  const have = new Set(account.subs.map((v) => v.id));
  const channels = new Set(account.subs.map((v) => v.channel_id).filter(Boolean));
  return [...account.subs, ...feed.subs.filter((v) => !have.has(v.id) && !channels.has(v.channel_id))];
}

/** Shorts to offer: the account's first, then AzotoTV's own. */
export function shortsPool(): Video[] {
  const seen = new Set<string>();
  return [...(account.info ? account.shorts : []), ...feed.shorts].filter((v) => !seen.has(v.id) && !!seen.add(v.id));
}

// ---- words ----

export function duration(s: number): string {
  if (!s) return '';
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = Math.floor(s % 60);
  const mm = h ? String(m).padStart(2, '0') : String(m);
  return `${h ? `${h}:` : ''}${mm}:${String(sec).padStart(2, '0')}`;
}

/** "3 giorni fa" for feed videos, which only carry a date. */
export function when(v: Video): string {
  if (v.published) return v.published;
  if (!v.ts) return '';
  const d = Math.max(0, Date.now() / 1000 - v.ts);
  const n = (x: number) => Math.floor(x);
  if (d < 3600) return t('{a} min ago', { a: Math.max(1, n(d / 60)) });
  if (d < 86400) return n(d / 3600) === 1 ? t('1 hour ago') : t('{a} hours ago', { a: n(d / 3600) });
  if (d < 86400 * 30) return n(d / 86400) === 1 ? t('1 day ago') : t('{a} days ago', { a: n(d / 86400) });
  if (d < 86400 * 365) return n(d / 86400 / 30) === 1 ? t('1 month ago') : t('{a} months ago', { a: n(d / 86400 / 30) });
  return n(d / 86400 / 365) === 1 ? t('1 year ago') : t('{a} years ago', { a: n(d / 86400 / 365) });
}

/** "1.2M views" → "1.2M", "1,2 Mln di visualizzazioni" → "1,2 Mln" */
export const shortViews = (s: string) => s.replace(/ di visualizzazioni| visualizzazioni| views?/, '').trim();

// ---- cards ----

/** Videos met in lists, so a video's page has something to show before YouTube answers. */
export const known = new Map<string, Video>();

/** A video's card: A opens its page (Riproduci, Iscriviti, Mi piace… are there). */
export function videoTile(v: Video, row: string): Tile {
  known.set(v.id, v);
  return {
    key: `${row}-${v.id}`,
    src: thumb(v.id),
    title: v.title,
    sub: [v.channel, shortViews(v.views), when(v)].filter(Boolean).join(' · '),
    badge: v.live ? 'LIVE' : duration(v.length) || undefined,
    progress: progressOf(v.id),
    pin: videoPin(v),
    open: () => {
      noteOpened(v.id);
      goto(`/youtube/watch/${v.id}`);
    },
  };
}

/** A recommendation's card: getting focus counts as being looked at (see `noteShown`). */
export const recTile = (v: Video, row: string): Tile => ({ ...videoTile(v, row), focused: () => noteShown(v.id) });

/** Pins (the home's YouTube row): a video keeps what its card shows. */
export const videoPin = (v: Video): Pin => ({
  kind: 'video',
  id: v.id,
  name: v.title,
  sub: v.channel,
  art: { src: thumb(v.id), fit: 'cover' },
});

export const channelPin = (c: Channel): Pin => ({
  kind: 'ytchannel',
  id: c.id,
  name: c.name,
  sub: c.subs,
  art: c.avatar ? { src: ytImage(c.avatar)!, fit: 'contain' } : null,
});

/** A Short's card: plays at once (and counts towards today's Shorts). */
export function shortTile(v: Video, row: string): Tile {
  return { key: `${row}-${v.id}`, src: thumb(v.id, 'short'), title: v.title, sub: shortViews(v.views), tall: true, open: () => playShort(v) };
}

export function channelTile(c: Channel, row: string): Tile {
  return {
    key: `${row}-${c.id}`,
    src: ytImage(c.avatar),
    title: c.name,
    sub: c.subs,
    round: true,
    pin: channelPin(c),
    open: () => goto(`/youtube/channel/${c.id}`),
  };
}
