// TV and Live TV on the phone: the profile's Jellyfin (continue, next
// episodes, favourites, libraries, search, a film / series page) played here,
// with resume points shared with the TV, and "Sulla TV" / "Continua qui".

import {
  tr, h, fill, icon, btn, iconBtn, spinner, img, toast, menu, cover, on, onLong, progressLine, clock, landscape, keepAwake, attachStream, pullToRefresh, buzz, testMute,
} from './ui.js';
import {
  jf, account, isEpisode, isSeries, playable, episodeLabel, runtime, resumeTicks, progressOf, audioLabel, subtitleLabel, watchOnTV, jfDate, hhmm,
} from './api.js';
import { push } from './model.js';
import { music } from './music.js';

// ---- the TV app -------------------------------------------------------------------------------

const ORDER = ['movies', 'tvshows', 'boxsets'];
const libraryTypes = (kind) => ({ movies: 'Movie', tvshows: 'Series', boxsets: 'BoxSet' })[kind] || null;

export function tvAppPage() {
  const shelves = h('div', { class: 'shelves' }, spinner('pad60'));
  const results = h('div', { class: 'postergrid pad16', hidden: true });
  const search = searchField(tr('Films, series, episodes'), async (q) => {
    shelves.hidden = !!q;
    results.hidden = !q;
    if (!q) return;
    const page = await jf().items({ types: 'Movie,Series,Episode,BoxSet', limit: 40, search: q });
    fill(results, page.items.map((it) => posterCard(it)));
  });
  const body = h('div', { class: 'content flush' }, shelves, results);

  const load = async () => {
    const j = jf();
    try {
      const [resume, next, favs, views] = await Promise.all([j.resume().catch(() => []), j.nextUp().catch(() => []), j.favorites().catch(() => []), j.views()]);
      const started = new Set(resume.map((x) => x.seriesId).filter(Boolean));
      const nextUp = next.filter((x) => !started.has(x.seriesId || ''));
      const libs = views.filter((v) => ORDER.includes(v.collectionType || '')).sort((a, b) => ORDER.indexOf(a.collectionType) - ORDER.indexOf(b.collectionType));
      const latest = await Promise.all(
        libs.map((v) =>
          (v.collectionType === 'boxsets' ? j.items({ parent: v.id, limit: 20, recursive: false }).then((p) => p.items) : j.latest(v.id)).catch(() => []),
        ),
      );
      fill(
        shelves,
        resume.length ? shelf(tr('Continue watching'), wideRow(resume)) : null,
        nextUp.length ? shelf(tr('Next episodes'), wideRow(nextUp)) : null,
        favs.length ? shelf(tr('Favourites'), posterRow(favs)) : null,
        libs.map((v, i) => shelf(v.name, posterRow(latest[i]), () => push(libraryPage(v)))),
      );
    } catch (e) {
      fill(shelves, h('p', { class: 'dim pad16' }, e.message));
    }
  };
  load();
  const page = { title: 'TV', large: true, body, under: () => search };
  setTimeout(() => pullToRefresh(page.scroller, load), 0);
  return page;
}

/** A search field under the title; `run(query)` after a short pause in typing. */
export function searchField(placeholder, run) {
  const input = h('input', { class: 'field search', type: 'search', placeholder, autocapitalize: 'off', autocorrect: 'off', enterkeyhint: 'search' });
  let timer;
  let gen = 0;
  input.addEventListener('input', () => {
    clearTimeout(timer);
    const q = input.value.trim();
    const mine = ++gen;
    timer = setTimeout(async () => {
      try {
        await run(q, () => mine === gen);
      } catch (e) {
        toast(e.message);
      }
    }, q ? 300 : 0);
  });
  return h('div', { class: 'searchbar' }, icon('search'), input);
}

/** A titled row (with "Tutti" when there's a library behind it). */
export function shelf(title, content, all) {
  return h(
    'section',
    { class: 'shelf' },
    h('div', { class: 'shelfhead' }, h('h2', { class: 'title3' }, title), all ? h('button', { class: 'linkbtn', type: 'button', onclick: all }, tr('All'), icon('chevron')) : null),
    content,
  );
}

function posterRow(items) {
  return h('div', { class: 'hrow' }, items.map((it) => posterCard(it, 'w118')));
}

function wideRow(items) {
  return h('div', { class: 'hrow' }, items.map((it) => wideCard(it)));
}

function watchedBadge(it) {
  if (it.userData?.played) return h('span', { class: 'badge round' }, icon('check'));
  const n = it.userData?.unplayedItemCount;
  if (isSeries(it) && n > 0) return h('span', { class: 'badge' }, String(n));
  return null;
}

export function posterCard(it, cls = '') {
  const j = jf();
  return h(
    'button',
    { class: 'poster ' + cls, type: 'button', onclick: () => push(itemPage(it)) },
    h('div', { class: 'pimg' }, img(j.poster(it)), h('div', { class: 'tr' }, watchedBadge(it)), h('div', { class: 'bl' }, progressLine(progressOf(it)))),
    h('div', { class: 'caption b clamp2' }, it.name || ''),
    it.productionYear ? h('div', { class: 'caption2 dim' }, String(it.productionYear)) : null,
  );
}

/** "Continua" / "Prossimi episodi": a tap plays from where it was; holding offers the page. */
function wideCard(it) {
  const j = jf();
  const pic = h(
    'button',
    { class: 'wimg', type: 'button', onclick: () => play(it) },
    img(j.wide(it)),
    h('span', { class: 'playbadge' }, icon('play')),
    h('div', { class: 'bl' }, progressLine(progressOf(it))),
  );
  onLong(pic, () =>
    menu([
      { label: tr('Open the page'), icon: 'info', run: () => push(itemPage(it)) },
      { label: tr('Watch on the TV'), icon: 'tv', run: () => watchItemOnTV(it) },
    ]),
  );
  return h(
    'div',
    { class: 'widecard' },
    pic,
    h('div', { class: 'subhead ellipsis' }, isEpisode(it) ? it.seriesName || it.name : it.name),
    h('div', { class: 'caption dim ellipsis' }, isEpisode(it) ? `${episodeLabel(it)} · ${it.name}` : runtime(it) || ''),
  );
}

/** A whole library, in pages, with a sort. */
function libraryPage(view) {
  const grid = h('div', { class: 'postergrid pad16' });
  const more = h('div', { class: 'moreslot' });
  const body = h('div', { class: 'content flush' }, grid, more);
  const SORTS = [
    ['DateCreated', tr('Recently added')],
    ['SortName', tr('Title')],
    ['PremiereDate', tr('Release date')],
    ['CommunityRating', tr('Rating')],
  ];
  let sort = 'DateCreated';
  let count = 0;
  let total = Infinity;
  let busy = false;
  const next = async () => {
    if (busy || count >= total) return;
    busy = true;
    fill(more, spinner());
    try {
      const p = await jf().items({
        parent: view.id,
        types: libraryTypes(view.collectionType),
        sort,
        order: sort === 'SortName' ? 'Ascending' : 'Descending',
        start: count,
        limit: 60,
        recursive: view.collectionType !== 'boxsets',
      });
      grid.append(...p.items.map((it) => posterCard(it)));
      count += p.items.length;
      total = p.totalRecordCount ?? count;
    } catch (e) {
      toast(e.message);
    }
    busy = false;
    fill(more);
  };
  const restart = () => {
    fill(grid);
    count = 0;
    total = Infinity;
    next();
  };
  const io = new IntersectionObserver((e) => e[0].isIntersecting && next(), { rootMargin: '800px' });
  io.observe(more);
  restart();
  return {
    title: view.name,
    body,
    actions: () => [iconBtn('sort', () => menu(SORTS.map(([k, label]) => ({ label, checked: k === sort, run: () => ((sort = k), restart()) }))), tr('Sort'), 'navbtn')],
    dispose: () => io.disconnect(),
  };
}

// ---- one film / series / collection --------------------------------------------------------------

export function itemPage(start) {
  let item = start;
  let audio = null;
  let sub = null;
  let seasons = [];
  let season = null;
  let next = null;
  let series = null;
  let children = [];
  const head = h('div', { class: 'itemhead' });
  const main = h('div', { class: 'itembody' });
  const body = h('div', { class: 'content flush item' }, head, main);
  const page = { title: '', short: item.name, body };

  /** what the big button plays: the item, or the series' next episode */
  const target = () => (isSeries(item) ? next : playable(item) ? item : null);
  const tracks = (t) => (t && t.id === item.id ? { audio, sub } : {});

  const meta = () => {
    const parts = [];
    if (item.productionYear) parts.push(String(item.productionYear));
    if (runtime(item)) parts.push(runtime(item));
    if (item.officialRating) parts.push(item.officialRating);
    if (item.communityRating) parts.push(`★ ${item.communityRating.toFixed(1)}`);
    if (item.genres?.length) parts.push(item.genres.slice(0, 2).join(', '));
    return parts.join(' · ');
  };

  const playLabel = (t) => {
    const ep = isEpisode(t) && isSeries(item) ? ` ${episodeLabel(t)}` : '';
    const r = resumeTicks(t);
    return r ? tr('Resume{ep} from {time}', { ep, time: clock(r / 1e7) }) : tr('Play{ep}', { ep });
  };

  const draw = () => {
    const j = jf();
    const logo = j.logo(item);
    fill(
      head,
      img(j.backdrop(item), 'backdrop'),
      h(
        'div',
        { class: 'headtext' },
        logo ? img(logo, 'logo') : h('h1', { class: 'title1' }, isEpisode(item) ? item.seriesName || item.name : item.name),
        isEpisode(item) ? h('div', { class: 'headline' }, `${episodeLabel(item)} · ${item.name}`) : null,
        h('div', { class: 'caption dim' }, meta()),
      ),
    );
    const t = target();
    const fav = item.userData?.isFavorite;
    const seen = item.userData?.played;
    fill(
      main,
      t ? btn(playLabel(t), () => play(t, tracks(t)), { kind: 'primary', icon: 'play', cls: 'big wide' }) : null,
      h(
        'div',
        { class: 'row gap8' },
        t && resumeTicks(t) ? btn(tr('From the start'), () => play(t, { ...tracks(t), from: 0 }), { icon: 'restart', cls: 'grow' }) : null,
        t ? btn(tr('On the TV'), () => watchItemOnTV(t, tracks(t)), { icon: 'tv', cls: 'grow' }) : null,
        iconBtn(fav ? 'heartFill' : 'heart', async () => {
          await jf().setFavorite(item.id, !fav).catch((e) => toast(e.message));
          load();
        }, tr('Favourite'), 'glassbtn sq'),
        item.type !== 'BoxSet'
          ? iconBtn(seen ? 'checkCircle' : 'circle', async () => {
              await jf().setPlayed(item.id, !seen).catch((e) => toast(e.message));
              load();
            }, tr('Watched'), 'glassbtn sq')
          : null,
      ),
      playable(item) ? trackPickers() : null,
      item.overview ? h('p', { class: 'callout dim' }, item.overview) : null,
      isSeries(item) ? seriesPart() : null,
      series ? btn(tr('All episodes of {name}', { name: series.name }), () => push(itemPage(series)), { icon: 'list', cls: 'wide' }) : null,
      item.type === 'BoxSet' || item.type === 'Season' ? h('div', { class: 'postergrid' }, children.map((c) => posterCard(c))) : null,
    );
  };

  const trackPickers = () => {
    const streams = item.mediaStreams || [];
    const audios = streams.filter((s) => s.type === 'Audio');
    const subs = streams.filter((s) => s.type === 'Subtitle');
    if (audios.length < 2 && !subs.length) return null;
    const cur = (list, i) => list.find((s) => s.index === i);
    return h(
      'div',
      { class: 'row gap8 wrap' },
      audios.length > 1
        ? chip('speaker', cur(audios, audio) ? audioLabel(cur(audios, audio)) : tr('Audio'), () =>
            menu(audios.map((s) => ({ label: audioLabel(s), checked: s.index === audio, run: () => ((audio = s.index), draw()) })), { title: tr('Audio') }),
          )
        : null,
      subs.length
        ? chip('captions', cur(subs, sub) ? subtitleLabel(cur(subs, sub)) : tr('No subtitles'), () =>
            menu(
              [{ label: tr('None'), checked: sub == null || sub < 0, run: () => ((sub = -1), draw()) }, ...subs.map((s) => ({ label: subtitleLabel(s), checked: s.index === sub, run: () => ((sub = s.index), draw()) }))],
              { title: tr('Subtitles') },
            ),
          )
        : null,
    );
  };

  const seriesPart = () => {
    const eps = h('div', { class: 'episodes' }, spinner());
    const tabs =
      seasons.length > 1
        ? h(
            'div',
            { class: 'hrow chips' },
            seasons.map((s) => h('button', { class: 'chipbtn' + (s.id === season ? ' on' : ''), type: 'button', onclick: () => ((season = s.id), draw()) }, s.name)),
          )
        : null;
    if (season)
      jf()
        .episodes(item.id, season)
        .then((list) => fill(eps, list.map((e) => episodeRow(e, load))))
        .catch((e) => fill(eps, h('p', { class: 'dim' }, e.message)));
    return h('div', { class: 'col gap12' }, tabs, eps);
  };

  const load = async () => {
    const j = jf();
    try {
      item = await j.item(item.id);
    } catch {}
    if (playable(item)) {
      const src = item.mediaSources?.[0];
      if (audio == null) audio = src?.defaultAudioStreamIndex ?? null;
      if (sub == null) sub = src?.defaultSubtitleStreamIndex ?? -1;
    }
    if (isEpisode(item) && item.seriesId && !series) series = await j.item(item.seriesId).catch(() => null);
    if (isSeries(item)) {
      seasons = await j.seasons(item.id).catch(() => []);
      next = await j.resolve(item).catch(() => null);
      if (!season) season = seasons.find((s) => s.id === next?.seasonId)?.id || seasons[0]?.id || null;
    }
    if (item.type === 'BoxSet' || item.type === 'Season') {
      children = (await j.items({ parent: item.id, sort: 'ProductionYear,SortName', limit: 200, recursive: false }).catch(() => ({ items: [] }))).items;
    }
    draw();
  };
  draw();
  load();
  setTimeout(() => pullToRefresh(page.scroller, load), 0);
  return page;
}

function chip(ic, text, onclick) {
  return h('button', { class: 'chipbtn glass', type: 'button', onclick }, icon(ic), h('span', { class: 'ellipsis' }, text));
}

function episodeRow(e, changed) {
  const j = jf();
  const row = h(
    'button',
    { class: 'eprow', type: 'button', onclick: () => play(e) },
    h('div', { class: 'epimg' }, img(j.wide(e, 400)), h('div', { class: 'tr' }, watchedBadge(e)), h('div', { class: 'bl' }, progressLine(progressOf(e)))),
    h(
      'div',
      { class: 'grow col gap2' },
      h('div', { class: 'subhead clamp2' }, `${e.indexNumber != null ? e.indexNumber + '. ' : ''}${e.name}`),
      runtime(e) ? h('div', { class: 'caption dim' }, runtime(e)) : null,
      e.overview ? h('div', { class: 'caption2 dim clamp2' }, e.overview) : null,
    ),
  );
  onLong(row, () =>
    menu([
      { label: tr('Watch on the TV'), icon: 'tv', run: () => watchItemOnTV(e) },
      {
        label: e.userData?.played ? tr('Mark as unwatched') : tr('Mark as watched'),
        icon: 'checkCircle',
        run: async () => {
          await jf().setPlayed(e.id, !e.userData?.played).catch(() => {});
          changed();
        },
      },
    ]),
  );
  return row;
}

// ---- playing ---------------------------------------------------------------------------------------

/** Plays a film / episode / series (its next episode) here, from where the profile was (or `from`). */
export async function play(it, { from, audio, sub } = {}) {
  try {
    const target = await jf().resolve(it);
    if (!target) return toast(tr('Nothing to play'));
    openVideo(target, { startTicks: from ?? resumeTicks(target), audio, sub });
  } catch (e) {
    toast(e.message);
  }
}

/** "Guarda sulla TV": the TV plays it as this profile, from where the profile was. */
export async function watchItemOnTV(it, { audio, sub } = {}) {
  try {
    const t = await jf().resolve(it);
    if (!t) return toast(tr('Nothing to play'));
    watchOnTV({ item: t.id, ticks: resumeTicks(t), audio, sub });
  } catch (e) {
    toast(e.message);
  }
}

/**
 * What this browser plays as it is, and what Jellyfin turns everything else into:
 * HLS with fMP4 pieces (the picture copied when the browser can show it), text
 * subtitles as WebVTT files beside it, picture subtitles burnt in.
 */
function deviceProfile() {
  const ms = window.MediaSource || window.ManagedMediaSource;
  const ok = (type) => !!(ms?.isTypeSupported?.(type) || document.createElement('video').canPlayType(type) === 'probably');
  const hevc = ok('video/mp4; codecs="hvc1.1.6.L120.90"');
  const ac3 = ok('audio/mp4; codecs="ac-3"');
  const video = hevc ? 'h264,hevc' : 'h264';
  const audio = 'aac,mp3,opus,flac' + (ac3 ? ',ac3,eac3' : '');
  return {
    Name: 'AzotoTV phone page',
    MaxStreamingBitrate: 40_000_000,
    MaxStaticBitrate: 40_000_000,
    MusicStreamingTranscodingBitrate: 256_000,
    DirectPlayProfiles: [
      { Container: 'mp4,m4v', Type: 'Video', VideoCodec: video, AudioCodec: audio },
      { Container: 'mp3,aac,m4a,flac,wav,ogg,opus,webm', Type: 'Audio' },
    ],
    TranscodingProfiles: [
      { Container: 'mp4', Type: 'Video', VideoCodec: video, AudioCodec: ac3 ? 'aac,ac3,eac3' : 'aac', Protocol: 'hls', Context: 'Streaming', MaxAudioChannels: '6', MinSegments: 1, BreakOnNonKeyFrames: true },
      { Container: 'aac', Type: 'Audio', AudioCodec: 'aac', Protocol: 'http', Context: 'Streaming', MaxAudioChannels: '2' },
    ],
    ContainerProfiles: [],
    CodecProfiles: [
      {
        Type: 'Video',
        Codec: 'h264',
        Conditions: [
          { Condition: 'LessThanEqual', Property: 'VideoLevel', Value: '52', IsRequired: false },
          { Condition: 'NotEquals', Property: 'IsInterlaced', Value: 'true', IsRequired: false },
        ],
      },
    ],
    SubtitleProfiles: [
      { Format: 'vtt', Method: 'External' },
      { Format: 'pgssub', Method: 'Encode' },
      { Format: 'dvdsub', Method: 'Encode' },
      { Format: 'dvbsub', Method: 'Encode' },
      { Format: 'ass', Method: 'Encode' },
      { Format: 'ssa', Method: 'Encode' },
    ],
  };
}

/** One film / episode playing: asks Jellyfin how, plays it, tells Jellyfin where it is, goes on to the next episode. */
class VideoSession {
  constructor(item, req, video, ui) {
    this.j = jf();
    this.item = item;
    this.audio = req.audio ?? null;
    this.sub = req.sub ?? null;
    this.video = video;
    this.ui = ui;
    this.gen = 0;
    this.reported = false;
    this.lastReport = 0;
    this.stopStream = null;
    video.addEventListener('timeupdate', () => this.tick());
    video.addEventListener('ended', () => this.ended());
    video.addEventListener('play', () => this.ui.changed());
    video.addEventListener('pause', () => this.ui.changed());
    this.start((req.startTicks || 0) / 1e7);
  }

  get position() {
    return this.video.currentTime || 0;
  }

  get duration() {
    const d = this.video.duration;
    return isFinite(d) && d > 0 ? d : (this.item.runTimeTicks || 0) / 1e7;
  }

  get paused() {
    return this.video.paused;
  }

  /** Asks Jellyfin for a stream from `seconds` on (again after another audio / subtitle choice). */
  async start(seconds) {
    const gen = ++this.gen;
    this.ui.loading(true);
    this.ui.error(null);
    try {
      if (!this.item.mediaStreams) this.item = await this.j.item(this.item.id);
      const streams = this.item.mediaStreams || [];
      this.audioStreams = streams.filter((s) => s.type === 'Audio');
      this.subStreams = streams.filter((s) => s.type === 'Subtitle');
      const body = {
        UserId: this.j.user,
        DeviceProfile: deviceProfile(),
        MaxStreamingBitrate: 40_000_000,
        StartTimeTicks: Math.floor(seconds * 1e7),
        EnableDirectPlay: true,
        EnableDirectStream: true,
        EnableTranscoding: true,
        AllowVideoStreamCopy: true,
        AllowAudioStreamCopy: true,
        AutoOpenLiveStream: true,
      };
      if (this.audio != null) body.AudioStreamIndex = this.audio;
      if (this.sub != null) body.SubtitleStreamIndex = this.sub;
      const info = await this.j.send('POST', `Items/${this.item.id}/PlaybackInfo`, { userId: this.j.user }, body);
      if (gen !== this.gen) return;
      const src = info.mediaSources?.[0];
      if (!src) throw new Error(tr("This video can't be played"));
      if (this.audio == null) this.audio = src.defaultAudioStreamIndex ?? null;
      if (this.sub == null) this.sub = src.defaultSubtitleStreamIndex ?? -1;
      this.session = info.playSessionId || crypto.randomUUID?.() || String(Math.random());
      this.source = src.id || this.item.id;
      let url;
      if (src.transcodingUrl) {
        url = this.j.url(src.transcodingUrl);
        this.method = src.transcodingUrl.includes('VideoCodec=copy') || !src.transcodingUrl.includes('TranscodeReasons') ? 'DirectStream' : 'Transcode';
      } else {
        url = this.j.url(`Videos/${this.item.id}/stream`, { static: 'true', mediaSourceId: this.source, playSessionId: this.session });
        this.method = 'DirectPlay';
      }
      this.stopStream?.();
      for (const t of [...this.video.querySelectorAll('track')]) t.remove();
      // text subtitles: a WebVTT file beside the picture
      const s = (src.mediaStreams || []).find((x) => x.type === 'Subtitle' && x.index === this.sub);
      if (s && s.deliveryMethod === 'External' && s.deliveryUrl) {
        const t = h('track', { kind: 'subtitles', src: this.j.url(s.deliveryUrl), srclang: (s.language || 'und').slice(0, 2), label: subtitleLabel(s), default: true });
        this.video.append(t);
        t.track.mode = 'showing';
      }
      this.stopStream = await attachStream(this.video, url, { onFatal: () => gen === this.gen && this.fail(tr('Playback failed')) });
      await new Promise((resolve, reject) => {
        const ok = () => (cleanup(), resolve());
        const bad = () => (cleanup(), reject(new Error(tr('Playback failed'))));
        const cleanup = () => {
          this.video.removeEventListener('loadedmetadata', ok);
          this.video.removeEventListener('error', bad);
        };
        this.video.addEventListener('loadedmetadata', ok);
        this.video.addEventListener('error', bad);
      });
      if (gen !== this.gen) return;
      if (seconds > 1) this.video.currentTime = seconds;
      await this.video.play().catch(() => {});
      this.ui.loading(false);
      this.reported = true;
      this.report('Sessions/Playing');
      this.ui.changed();
    } catch (e) {
      if (gen !== this.gen) return;
      this.fail(e.message);
    }
  }

  fail(message) {
    this.ui.loading(false);
    this.ui.error(message);
  }

  tick() {
    this.ui.time();
    if (Date.now() - this.lastReport >= 10_000 && this.reported) this.report('Sessions/Playing/Progress', { EventName: 'timeupdate' });
  }

  body(extra = {}) {
    const b = {
      ItemId: this.item.id,
      MediaSourceId: this.source,
      PlaySessionId: this.session,
      PositionTicks: Math.floor(this.position * 1e7),
      IsPaused: this.paused,
      PlayMethod: this.method,
      CanSeek: true,
    };
    if (this.audio != null) b.AudioStreamIndex = this.audio;
    if (this.sub != null) b.SubtitleStreamIndex = this.sub;
    return { ...b, ...extra };
  }

  report(path, extra) {
    this.lastReport = Date.now();
    this.j.report(path, this.body(extra));
  }

  /** Jellyfin learns where it stopped (and ends its stream). */
  async reportStop(at) {
    if (!this.reported) return;
    this.reported = false;
    const b = this.body();
    if (at != null) b.PositionTicks = Math.floor(at * 1e7);
    await this.j.report('Sessions/Playing/Stopped', b);
  }

  toggle() {
    if (this.video.paused) this.video.play();
    else this.video.pause();
    this.report('Sessions/Playing/Progress', { EventName: this.video.paused ? 'pause' : 'unpause' });
  }

  seek(seconds) {
    this.video.currentTime = Math.max(0, Math.min(seconds, Math.max(this.duration - 1, 0)));
    this.report('Sessions/Playing/Progress', { EventName: 'timeupdate' });
  }

  /** Another audio track or subtitles: Jellyfin makes a new stream from here. */
  async choose({ audio, sub }) {
    const at = this.position;
    await this.reportStop();
    if (audio != null) this.audio = audio;
    if (sub != null) this.sub = sub;
    this.start(at);
  }

  async ended() {
    await this.reportStop(this.duration);
    try {
      if (isEpisode(this.item) && this.item.seriesId) {
        const all = await this.j.episodes(this.item.seriesId);
        const i = all.findIndex((e) => e.id === this.item.id);
        if (i >= 0 && i + 1 < all.length) {
          const next = await this.j.item(all[i + 1].id);
          this.ui.upNext(next);
          this.item = next;
          this.audio = null;
          this.sub = null;
          this.ui.changed();
          return this.start(0);
        }
      }
    } catch {}
    this.ui.finished();
  }

  async close() {
    this.gen++;
    this.video.pause();
    await this.reportStop();
    this.stopStream?.();
  }
}

/** The full-screen player, sideways. */
export function openVideo(item, req = {}) {
  music.yield();
  let session;
  let hideTimer;
  let scrubbing = false;
  const { close } = cover(
    (close) => {
      const video = h('video', { playsinline: true, 'webkit-playsinline': true, preload: 'auto' });
      const wait = h('div', { class: 'pwait' }, spinner(), h('div', { class: 'subhead' }));
      const err = h('div', { class: 'perror', hidden: true });
      const title = h('div', { class: 'ptitle grow' });
      const playBtn = h('button', { class: 'pbig', type: 'button', 'aria-label': tr('Play/Pause') }, icon('pause'));
      const seek = h('input', { class: 'seek', type: 'range', min: 0, max: 1, step: 'any', value: 0 });
      const cur = h('span', {}, '0:00');
      const left = h('span', {}, '-0:00');
      const tracksBtn = iconBtn('captions', () => tracksMenu(), tr('Audio and subtitles'), 'glassbtn');
      const tvBtn = btn(tr('On the TV'), () => toTV(), { icon: 'tv' });
      const controls = h(
        'div',
        { class: 'pcontrols' },
        h('div', { class: 'ptop' }, iconBtn('close', () => close(), tr('Close'), 'glassbtn'), title, tracksBtn, tvBtn),
        h(
          'div',
          { class: 'pmid' },
          h('button', { class: 'pskip', type: 'button', onclick: () => (session.seek(session.position - 10), autoHide()), 'aria-label': '-10' }, icon('back10')),
          playBtn,
          h('button', { class: 'pskip', type: 'button', onclick: () => (session.seek(session.position + 30), autoHide()), 'aria-label': '+30' }, icon('fwd30')),
        ),
        h('div', { class: 'pbottom' }, seek, h('div', { class: 'ptimes' }, cur, left)),
      );
      const el = h('div', { class: 'player' }, video, h('div', { class: 'ptap' }), wait, err, controls);
      playBtn.onclick = () => (session.toggle(), autoHide());
      el.querySelector('.ptap').onclick = () => {
        el.classList.toggle('bare');
        if (!el.classList.contains('bare')) autoHide();
      };
      seek.addEventListener('input', () => {
        scrubbing = true;
        clearTimeout(hideTimer);
        cur.textContent = clock(+seek.value);
      });
      seek.addEventListener('change', () => {
        scrubbing = false;
        session.seek(+seek.value);
        autoHide();
      });
      const autoHide = () => {
        clearTimeout(hideTimer);
        hideTimer = setTimeout(() => !scrubbing && session && !session.paused && el.classList.add('bare'), 4000);
      };
      const ui = {
        loading: (on) => {
          wait.hidden = !on;
          controls.querySelector('.pmid').style.opacity = on ? 0 : 1;
          tvBtn.disabled = on;
        },
        error: (m) => {
          err.hidden = !m;
          if (m) fill(err, icon('warn', 'huge'), h('p', { class: 'tc' }, m), btn(tr('Close'), () => close(), { kind: 'primary' }));
          controls.hidden = !!m;
        },
        changed: () => {
          const it = session?.item || item;
          fill(
            title,
            h('div', { class: 'headline ellipsis' }, isEpisode(it) ? it.seriesName || it.name : it.name),
            isEpisode(it) ? h('div', { class: 'subhead dim ellipsis' }, `${episodeLabel(it)} · ${it.name}`) : null,
          );
          fill(playBtn, icon(session && session.paused ? 'play' : 'pause'));
          if (session && !session.paused) autoHide();
          else el.classList.remove('bare');
        },
        time: () => {
          if (!session || scrubbing) return;
          seek.max = Math.max(session.duration, 1);
          seek.value = session.position;
          cur.textContent = clock(session.position);
          left.textContent = '-' + clock(Math.max(0, session.duration - session.position));
        },
        upNext: (next) => (wait.lastChild.textContent = tr('Next episode: {name}', { name: next.name })),
        finished: () => close(),
      };
      ui.changed();
      session = new VideoSession(item, req, video, ui);
      const tracksMenu = () => {
        const items = [];
        if (session.audioStreams?.length > 1) {
          items.push(...session.audioStreams.map((a) => ({ label: `${tr('Audio')}: ${audioLabel(a)}`, checked: a.index === session.audio, run: () => session.choose({ audio: a.index }) })), null);
        }
        items.push({ label: `${tr('Subtitles')}: ${tr('None')}`, checked: (session.sub ?? -1) < 0, run: () => session.choose({ sub: -1 }) });
        items.push(...(session.subStreams || []).map((s) => ({ label: `${tr('Subtitles')}: ${subtitleLabel(s)}`, checked: s.index === session.sub, run: () => session.choose({ sub: s.index }) })));
        menu(items);
      };
      /** "Sulla TV": the TV goes on from here (as this profile), the phone stops. */
      const toTV = async () => {
        tvBtn.disabled = true;
        const ticks = Math.floor(session.position * 1e7);
        const { audio, sub } = session;
        const id = session.item.id;
        await session.close();
        await watchOnTV({ item: id, ticks, audio, sub });
        close();
      };
      return el;
    },
    {
      cls: 'black',
      onClose: () => {
        clearTimeout(hideTimer);
        session?.close();
        landscape(false);
        keepAwake(false);
      },
    },
  );
  landscape(true);
  keepAwake(true);
  return close;
}

// ---- Live TV -----------------------------------------------------------------------------------------

/** Channels and their guide (as the TV's live.svelte.ts), shared by the list and the player. */
const guide = {
  channels: [],
  programs: {},
  loadedAt: 0,
  user: '',
  async load(force = false) {
    const j = jf();
    if (!force && this.user === j.user && this.channels.length && Date.now() - this.loadedAt < 600_000) return;
    const page = await j.get('LiveTv/Channels', { userId: j.user, EnableImageTypes: 'Primary', Limit: 500 });
    this.channels = page.items.sort((a, b) => (parseFloat(a.channelNumber) || 9999) - (parseFloat(b.channelNumber) || 9999));
    const now = new Date();
    const progs = await j.get('LiveTv/Programs', {
      userId: j.user,
      ChannelIds: this.channels.map((c) => c.id).join(','),
      MinEndDate: now.toISOString(),
      MaxStartDate: new Date(now.getTime() + 8 * 3600_000).toISOString(),
      SortBy: 'StartDate',
      EnableImageTypes: 'Primary',
      Limit: 2000,
    });
    this.programs = {};
    for (const p of progs.items) (this.programs[p.channelId] ||= []).push(p);
    this.loadedAt = Date.now();
    this.user = j.user;
  },
  /** what's on now and next on `ch` */
  nowNext(ch, t = new Date()) {
    const list = (this.programs[ch] || []).filter((p) => (jfDate(p.endDate) || Infinity) > t);
    if (!list.length) return [null, null];
    if ((jfDate(list[0].startDate) || Infinity) <= t) return [list[0], list[1] || null];
    return [null, list[0]];
  },
  progress(p, t = new Date()) {
    if (!p) return null;
    const a = jfDate(p.startDate);
    const b = jfDate(p.endDate);
    if (!a || !b || b <= a) return null;
    return Math.min(1, Math.max(0, (t - a) / (b - a)));
  },
};

/** Diretta TV on the phone: the channels with what's on, a tap watches here. */
export function livePage() {
  const list = h('div', { class: 'channels' }, spinner('pad60'));
  const body = h('div', { class: 'content flush' }, list);
  const draw = () => {
    const j = jf();
    if (!guide.channels.length) return fill(list, h('p', { class: 'dim pad16' }, tr('No channels')));
    fill(
      list,
      guide.channels.map((ch) => {
        const [now, next] = guide.nowNext(ch.id);
        const row = h(
          'button',
          { class: 'chrow', type: 'button', onclick: () => openLive(ch.id) },
          h('div', { class: 'chlogo' }, img(j.poster(ch, 160), 'fit')),
          h(
            'div',
            { class: 'grow col gap2' },
            h('div', { class: 'subhead b' }, ch.name),
            h('div', { class: 'caption ellipsis' }, now?.name || tr('No guide')),
            progressLine(guide.progress(now)),
            next ? h('div', { class: 'caption2 dim ellipsis' }, `${hhmm(jfDate(next.startDate))} · ${next.name}`) : null,
          ),
        );
        onLong(row, () => menu([{ label: tr('Watch on the TV'), icon: 'tv', run: () => watchOnTV({ channel: ch.id }) }]));
        return row;
      }),
    );
  };
  const load = async (force) => {
    try {
      await guide.load(force);
      draw();
    } catch (e) {
      fill(list, h('p', { class: 'dim pad16' }, e.message));
    }
  };
  load();
  const timer = setInterval(draw, 30_000);
  const page = { title: tr('Live TV'), large: true, body, dispose: () => clearInterval(timer) };
  setTimeout(() => pullToRefresh(page.scroller, () => load(true)), 0);
  return page;
}

/** A channel playing on the phone, sideways. Swipe up / down (or ▲ ▼) changes channel. */
export function openLive(channelId) {
  music.yield();
  let channel = channelId;
  let gen = 0;
  let stop = null;
  let hideTimer;
  cover(
    (close) => {
      const video = h('video', { playsinline: true, 'webkit-playsinline': true, autoplay: true });
      const wait = h('div', { class: 'pwait' }, spinner());
      const err = h('div', { class: 'perror small', hidden: true });
      const info = h('div', { class: 'row gap16 grow' });
      const controls = h(
        'div',
        { class: 'pcontrols' },
        h(
          'div',
          { class: 'ptop' },
          iconBtn('close', () => close(), tr('Close'), 'glassbtn'),
          h('div', { class: 'grow' }),
          btn(tr('On the TV'), async () => {
            video.pause();
            await watchOnTV({ channel });
            close();
          }, { icon: 'tv' }),
        ),
        h('div', { class: 'grow' }),
        h('div', { class: 'row end gap16' }, info, h('div', { class: 'col gap8' }, iconBtn('up', () => zap(1), tr('Next channel'), 'glassbtn'), iconBtn('down', () => zap(-1), tr('Previous channel'), 'glassbtn'))),
      );
      const tap = h('div', { class: 'ptap' });
      const el = h('div', { class: 'player live' }, video, tap, wait, err, controls);
      const autoHide = () => {
        clearTimeout(hideTimer);
        hideTimer = setTimeout(() => el.classList.add('bare'), 5000);
      };
      tap.onclick = () => {
        el.classList.toggle('bare');
        if (!el.classList.contains('bare')) autoHide();
      };
      // swipes change channel
      let y0 = null;
      tap.addEventListener('pointerdown', (e) => (y0 = { x: e.clientX, y: e.clientY }));
      tap.addEventListener('pointerup', (e) => {
        if (!y0) return;
        const dx = e.clientX - y0.x;
        const dy = e.clientY - y0.y;
        y0 = null;
        if (Math.abs(dy) > 40 && Math.abs(dy) > Math.abs(dx)) zap(dy < 0 ? 1 : -1);
      });
      const drawInfo = () => {
        const j = jf();
        const ch = guide.channels.find((c) => c.id === channel);
        if (!ch) return fill(info);
        const [now, next] = guide.nowNext(ch.id);
        fill(
          info,
          h('div', { class: 'chlogo big' }, img(j.poster(ch, 160), 'fit')),
          h(
            'div',
            { class: 'col gap2 grow' },
            h('div', { class: 'headline' }, `${ch.channelNumber ? ch.channelNumber + ' · ' : ''}${ch.name}`),
            now ? h('div', { class: 'subhead ellipsis' }, `${hhmm(jfDate(now.startDate))}–${hhmm(jfDate(now.endDate))} · ${now.name}`) : null,
            now ? h('div', { style: { maxWidth: '320px' } }, progressLine(guide.progress(now))) : null,
            next ? h('div', { class: 'caption dim ellipsis' }, tr('Then {time} · {name}', { time: hhmm(jfDate(next.startDate)), name: next.name })) : null,
          ),
        );
      };
      /** Jellyfin opens the channel and makes HLS of it (a browser can't open most channels' own streams). */
      const tune = async (id) => {
        const mine = ++gen;
        channel = id;
        drawInfo();
        wait.hidden = false;
        err.hidden = true;
        stop?.();
        stop = null;
        const j = jf();
        try {
          const r = await j.send('POST', `Items/${id}/PlaybackInfo`, { userId: j.user }, {
            UserId: j.user,
            AutoOpenLiveStream: true,
            EnableDirectPlay: false,
            EnableDirectStream: false,
            DeviceProfile: { TranscodingProfiles: [{ Container: 'ts', Type: 'Video', VideoCodec: 'h264', AudioCodec: 'aac', Protocol: 'hls', Context: 'Streaming' }] },
          });
          if (mine !== gen) return;
          const t = r.mediaSources?.[0]?.transcodingUrl;
          if (!t) throw new Error(tr('Channel not available'));
          stop = await attachStream(video, j.url(t), { onFatal: () => mine === gen && showErr(tr('Channel not available')) });
          await video.play().catch(() => {});
          await new Promise((res) => (video.readyState >= 2 ? res() : video.addEventListener('playing', res, { once: true })));
          if (mine === gen) wait.hidden = true;
        } catch (e) {
          if (mine === gen) showErr(e.message);
        }
      };
      const showErr = (m) => {
        wait.hidden = true;
        err.hidden = false;
        err.textContent = m;
      };
      /** ▲ = next channel, as on the TV. */
      const zap = (dir) => {
        const list = guide.channels;
        const i = list.findIndex((c) => c.id === channel);
        if (i < 0 || !list.length) return;
        buzz();
        el.classList.remove('bare');
        autoHide();
        tune(list[(i + dir + list.length) % list.length].id);
      };
      guide.load().then(() => {
        drawInfo();
        tune(channel);
        autoHide();
      }, (e) => showErr(e.message));
      return el;
    },
    {
      cls: 'black',
      onClose: () => {
        gen++;
        clearTimeout(hideTimer);
        stop?.();
        landscape(false);
        keepAwake(false);
      },
    },
  );
  landscape(true);
  keepAwake(true);
}
