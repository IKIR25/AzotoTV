// The YouTube app's recommendations: "Consigliati per te" and the "Perché hai
// guardato «…»" rows, from what one profile did here (what it watched and for
// how long, likes, subscriptions, searches, "Non mi interessa", what it was
// shown and passed over). YouTube, signed out, only provides the material: a
// video's related videos, a channel's uploads, searches.
//
// Free of Svelte and Tauri on purpose: it also runs under plain node against
// the real YouTube, with a stand-in for youtube.rs as the `Source`.
import type { Channel, SearchResult, Video, VideoPage, Watched } from './youtube.svelte';
import { lang, t } from '$lib/i18n';

export interface Source {
  video(id: string): Promise<VideoPage>;
  feed(channels: string[], shorts: boolean): Promise<Video[]>;
  search(query: string, recent?: boolean): Promise<SearchResult>;
}

/** What the profile did (kept by youtube.svelte.ts). */
export interface Profile {
  subs: Channel[];
  history: Watched[];
  /** newest first */
  liked: Video[];
  /** searches that led somewhere (a result was opened), newest first */
  searches: { q: string; at: number }[];
  /** "Non mi interessa", newest first */
  nope: { v: Video; at: number }[];
  /** recommendations looked at and passed over: id → [times, last time (ms)] */
  shown: Record<string, [number, number]>;
  shorts: { seen: string[] };
}

export interface Shelf {
  title: string;
  videos: Video[];
}

export interface Feed {
  /** newest normal videos of the subscriptions */
  subs: Video[];
  recommended: Video[];
  /** "Perché hai guardato «…»", "Perché hai cercato «…»" */
  more: Shelf[];
  /** Shorts to offer (the row shows only as many as are left today) */
  shorts: Video[];
}

/**
 * Nothing watched or followed yet: popular this week on a few of these. For
 * Italian, words that bring Italian videos (plain "scienza" or "tecnologia"
 * bring the whole world's, auto-translated).
 */
const DISCOVER_BY_LANGUAGE: Record<string, string[]> = {
  it: [
    'minecraft ita',
    'gameplay ita',
    'calcio highlights',
    'ricette italiane',
    'trailer italiano',
    'documentario animali',
    'viaggi italia',
    'musica italiana',
    'geopop',
  ],
  en: ['minecraft', 'gameplay', 'sports highlights', 'easy recipes', 'official trailer', 'animal documentary', 'travel vlog', 'music video', 'science explained'],
};
const DISCOVER = DISCOVER_BY_LANGUAGE[lang] ?? DISCOVER_BY_LANGUAGE.en;
const DAY = 86_400_000;
const SIZE = 36;
/** a recommendation passed over this many times is dropped (for `SHOWN_DAYS`) */
const SHOWN_MAX = 5;
export const SHOWN_DAYS = 10;

/** How much a watch says about the profile's taste, 0..1. */
export function engagement(w: Watched): number {
  const dur = w.dur || w.v.length;
  if (!dur) return Math.min(1, w.pos / 600);
  const frac = Math.min(1, w.pos / dur);
  if (frac >= 0.9) return 1;
  // left within a minute or so, before most of it: it was picked, little more
  if (w.pos < 90 && frac < 0.6) return 0.15;
  // a good share of it, or a good stretch of minutes, whichever says more
  return Math.min(1, Math.max(0.25, frac / 0.6, w.pos / 900));
}

/** 1 now, halving every `half` days */
const fade = (at: number, now: number, half: number) => 0.5 ** (Math.max(0, now - at) / DAY / half);

const chKey = (v: Video) => v.channel_id || v.channel;

const clip = (s: string, n = 42) => (s.length > n ? `${s.slice(0, n - 1).trimEnd()}…` : s);

/**
 * How much the profile likes each channel: watched well, liked and subscribed
 * add up; dropped after a minute and "Non mi interessa" take away.
 */
function channelTaste(p: Profile, now: number): Map<string, number> {
  const taste = new Map<string, number>();
  const add = (ch: string, x: number) => ch && taste.set(ch, (taste.get(ch) ?? 0) + x);
  for (const w of p.history) if (!w.v.short) add(chKey(w.v), (engagement(w) - 0.2) * Math.max(0.2, fade(w.at, now, 30)));
  p.liked.forEach((v, i) => add(chKey(v), 0.8 * 0.9 ** i));
  for (const c of p.subs) add(c.id, 1.5);
  // one "Non mi interessa" sinks the channel's videos, a second one hides them (`ok` in buildFeed)
  for (const n of p.nope) add(chKey(n.v), -0.8 * fade(n.at, now, 30));
  return taste;
}

// YouTube words ages in the language it is asked in (locale.rs): English and Italian
const UNITS: [RegExp, number][] = [
  [/^second/, 0],
  [/^minut/, 0],
  [/^(hours?|or[ae])$/, 0.04],
  [/^(days?|giorn[oi])$/, 1],
  [/^(weeks?|settiman[ae])$/, 7],
  [/^(months?|mes[ei])$/, 30],
  [/^(years?|ann[oi])$/, 365],
];

/** A video's age in days, from its date or "3 days ago" / "3 giorni fa" (null: unknown). */
export function ageDays(v: Video, now: number): number | null {
  if (v.ts) return Math.max(0, now / 1000 - v.ts) / 86400;
  const m = /(\d+)\s+(\S+)\s+(?:fa|ago)/.exec(v.published);
  const unit = m && UNITS.find(([re]) => re.test(m[2]));
  return unit ? Number(m[1]) * unit[1] : null;
}

/**
 * New videos go up a little, very old ones down a little. Only a little: the
 * filler YouTube adds to related lists (see `fillers`) is mostly a few hours old.
 */
function freshness(v: Video, now: number): number {
  const d = ageDays(v, now);
  if (d === null) return 0;
  return d <= 7 ? 0.06 : d <= 30 ? 0.03 : d >= 3 * 365 ? -0.06 : 0;
}

export function shuffle<T>(a: T[], rand = Math.random): T[] {
  const out = [...a];
  for (let i = out.length - 1; i > 0; i--) {
    const j = Math.floor(rand() * (i + 1));
    [out[i], out[j]] = [out[j], out[i]];
  }
  return out;
}

/** `n` of `items` at random, the heavier the likelier, no repeats. */
function draw<T>(items: T[], weight: (x: T) => number, n: number, rand: () => number): T[] {
  return items
    .map((x) => ({ x, k: rand() ** (1 / Math.max(1e-6, weight(x))) }))
    .sort((a, b) => b.k - a.k)
    .slice(0, n)
    .map((e) => e.x);
}

/** Where candidates come from: a video (its related videos) or a search (its results). */
interface Seed {
  id?: string;
  q?: string;
  w: number;
  /** the title of its own row, if it gets one */
  row?: string;
}

interface Found {
  videos: Video[];
  shorts: Video[];
}

interface Candidate {
  v: Video;
  /** the strongest seed's vote, and all the votes */
  best: number;
  sum: number;
  /** the seed of the strongest vote (-1: a channel the profile watches a lot) */
  from: number;
  score: number;
}

/**
 * Build the profile's feeds. A video gets votes from the seeds it is related
 * to (the profile's strongest interests always, a weighted draw of the others,
 * so the feed moves between visits), then goes up or down with how the
 * profile feels about its channel, its age and how often it was passed over.
 * Some places go to videos a step further out, many while the profile is new,
 * few later. Never throws.
 */
export async function buildFeed(p: Profile, src: Source, now = Date.now(), rand = Math.random): Promise<Feed> {
  const none = () => [] as Video[];
  const subIds = p.subs.map((c) => c.id);
  const watched = new Set(p.history.map((w) => w.v.id));
  const nope = new Set(p.nope.map((n) => n.v.id));
  const liked = new Set(p.liked.map((v) => v.id));
  const taste = channelTaste(p, now);
  const tasteOf = (v: Video) => taste.get(chKey(v)) ?? 0;
  const shownTimes = (id: string) => {
    const s = p.shown[id];
    return s && now - s[1] < SHOWN_DAYS * DAY ? s[0] : 0;
  };
  const ok = (v: Video) =>
    !watched.has(v.id) &&
    !nope.has(v.id) &&
    !v.live &&
    !v.short &&
    (!v.length || (v.length >= 60 && v.length <= 3 * 3600)) &&
    tasteOf(v) > -1 &&
    shownTimes(v.id) < SHOWN_MAX;

  const pool: Seed[] = [];
  for (const w of p.history) {
    if (w.v.short || nope.has(w.v.id)) continue;
    const weight = engagement(w) * Math.max(0.15, fade(w.at, now, 14)) + (liked.has(w.v.id) ? 0.5 : 0);
    pool.push({ id: w.v.id, w: weight, row: t('Because you watched “{clip}”', { clip: clip(w.v.title) }) });
  }
  p.liked.forEach((v, i) => !watched.has(v.id) && pool.push({ id: v.id, w: 0.8 * 0.9 ** i, row: t('Because you like “{clip}”', { clip: clip(v.title) }) }));
  for (const s of p.searches.slice(0, 5)) pool.push({ q: s.q, w: 0.5 * Math.max(0.1, fade(s.at, now, 7)), row: t('Because you searched for “{clip}”', { clip: clip(s.q) }) });
  pool.sort((a, b) => b.w - a.w);
  const blank = !pool.length && !p.subs.length;
  const discovering = blank ? discover(src, ok, rand) : null;

  // channels watched a lot but not followed: their newest videos are candidates too
  const fans = [...taste]
    .filter(([ch, t]) => t >= 1 && ch.startsWith('UC') && !subIds.includes(ch))
    .sort((a, b) => b[1] - a[1])
    .slice(0, 3)
    .map(([ch]) => ch);
  const [subs, subShorts, fanVideos] = await Promise.all([
    src.feed(subIds, false).catch(none),
    src.feed(subIds, true).catch(none),
    src.feed(fans, false).catch(none),
  ]);
  const subsNew = subs.filter((v) => !watched.has(v.id) && !nope.has(v.id)).slice(0, 40);

  const seeds: Seed[] = [
    ...pool.slice(0, 2),
    ...draw(pool.slice(2), (s) => s.w, 8, rand),
    ...draw(subsNew.slice(0, 10), () => 1, 2, rand).map((v): Seed => ({ id: v.id, w: 0.4 })),
  ].filter((s, i, a) => a.findIndex((x) => x.id === s.id && x.q === s.q) === i);
  const fetchAll = async (ss: Seed[]) =>
    (
      await Promise.allSettled(
        ss.map((s): Promise<Found> => (s.q ? src.search(s.q) : src.video(s.id!).then((r) => ({ videos: r.related, shorts: r.shorts })))),
      )
    ).map((r): Found => (r.status === 'fulfilled' ? r.value : { videos: [], shorts: [] }));
  const found = await fetchAll(seeds);

  const vote = (into: Map<string, Candidate>, v: Video, x: number, from: number) => {
    const c = into.get(v.id) ?? { v, best: 0, sum: 0, from, score: 0 };
    if (x > c.best) Object.assign(c, { best: x, from });
    c.sum += x;
    into.set(v.id, c);
  };
  let filler = fillers(found.map((f) => f.videos));
  const rate = (cands: Map<string, Candidate>) => {
    for (const c of cands.values()) {
      const t = tasteOf(c.v);
      c.score =
        c.best +
        // turning up under several seeds helps, but less and less (and not for the filler)
        (filler.has(c.v.id) ? -0.25 : 0.5 * (c.sum - c.best)) +
        (t > 0 ? 0.3 * Math.tanh(t) : 0.5 * t) +
        freshness(c.v, now) -
        0.25 * Math.max(0, shownTimes(c.v.id) - 1) +
        rand() * 0.15;
    }
  };
  const top = Math.max(...seeds.map((s) => s.w));
  const near = new Map<string, Candidate>();
  found.forEach((f, i) => f.videos.forEach((v, j) => ok(v) && vote(near, v, seeds[i].w / top / (1 + 0.08 * j), i)));
  for (const v of fanVideos) if (ok(v) && !near.has(v.id)) vote(near, v, 0.3, -1);
  rate(near);

  // One step further, from a few of the best candidates: more votes for the
  // near ones, and new ground for the profile to find other interests in (a
  // related video's related videos stay in its language and drift slowly,
  // unlike searches on random subjects). A profile's first videos lean on it
  // most, or their feed would be one single list.
  const direct = found.filter((f) => f.videos.length).length;
  const ranked = [...near.values()]
    .filter((c) => !filler.has(c.v.id))
    .sort((a, b) => b.score - a.score)
    .slice(0, 30);
  const bridges = draw(ranked, (c) => Math.max(0.05, c.score), direct < 6 ? 6 - direct : 3, rand);
  const far = new Map<string, Candidate>();
  const across = await fetchAll(bridges.map((c) => ({ id: c.v.id, w: 0 })));
  filler = fillers([...found, ...across].map((f) => f.videos));
  across.forEach((f, i) =>
    f.videos.forEach((v, j) => {
      if (!ok(v)) return;
      const x = (0.5 * bridges[i].best) / (1 + 0.08 * j);
      const c = near.get(v.id);
      if (c) c.sum += x;
      else vote(far, v, x, seeds.length + i);
    }),
  );
  rate(near);
  rate(far);
  const mine = spread([...near.values()], SIZE);

  // places for the new ground: a third while the profile is new, a tenth once it is well known
  const signal = pool.filter((s) => s.w >= 0.3).length + p.subs.length;
  let share = Math.min(0.3, Math.max(0.1, 0.3 - 0.04 * signal));
  let wider = spread([...far.values()], SIZE);
  let discovered: Found | null = null;
  if (blank || mine.length + wider.length < 8) {
    // nothing to go on (or YouTube said little): what's popular this week
    discovered = await (discovering ?? discover(src, ok, rand));
    wider = discovered.videos;
    if (blank) share = 1;
  }

  // the first two are always the profile's own, then the new ground takes `share` of the places
  const recommended: Video[] = [];
  const taken = new Set<string>();
  const perChannel = new Map<string, number>();
  const take = (l: Video[]) => {
    for (let v = l.shift(); v; v = l.shift()) {
      const n = perChannel.get(chKey(v)) ?? 0;
      if (taken.has(v.id) || n >= 2) continue;
      taken.add(v.id);
      perChannel.set(chKey(v), n + 1);
      recommended.push(v);
      return true;
    }
    return false;
  };
  for (let credit = 0; recommended.length < SIZE && (mine.length || wider.length); ) {
    credit = Math.min(1, credit + share);
    const wild = recommended.length >= 2 && credit >= 1;
    if (wild) credit -= 1;
    if (!take(wild ? wider : mine)) take(wild ? mine : wider);
  }

  // rows of their own for the strongest interests, without what the first row shows
  // on screen; the ones the other interests agree on first
  const more: Shelf[] = [];
  const used = new Set(recommended.slice(0, 6).map((v) => v.id));
  const strongest = seeds
    .map((s, i) => ({ s, f: found[i] }))
    .filter(({ s }) => s.row && s.w >= 0.3)
    .sort((a, b) => b.s.w - a.s.w);
  const scoreOf = (v: Video) => near.get(v.id)?.score ?? 0;
  for (const { s, f } of strongest) {
    if (more.length >= 2) break;
    const per = new Map<string, number>();
    const videos = f.videos
      .filter((v) => ok(v) && !used.has(v.id) && !filler.has(v.id))
      .sort((a, b) => scoreOf(b) - scoreOf(a))
      .filter((v) => {
        const n = per.get(chKey(v)) ?? 0;
        per.set(chKey(v), n + 1);
        return n < 2;
      })
      .slice(0, 16);
    if (videos.length < 6) continue;
    videos.forEach((v) => used.add(v.id));
    more.push({ title: s.row!, videos });
  }

  const seenShorts = new Set(p.shorts.seen);
  const relShorts = [...found, ...across, ...(discovered ? [discovered] : [])].flatMap((f) => f.shorts);
  const shorts = [...subShorts.slice(0, 20), ...shuffle(relShorts, rand)]
    .filter((v, i, a) => !seenShorts.has(v.id) && a.findIndex((x) => x.id === v.id) === i)
    .slice(0, 30);

  return { subs: subsNew, recommended, more, shorts };
}

/**
 * YouTube slips a few of the moment's popular videos into every related list
 * (signed out, at least): a political rant under a pasta recipe and a football
 * match alike. Lists about the same thing share a couple of videos or more; a
 * video turning up in lists that otherwise have nothing in common is filler,
 * not a sign of taste.
 */
function fillers(lists: Video[][]): Set<string> {
  const sets = lists.map((l) => new Set(l.map((v) => v.id)));
  const group = sets.map((_, i) => i);
  const root = (i: number): number => (group[i] === i ? i : (group[i] = root(group[i])));
  for (let i = 0; i < sets.length; i++) {
    for (let j = i + 1; j < sets.length; j++) {
      let shared = 0;
      for (const id of sets[i]) if (sets[j].has(id) && ++shared >= 2) break;
      if (shared >= 2) group[root(i)] = root(j);
    }
  }
  const groupsOf = new Map<string, Set<number>>();
  sets.forEach((s, i) => s.forEach((id) => groupsOf.set(id, (groupsOf.get(id) ?? new Set()).add(root(i)))));
  return new Set([...groupsOf].filter(([, g]) => g.size >= 2).map(([id]) => id));
}

/** Popular this week on a few subjects, one from each in turn. */
async function discover(src: Source, ok: (v: Video) => boolean, rand: () => number): Promise<Found> {
  const got = await Promise.allSettled(draw(DISCOVER, () => 1, 3, rand).map((t) => src.search(t, true)));
  const lists = got.map((r) =>
    // normal lengths only: no 3-hour compilations
    r.status === 'fulfilled' ? r.value.videos.filter((v) => ok(v) && v.length >= 120 && v.length <= 5400).slice(0, 12) : [],
  );
  const videos: Video[] = [];
  for (let i = 0; i < 12; i++) for (const l of lists) if (l[i]) videos.push(l[i]);
  return { videos, shorts: got.flatMap((r) => (r.status === 'fulfilled' ? r.value.shorts : [])) };
}

/**
 * Best first, but at most two per channel, never the same channel twice in a
 * row, and the seeds take turns (one strong seed doesn't fill the row).
 */
function spread(cands: Candidate[], n: number): Video[] {
  let rest = [...cands].sort((a, b) => b.score - a.score);
  const out: Candidate[] = [];
  const per = new Map<string, number>();
  while (out.length < n && rest.length) {
    const recent = out.slice(-4);
    const lastCh = recent.length ? chKey(recent[recent.length - 1].v) : '';
    let pick = 0;
    let pickScore = -Infinity;
    for (let i = 0; i < Math.min(rest.length, 60); i++) {
      const c = rest[i];
      const s = c.score - 0.2 * recent.filter((o) => o.from === c.from).length - (chKey(c.v) === lastCh ? 0.5 : 0);
      if (s > pickScore) [pick, pickScore] = [i, s];
    }
    const [c] = rest.splice(pick, 1);
    out.push(c);
    const ch = chKey(c.v);
    per.set(ch, (per.get(ch) ?? 0) + 1);
    if (per.get(ch)! >= 2) rest = rest.filter((x) => chKey(x.v) !== ch);
  }
  return out.map((c) => c.v);
}
