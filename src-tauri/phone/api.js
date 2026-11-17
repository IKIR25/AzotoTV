// Talking to AzotoTV (src-tauri/src/phone.rs): the login this phone keeps,
// HTTP calls, the TV's live line (WebSocket), Jellyfin through the TV
// (phonejf.rs), Seerr through the TV. Same API as the iPhone app.

import { tr, emit, native, toast } from './ui.js';

// ---- the login ------------------------------------------------------------------

// { token, server, users: [{id, name}], active } — one token for this phone, good for
// each profile it signed into (with a PIN or the TV's code).
const KEY = 'azoto.login';
const OLD_TOKEN_KEY = 'azoto.token'; // the first phone page kept only the token

function readLogin() {
  try {
    const l = JSON.parse(localStorage.getItem(KEY) || 'null');
    if (l && l.token) return l;
    const t = localStorage.getItem(OLD_TOKEN_KEY);
    if (t) return { token: t, server: '', users: [], active: '' };
  } catch {}
  return null;
}

export let login = readLogin();
// an iPhone home-screen app opens with its key in the address (manifest.webmanifest)
{
  const k = new URLSearchParams(location.search).get('k');
  if (k && (!login || login.token !== k)) login = { token: k, server: '', users: [], active: '' };
}

export function store(l) {
  login = l && l.token && l.users.length ? l : null;
  try {
    if (login) localStorage.setItem(KEY, JSON.stringify(login));
    else localStorage.removeItem(KEY);
    localStorage.removeItem(OLD_TOKEN_KEY);
  } catch {}
  // "Add to Home Screen" takes this address and this manifest
  history.replaceState(history.state, '', login ? `/?k=${login.token}` : '/');
  const m = document.getElementById('manifest');
  if (m) m.href = login ? `/manifest.webmanifest?k=${login.token}` : '/manifest.webmanifest';
}

export const token = () => login?.token || '';
export const account = () => (login ? login.users.find((u) => u.id === login.active) || login.users[0] || null : null);

// ---- errors ------------------------------------------------------------------------

export class Unauthorized extends Error {}
export class Refused extends Error {}
export class Unreachable extends Error {}

const unreachable = () => new Unreachable(tr("I can't find the TV. The phone has to be on the home Wi-Fi."));

// ---- HTTP --------------------------------------------------------------------------

/** A call to AzotoTV. `body` is sent as JSON; `query` added to the address. */
export async function api(path, { method, body, query, signal } = {}) {
  const q = new URLSearchParams(query || {});
  const url = path + (q.toString() ? (path.includes('?') ? '&' : '?') + q : '');
  let r;
  try {
    r = await fetch(url, {
      method: method || (body ? 'POST' : 'GET'),
      headers: { 'x-azoto-key': token(), ...(body ? { 'content-type': 'application/json' } : {}) },
      body: body ? JSON.stringify(body) : undefined,
      signal,
    });
  } catch (e) {
    if (e.name === 'AbortError') throw e;
    throw unreachable();
  }
  const data = await r.json().catch(() => ({}));
  if (r.status === 401) throw new Unauthorized(tr('The TV no longer recognises this phone: sign in again.'));
  if (!r.ok) throw new Refused(data.error || tr('Error {code}', { code: r.status }));
  return data;
}

/** What this phone is, for the TV's list ("iPhone", the Android phone's own name). */
export function deviceLabel(name) {
  try {
    if (native && native.deviceName) return native.deviceName();
  } catch {}
  const ua = navigator.userAgent;
  let d = /iPhone/.test(ua) ? 'iPhone' : /iPad/.test(ua) ? 'iPad' : null;
  if (!d) {
    const m = ua.match(/Android [\d.]+; ([^;)]+)/);
    d = m && m[1].trim() !== 'K' ? m[1].trim() : /Android/.test(ua) ? tr('Android phone') : tr('Phone');
  }
  return name ? tr('{device} of {name}', { device: d, name }) : d;
}

// ---- the TV's state, live ------------------------------------------------------------

/**
 * What the TV is doing (phone.rs `phone_state`): text (its keyboard is open:
 * {value, label, secret}), volume, dark (screens off), user / user_name (the
 * profile on it), picker ("Chi guarda?"), game {name, players, screen, console},
 * controllers (its Controller screen is open), playing {id, name, live}.
 */
export const tv = {
  state: { text: null, volume: null, dark: false, user: '', user_name: '', picker: false, game: null, controllers: false, playing: null },
  connected: false,
  /** which player the phone's controller is (null: not one) */
  player: null,
  padError: null,
};

let ws = null;
let wanted = false;
let generation = 0;
let retryTimer;

export function linkStart() {
  if (wanted && ws) return;
  wanted = true;
  connect();
}

export function linkStop() {
  wanted = false;
  generation++;
  clearTimeout(retryTimer);
  if (ws) ws.close();
  ws = null;
  setConnected(false);
}

function setConnected(on) {
  if (tv.connected === on) return;
  tv.connected = on;
  if (!on) tv.player = null;
  emit('tv', tv);
}

function connect() {
  const gen = ++generation;
  clearTimeout(retryTimer);
  if (ws) ws.close();
  ws = null;
  if (!token()) return;
  const sock = new WebSocket(`${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws?k=${encodeURIComponent(token())}`);
  ws = sock;
  sock.onopen = () => gen === generation && setConnected(true);
  sock.onmessage = (e) => {
    if (gen !== generation) return;
    let j;
    try {
      j = JSON.parse(e.data);
    } catch {
      return;
    }
    if (j.t === 'player') {
      tv.player = j.player ?? null;
      tv.padError = j.error ?? null;
      emit('player', tv);
    } else if (j.t === 'rumble') {
      emit('rumble', j.ms || 0);
    } else if (!j.t) {
      tv.state = { ...tv.state, ...j };
      emit('tv', tv);
    }
  };
  sock.onclose = () => {
    if (gen !== generation) return;
    ws = null;
    setConnected(false);
    if (wanted) retryTimer = setTimeout(() => gen === generation && connect(), 2000);
  };
}

export function sendTv(msg) {
  if (ws && ws.readyState === 1) ws.send(JSON.stringify(msg));
}

/** A remote action (remote.rs): up down left right a a-hold b start home play stop ff rew next prev chup chdown. */
export const key = (a) => sendTv({ t: 'key', a });
export const typeText = (v, enter = false) => sendTv({ t: 'text', v, enter });
export const volume = (d) => sendTv({ t: 'vol', d });
export const power = () => sendTv({ t: 'power' });

// the Android app's volume keys
window.azotoVolume = (d) => volume(d);

// ---- Jellyfin, as the active profile (through AzotoTV) -----------------------------------

const FIELDS = 'Overview,Genres,CommunityRating,OfficialRating,ProductionYear,RunTimeTicks,ChildCount';

const KEEP = new Set(['ImageTags', 'RequiredHttpHeaders', 'ProviderIds', 'ImageBlurHashes']);

/** Jellyfin's PascalCase keys -> camelCase (as the iPhone app reads them). */
function lower(v) {
  if (Array.isArray(v)) return v.map(lower);
  if (v && typeof v === 'object') {
    const o = {};
    // dictionaries keep their keys ("ImageTags": {"Primary": …})
    for (const k in v) o[k.charAt(0).toLowerCase() + k.slice(1)] = KEEP.has(k) ? v[k] : lower(v[k]);
    return o;
  }
  return v;
}

export class JF {
  constructor(user) {
    this.user = user;
    this.root = `/api/app/jf/${token()}/${user}/`;
  }

  /** `path` is Jellyfin's (a full Jellyfin path with its query works too). */
  url(path, query) {
    const clean = path.startsWith('/') ? path.slice(1) : path;
    if (!query) return this.root + clean;
    const q = new URLSearchParams(query).toString();
    return this.root + clean + (clean.includes('?') ? '&' : '?') + q;
  }

  async send(method, path, query, body) {
    let r;
    try {
      r = await fetch(this.url(path, query), {
        method,
        headers: body ? { 'content-type': 'application/json' } : {},
        body: body ? JSON.stringify(body) : undefined,
        cache: 'no-store',
      });
    } catch {
      throw unreachable();
    }
    if (r.status === 401) throw new Unauthorized(tr('The TV no longer recognises this phone: sign in again.'));
    if (!r.ok) {
      const j = await r.json().catch(() => null);
      throw new Refused(j?.error || tr('Error {code}', { code: r.status }));
    }
    const text = await r.text();
    return text ? lower(JSON.parse(text)) : null;
  }

  get(path, query) {
    return this.send('GET', path, query);
  }

  // images (no `tag=`: that's what makes the server burn flags into posters)
  image(id, kind = 'Primary', width = 360, tag) {
    const q = { maxWidth: width, quality: 85 };
    if (tag) q.v = tag;
    return this.url(`Items/${id}/Images/${kind}`, q);
  }

  /** The upright poster (an episode: its series'). */
  poster(it, width = 360) {
    if ((it.type === 'Episode' || it.type === 'Season') && it.seriesId && it.seriesPrimaryImageTag) return this.image(it.seriesId, 'Primary', width, it.seriesPrimaryImageTag);
    if (it.imageTags?.Primary) return this.image(it.id, 'Primary', width, it.imageTags.Primary);
    if (it.albumId && it.albumPrimaryImageTag) return this.image(it.albumId, 'Primary', width, it.albumPrimaryImageTag);
    return null;
  }

  /** A wide picture: the episode's frame, the film's thumb / backdrop, the series' one. */
  wide(it, width = 640) {
    if (it.type === 'Episode' && it.imageTags?.Primary) return this.image(it.id, 'Primary', width, it.imageTags.Primary);
    if (it.imageTags?.Thumb) return this.image(it.id, 'Thumb', width, it.imageTags.Thumb);
    if (it.backdropImageTags?.[0]) return this.image(it.id, 'Backdrop', width, it.backdropImageTags[0]);
    if (it.parentThumbItemId && it.parentThumbImageTag) return this.image(it.parentThumbItemId, 'Thumb', width, it.parentThumbImageTag);
    if (it.parentBackdropItemId && it.parentBackdropImageTags?.[0]) return this.image(it.parentBackdropItemId, 'Backdrop', width, it.parentBackdropImageTags[0]);
    return null;
  }

  backdrop(it, width = 1200) {
    if (it.backdropImageTags?.[0]) return this.image(it.id, 'Backdrop', width, it.backdropImageTags[0]);
    if (it.parentBackdropItemId && it.parentBackdropImageTags?.[0]) return this.image(it.parentBackdropItemId, 'Backdrop', width, it.parentBackdropImageTags[0]);
    return this.wide(it, width);
  }

  logo(it, width = 600) {
    return it.imageTags?.Logo ? this.image(it.id, 'Logo', width, it.imageTags.Logo) : null;
  }

  // the library

  common() {
    return { userId: this.user, Fields: FIELDS, EnableImageTypes: 'Primary,Backdrop,Thumb,Logo' };
  }

  async views() {
    return (await this.get('UserViews', { userId: this.user })).items;
  }

  async resume() {
    return (await this.get('UserItems/Resume', { ...this.common(), Limit: 20, MediaTypes: 'Video' })).items;
  }

  async nextUp(series) {
    const q = { ...this.common(), Limit: 20 };
    if (series) q.seriesId = series;
    return (await this.get('Shows/NextUp', q)).items;
  }

  latest(parent) {
    return this.get('Items/Latest', { ...this.common(), ParentId: parent, Limit: 20 });
  }

  async favorites() {
    return (await this.get('Items', { ...this.common(), Recursive: true, Filters: 'IsFavorite', IncludeItemTypes: 'Movie,Series,BoxSet', SortBy: 'SortName', Limit: 40 })).items;
  }

  items({ parent, types, sort = 'SortName', order = 'Ascending', start = 0, limit = 60, search, recursive = true, extra } = {}) {
    const q = { ...this.common(), Recursive: recursive, SortBy: sort, SortOrder: order, StartIndex: start, Limit: limit, ...extra };
    if (parent) q.ParentId = parent;
    if (types) q.IncludeItemTypes = types;
    if (search) q.SearchTerm = search;
    return this.get('Items', q);
  }

  item(id) {
    return this.get(`Items/${id}`, { userId: this.user });
  }

  async seasons(series) {
    return (await this.get(`Shows/${series}/Seasons`, this.common())).items;
  }

  async episodes(series, season) {
    const q = this.common();
    if (season) q.seasonId = season;
    return (await this.get(`Shows/${series}/Episodes`, q)).items;
  }

  setPlayed(id, on) {
    return this.send(on ? 'POST' : 'DELETE', `UserPlayedItems/${id}`, { userId: this.user });
  }

  setFavorite(id, on) {
    return this.send(on ? 'POST' : 'DELETE', `UserFavoriteItems/${id}`, { userId: this.user });
  }

  /** Series -> the next episode to watch, season -> its first unwatched one, else the item. */
  async resolve(it) {
    if (it.type === 'Series') {
      const next = (await this.nextUp(it.id))[0];
      if (next) return this.item(next.id);
      const first = (await this.episodes(it.id))[0];
      return first ? this.item(first.id) : null;
    }
    if (it.type === 'Season' && it.seriesId) {
      const eps = await this.episodes(it.seriesId, it.id);
      const e = eps.find((x) => !x.userData?.played) || eps[0];
      return e ? this.item(e.id) : null;
    }
    return it.mediaStreams ? it : this.item(it.id);
  }

  /** Jellyfin hears about playback here (resume points shared with the TV). */
  report(path, body) {
    return this.send('POST', path, null, body).catch(() => {});
  }
}

/** Jellyfin as the active profile (null when signed out). */
export function jf() {
  const a = account();
  return a ? new JF(a.id) : null;
}

// ---- reading Jellyfin items ------------------------------------------------------

export const isEpisode = (it) => it.type === 'Episode';
export const isSeries = (it) => it.type === 'Series';
export const playable = (it) => ['Movie', 'Episode', 'Video', 'MusicVideo'].includes(it.type);

/** "S2 E5" */
export function episodeLabel(it) {
  let s = '';
  if (it.parentIndexNumber != null) s += `S${it.parentIndexNumber} `;
  if (it.indexNumber != null) s += `E${it.indexNumber}`;
  return s.trim();
}

export function runtime(it) {
  const t = it.runTimeTicks;
  if (!t) return null;
  const m = Math.floor(t / 600_000_000);
  return m >= 60 ? `${Math.floor(m / 60)} h ${m % 60} min` : `${m} min`;
}

/** Where the profile stopped, if it did (and hasn't finished it). */
export function resumeTicks(it) {
  const d = it.userData;
  if (!d || d.played || !(d.playbackPositionTicks > 0)) return null;
  return d.playbackPositionTicks;
}

export function progressOf(it) {
  const p = resumeTicks(it);
  if (p == null || !it.runTimeTicks) return null;
  return Math.min(1, p / it.runTimeTicks);
}

/** Names of languages and audio formats, as on the TV (src/lib/tracks.ts). */
const LANGS = {
  ita: 'Italian', it: 'Italian', eng: 'English', en: 'English', spa: 'Spanish', es: 'Spanish', fra: 'French', fre: 'French', fr: 'French',
  deu: 'German', ger: 'German', de: 'German', jpn: 'Japanese', ja: 'Japanese', por: 'Portuguese', pt: 'Portuguese', rus: 'Russian', ru: 'Russian',
  zho: 'Chinese', chi: 'Chinese', zh: 'Chinese', kor: 'Korean', ko: 'Korean', nld: 'Dutch', dut: 'Dutch', pol: 'Polish', tur: 'Turkish',
  ara: 'Arabic', hin: 'Hindi', swe: 'Swedish', nor: 'Norwegian', dan: 'Danish', fin: 'Finnish', ell: 'Greek', gre: 'Greek', ces: 'Czech',
  cze: 'Czech', hun: 'Hungarian', ron: 'Romanian', rum: 'Romanian', ukr: 'Ukrainian', heb: 'Hebrew',
};
const CODECS = { eac3: 'Dolby Digital+', ac3: 'Dolby Digital', truehd: 'Dolby TrueHD', dts: 'DTS', aac: 'AAC', flac: 'FLAC', opus: 'Opus', mp3: 'MP3' };

export function language(code) {
  if (!code || code === 'und') return tr('Unknown|language');
  const n = LANGS[code.toLowerCase()];
  return n ? tr(n) : code.toUpperCase();
}

export function audioLabel(s) {
  const parts = [language(s.language)];
  const c = s.codec && CODECS[s.codec.toLowerCase()];
  if (c) parts.push(c);
  if (s.channels) parts.push(s.channels >= 6 ? '5.1' : s.channels === 2 ? 'Stereo' : s.channels === 1 ? 'Mono' : tr('{n} channels', { n: s.channels }));
  return parts.join(' · ');
}

export function subtitleLabel(s) {
  let t = language(s.language);
  if (s.isForced) t += ' ' + tr('(forced)');
  const title = s.title || '';
  if (title && title.toLowerCase() !== t.toLowerCase() && !['forced', 'sdh', 'default'].includes(title.toLowerCase())) t += ` · ${title}`;
  return t;
}

/** Jellyfin dates ("2026-11-08T20:30:00.0000000Z"). */
export const jfDate = (s) => (s ? new Date(s.replace(/\.\d+Z$/, 'Z')) : null);
export const hhmm = (d) => (d ? d.toLocaleTimeString(navigator.language || 'it', { hour: '2-digit', minute: '2-digit' }) : '');

// ---- Seerr, as the profile's linked user ------------------------------------------------

export async function seerr(method, path, query = {}, body) {
  const a = account();
  if (!a) throw new Unauthorized('');
  const q = Object.entries({ language: lang2(), ...query });
  return api('/api/app/seerr', { body: { method, path, query: q, body }, query: { user: a.id } });
}

const lang2 = () => (document.documentElement.lang || 'en').slice(0, 2);

export const tmdb = (path, size = 'w342') => (path ? `https://image.tmdb.org/t/p/${size}${path}` : null);

// ---- things the whole app does ----------------------------------------------------------

/** "Manda alla TV": a YouTube link plays in the profile's YouTube, anything else through yt-dlp. */
export async function sendLink(url) {
  try {
    const r = await api('/api/send', { body: { url } });
    toast(r.ask ? tr('The TV asks to switch to your profile') : r.kind === 'youtube' ? tr('On YouTube, on the TV') : tr('Sent to the TV'));
    return true;
  } catch (e) {
    toast(e.message);
    return false;
  }
}

/** "Guarda sulla TV": the TV plays it as this profile. */
export async function watchOnTV(body) {
  const a = account();
  if (!a) return;
  try {
    await api('/api/app/tv/watch', { body: { user: a.id, ...body } });
    toast(tr('On the TV'));
    return true;
  } catch (e) {
    toast(e.message);
    return false;
  }
}

/** Shows photos (or a whole album) on the TV. */
export async function showOnTV(body) {
  try {
    const r = await api('/api/show', { body });
    toast(r.ask ? tr('Confirm your profile on the TV') : tr('On the TV'));
  } catch (e) {
    toast(e.message);
  }
}

/** A song / video URL some players need with the key in it. */
export const thumb = (id, preview) => `/api/thumb/${id}?k=${token()}${preview ? '&size=preview' : ''}`;
