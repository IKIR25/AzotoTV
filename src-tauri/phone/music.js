// Musica on the phone: the profile's music library and playlists, playing
// here (a queue, the lock screen's controls through the Media Session API),
// each song reported to Jellyfin like on the TV.

import { tr, h, fill, icon, btn, iconBtn, spinner, img, toast, menu, sheet, on, emit, clock, attachStream, pullToRefresh, native, can, testMute } from './ui.js';
import { jf } from './api.js';
import { push } from './model.js';
import { searchField, shelf } from './watch.js';

const PINK = '#FF4FA3';

// ---- the player ----------------------------------------------------------------------

class MusicPlayer {
  queue = [];
  original = [];
  index = 0;
  shuffled = false;
  repeat = 'off'; // off | all | one
  loading = false;
  reported = null;
  lastReport = 0;
  session = '';

  constructor() {
    this.audio = new Audio();
    this.audio.preload = 'auto';
    this.audio.addEventListener('ended', () => this.ended());
    this.audio.addEventListener('timeupdate', () => this.tick());
    this.audio.addEventListener('play', () => this.changed());
    this.audio.addEventListener('pause', () => this.changed());
    this.audio.addEventListener('loadedmetadata', () => this.changed());
    const ms = navigator.mediaSession;
    if (ms) {
      const set = (a, fn) => {
        try {
          ms.setActionHandler(a, fn);
        } catch {}
      };
      set('play', () => this.resume());
      set('pause', () => this.pause());
      set('nexttrack', () => this.next());
      set('previoustrack', () => this.previous());
      set('seekto', (e) => this.seek(e.seekTime));
    }
  }

  get current() {
    return this.queue[this.index] || null;
  }
  get active() {
    return !!this.current;
  }
  get playing() {
    return this.active && !this.audio.paused;
  }
  get position() {
    return this.audio.currentTime || 0;
  }
  get duration() {
    const d = this.audio.duration;
    return isFinite(d) && d > 0 ? d : (this.current?.runTimeTicks || 0) / 1e7;
  }

  /** Plays `items` from `start` (shuffled: a random order, starting from that song). */
  play(items, start = 0, shuffle = false) {
    if (!items.length) return;
    this.j = jf();
    this.original = items;
    let list = items;
    let first = Math.min(Math.max(start, 0), items.length - 1);
    if (shuffle) {
      const pick = items[start === 0 ? Math.floor(Math.random() * items.length) : first];
      list = [pick, ...shuffleArr(items.filter((x) => x.id !== pick.id))];
      first = 0;
    }
    this.shuffled = shuffle;
    this.queue = list;
    this.load(first);
  }

  toggleShuffle() {
    const cur = this.current;
    if (!cur) return;
    if (this.shuffled) {
      this.queue = this.original;
      this.index = Math.max(0, this.original.findIndex((x) => x.id === cur.id));
    } else {
      this.queue = [cur, ...shuffleArr(this.original.filter((x) => x.id !== cur.id))];
      this.index = 0;
    }
    this.shuffled = !this.shuffled;
    this.changed();
  }

  cycleRepeat() {
    this.repeat = { off: 'all', all: 'one', one: 'off' }[this.repeat];
    this.changed();
  }

  async load(i) {
    if (!this.j || !this.queue[i]) return;
    this.stopReport();
    this.index = i;
    this.loading = true;
    this.session = crypto.randomUUID?.() || String(Math.random());
    const it = this.queue[i];
    const url = this.j.url(`Audio/${it.id}/universal`, {
      UserId: this.j.user,
      Container: 'mp3,aac,m4a,flac,wav,ogg,opus,webm',
      TranscodingContainer: 'aac',
      TranscodingProtocol: 'http',
      AudioCodec: 'aac',
      MaxStreamingBitrate: '320000000',
      PlaySessionId: this.session,
    });
    this.stop_?.();
    this.stop_ = await attachStream(this.audio, url);
    this.audio.muted = testMute;
    this.changed();
    try {
      await this.audio.play();
      if (this.current?.id !== it.id) return;
      this.reported = it.id;
      this.report('Sessions/Playing');
    } catch (e) {
      if (e.name !== 'AbortError') toast(e.message);
    }
    this.loading = false;
    this.updateNowPlaying();
    this.changed();
  }

  ended() {
    if (this.repeat === 'one') return this.load(this.index);
    if (this.repeat === 'all') return this.load(this.index + 1 < this.queue.length ? this.index + 1 : 0);
    if (this.index + 1 < this.queue.length) return this.load(this.index + 1);
    this.stopReport(true);
    this.changed();
  }

  toggle() {
    this.playing ? this.pause() : this.resume();
  }
  pause() {
    this.audio.pause();
    this.report('Sessions/Playing/Progress', { EventName: 'pause' });
  }
  resume() {
    if (!this.active) return;
    if (!this.audio.src && !this.audio.currentSrc) return this.load(this.index);
    this.audio.play().catch(() => {});
    this.report('Sessions/Playing/Progress', { EventName: 'unpause' });
  }
  next() {
    if (this.active) this.load(this.index + 1 < this.queue.length ? this.index + 1 : 0);
  }
  /** Back to the start of the song, or (in its first seconds) the one before. */
  previous() {
    if (!this.active) return;
    if (this.position > 4 || this.index === 0) this.seek(0);
    else this.load(this.index - 1);
  }
  jump(i) {
    this.load(i);
  }
  seek(s) {
    this.audio.currentTime = s;
  }

  /** Everything stops and the queue empties (the mini player goes away). */
  stop() {
    this.stopReport();
    this.audio.pause();
    this.stop_?.();
    this.stop_ = null;
    this.queue = [];
    this.original = [];
    if (navigator.mediaSession) navigator.mediaSession.metadata = null;
    if (can('musicState')) native.musicState('');
    this.changed();
  }

  /** A video starts on the phone: the music pauses. */
  yield() {
    if (this.playing) this.pause();
  }

  tick() {
    emit('music-time', this);
    if (this.playing && Date.now() - this.lastReport >= 10_000) this.report('Sessions/Playing/Progress', { EventName: 'timeupdate' });
  }

  report(path, extra = {}) {
    if (!this.j || !this.reported) return;
    this.lastReport = Date.now();
    this.j.report(path, {
      ItemId: this.reported,
      PlaySessionId: this.session,
      PositionTicks: Math.floor((path.endsWith('Stopped') && this._finished ? this.duration : this.position) * 1e7),
      IsPaused: !this.playing,
      PlayMethod: 'DirectPlay',
      CanSeek: true,
      ...extra,
    });
  }

  stopReport(finished = false) {
    if (!this.reported) return;
    this._finished = finished;
    this.report('Sessions/Playing/Stopped');
    this._finished = false;
    this.reported = null;
  }

  updateNowPlaying() {
    const it = this.current;
    if (!it) return;
    const artist = it.albumArtist || it.artists?.[0] || '';
    const art = this.j?.poster(it, 600);
    if (navigator.mediaSession && window.MediaMetadata) {
      navigator.mediaSession.metadata = new MediaMetadata({ title: it.name, artist, album: it.album || '', artwork: art ? [{ src: art, sizes: '600x600' }] : [] });
    }
  }

  changed() {
    if (navigator.mediaSession) navigator.mediaSession.playbackState = this.active ? (this.playing ? 'playing' : 'paused') : 'none';
    // the Android app keeps the music going in the background (and on the lock screen)
    if (can('musicState') && this.current) {
      const it = this.current;
      native.musicState(JSON.stringify({ title: it.name, artist: it.albumArtist || it.artists?.[0] || '', playing: this.playing }));
    }
    emit('music', this);
  }
}

function shuffleArr(a) {
  const out = [...a];
  for (let i = out.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [out[i], out[j]] = [out[j], out[i]];
  }
  return out;
}

export const music = new MusicPlayer();

// for tests and the Android app: where the music is
window.azotoMusicNow = () => ({ id: music.current?.id || null, playing: music.playing, at: music.position });

// the Android app's notification buttons
window.azotoMusic = (what) => {
  if (what === 'toggle') music.toggle();
  else if (what === 'next') music.next();
  else if (what === 'previous') music.previous();
  else if (what === 'stop') music.stop();
};

// ---- the library -----------------------------------------------------------------------

export function musicPage() {
  let library = null;
  const results = h('div', { class: 'col padh', hidden: true });
  const main = h('div', { class: 'col gap24' }, spinner('pad60'));
  const search = searchField(tr('Songs, albums, artists'), async (q, still) => {
    main.hidden = !!q;
    results.hidden = !q;
    if (!q) return;
    const p = await jf().items({ parent: library, types: 'MusicArtist,MusicAlbum,Audio', limit: 50, search: q });
    if (!still()) return;
    const songs = p.items.filter((x) => x.type === 'Audio');
    fill(results, p.items.map((it) => (it.type === 'Audio' ? songRow(it, () => music.play(songs, songs.indexOf(it))) : listRow(it))));
  });
  const body = h('div', { class: 'content flush' }, main, results);
  const playAll = async (shuffle) => {
    if (!library) return;
    const p = await jf().items({ parent: library, types: 'Audio', limit: 5000 });
    music.play(p.items, 0, shuffle);
  };
  const load = async () => {
    const j = jf();
    try {
      library = (await j.views()).find((v) => v.collectionType === 'music')?.id || null;
      const playlists = (await j.items({ types: 'Playlist', limit: 100 }).catch(() => ({ items: [] }))).items.filter((p) => p.mediaType === 'Audio');
      const recent = library ? (await j.items({ parent: library, types: 'MusicAlbum', sort: 'DateCreated', order: 'Descending', limit: 20 }).catch(() => ({ items: [] }))).items : [];
      fill(
        main,
        !library ? h('p', { class: 'dim padh' }, tr('No music library on the server')) : null,
        h(
          'div',
          { class: 'row gap12 padh' },
          btn(tr('Play'), () => playAll(false), { kind: 'primary pink', icon: 'play', cls: 'grow big', disabled: !library }),
          btn(tr('Shuffle'), () => playAll(true), { icon: 'shuffle', cls: 'grow big', disabled: !library }),
        ),
        h(
          'div',
          { class: 'menurows padh' },
          menuRow('stack', tr('Albums'), () => push(albumsPage(library))),
          menuRow('mic', tr('Artists'), () => push(artistsPage(library))),
          menuRow('note', tr('Songs'), () => push(songsPage(library))),
        ),
        playlists.length ? shelf(tr('Your playlists'), albumRow(playlists)) : null,
        recent.length ? shelf(tr('Recently added'), albumRow(recent)) : null,
      );
    } catch (e) {
      fill(main, h('p', { class: 'dim padh' }, e.message));
    }
  };
  load();
  const page = { title: tr('Music'), large: true, body, under: () => search };
  setTimeout(() => pullToRefresh(page.scroller, load), 0);
  return page;
}

function menuRow(ic, title, onclick) {
  return h('button', { class: 'menurow', type: 'button', onclick }, icon(ic, 'pink'), h('span', { class: 'grow' }, title), icon('chevron', 'dim'));
}

/** Square covers in a row (albums, playlists). */
function albumRow(items) {
  return h('div', { class: 'hrow' }, items.map((it) => albumCard(it, 'w140')));
}

function coverImg(it, width, round) {
  const url = jf().poster(it, width);
  return h('div', { class: 'art' + (round ? ' round' : '') }, url ? img(url) : h('div', { class: 'imgbox broken' }, icon('note')));
}

function albumCard(it, cls = '') {
  return h(
    'button',
    { class: 'albumcard ' + cls, type: 'button', onclick: () => push(trackListPage(it)) },
    coverImg(it, 400, it.type === 'MusicArtist'),
    h('div', { class: 'caption b ellipsis' }, it.name),
    it.albumArtist ? h('div', { class: 'caption2 dim ellipsis' }, it.albumArtist) : null,
  );
}

/** An album, artist or playlist in a list. */
function listRow(it) {
  const kind = it.type === 'MusicArtist' ? tr('Artist') : it.type === 'Playlist' ? tr('Playlist') : [tr('Album'), it.albumArtist, it.productionYear].filter(Boolean).join(' · ');
  return h(
    'button',
    { class: 'listrow', type: 'button', onclick: () => push(trackListPage(it)) },
    h('div', { class: 'w52' }, coverImg(it, 160, it.type === 'MusicArtist')),
    h('div', { class: 'grow col' }, h('div', { class: 'subhead ellipsis' }, it.name), h('div', { class: 'caption dim ellipsis' }, kind)),
    icon('chevron', 'dim'),
  );
}

/** A song: a tap plays it (and the rest of its list). */
function songRow(it, play, number) {
  const title = h('div', { class: 'subhead ellipsis' }, it.name);
  const el = h(
    'button',
    { class: 'listrow song', type: 'button', onclick: play },
    number != null ? h('div', { class: 'num dim' }, String(number)) : h('div', { class: 'w46' }, coverImg(it, 160)),
    h('div', { class: 'grow col' }, title, h('div', { class: 'caption dim ellipsis' }, it.albumArtist || it.artists?.[0] || '')),
    it.runTimeTicks ? h('div', { class: 'caption dim mono' }, clock(it.runTimeTicks / 1e7)) : null,
  );
  const mark = () => title.classList.toggle('pinktext', music.current?.id === it.id);
  mark();
  on('music', mark, el);
  return el;
}

/** An album, a playlist or an artist's songs. */
function trackListPage(list) {
  let songs = [];
  const j = jf();
  const rows = h('div', { class: 'col' }, spinner());
  const summary = h('div', { class: 'caption dim' });
  const albums = h('div');
  const play = (i, shuffle) => songs.length && music.play(songs, i, shuffle);
  const body = h(
    'div',
    { class: 'content tracklist' },
    h('div', { class: 'bigcover' }, coverImg(list, 600, list.type === 'MusicArtist')),
    h(
      'div',
      { class: 'tc col gap4' },
      h('div', { class: 'title2' }, list.name),
      list.type === 'MusicAlbum' && list.albumArtist ? h('div', { class: 'pinktext' }, list.albumArtist) : null,
      summary,
    ),
    h('div', { class: 'row gap12' }, btn(tr('Play'), () => play(0, false), { kind: 'primary pink', icon: 'play', cls: 'grow big' }), btn(tr('Shuffle'), () => play(0, true), { icon: 'shuffle', cls: 'grow big' })),
    albums,
    rows,
  );
  (async () => {
    try {
      if (list.type === 'Playlist') {
        songs = (await j.get(`Playlists/${list.id}/Items`, { userId: j.user, Fields: 'RunTimeTicks', EnableImageTypes: 'Primary' })).items;
      } else if (list.type === 'MusicArtist') {
        const q = { ArtistIds: list.id };
        songs = (await j.items({ types: 'Audio', sort: 'Album,ParentIndexNumber,IndexNumber,SortName', limit: 2000, extra: q })).items;
        const al = (await j.items({ types: 'MusicAlbum', sort: 'ProductionYear,SortName', order: 'Descending', limit: 200, extra: q })).items;
        if (al.length) fill(albums, h('div', { class: 'col gap8 bleed' }, h('div', { class: 'headline padh' }, tr('Albums')), albumRow(al)));
      } else {
        songs = (await j.items({ parent: list.id, types: 'Audio', sort: 'ParentIndexNumber,IndexNumber,SortName', limit: 500 })).items;
      }
      const total = Math.floor(songs.reduce((s, x) => s + (x.runTimeTicks || 0), 0) / 600_000_000);
      summary.textContent = [tr('{n} songs', { n: songs.length }), total ? (total >= 60 ? `${Math.floor(total / 60)} h ${total % 60} min` : `${total} min`) : null, list.productionYear]
        .filter(Boolean)
        .join(' · ');
      fill(rows, songs.length ? songs.map((s, i) => songRow(s, () => play(i, false), list.type === 'MusicAlbum' ? s.indexNumber ?? i + 1 : null)) : h('p', { class: 'dim tc' }, tr('No songs')));
    } catch (e) {
      fill(rows, h('p', { class: 'dim' }, e.message));
    }
  })();
  return { title: '', short: list.name, body };
}

function albumsPage(library) {
  const grid = h('div', { class: 'albumgrid pad16' }, spinner());
  let sort = 'SortName';
  const SORTS = [
    ['SortName', tr('Title')],
    ['AlbumArtist,SortName', tr('Artist')],
    ['DateCreated', tr('Recently added')],
    ['ProductionYear,SortName', tr('Year')],
  ];
  const load = async () => {
    const order = sort === 'DateCreated' || sort.startsWith('ProductionYear') ? 'Descending' : 'Ascending';
    const p = await jf().items({ parent: library, types: 'MusicAlbum', sort, order, limit: 2000 }).catch((e) => (toast(e.message), { items: [] }));
    fill(grid, p.items.map((it) => albumCard(it)));
  };
  load();
  return {
    title: tr('Albums'),
    body: h('div', { class: 'content flush' }, grid),
    actions: () => [iconBtn('sort', () => menu(SORTS.map(([k, label]) => ({ label, checked: k === sort, run: () => ((sort = k), load()) }))), tr('Sort'), 'navbtn')],
  };
}

function artistsPage(library) {
  const list = h('div', { class: 'col padh' }, spinner());
  (async () => {
    const j = jf();
    const q = { userId: j.user, SortBy: 'SortName', Limit: 2000, EnableImageTypes: 'Primary' };
    if (library) q.ParentId = library;
    const p = await j.get('Artists/AlbumArtists', q).catch((e) => (toast(e.message), { items: [] }));
    fill(list, p.items.map((it) => listRow(it)));
  })();
  return { title: tr('Artists'), body: h('div', { class: 'content flush' }, list) };
}

function songsPage(library) {
  const list = h('div', { class: 'col padh' }, spinner());
  (async () => {
    const p = await jf().items({ parent: library, types: 'Audio', limit: 5000 }).catch((e) => (toast(e.message), { items: [] }));
    // a long list: drawn in slices so the page opens at once
    const items = p.items;
    fill(list);
    let i = 0;
    const slice = () => {
      list.append(...items.slice(i, i + 200).map((it, k) => songRow(it, () => music.play(items, i0(it)))));
      i += 200;
      if (i < items.length) setTimeout(slice, 30);
    };
    const i0 = (it) => items.indexOf(it);
    slice();
  })();
  return { title: tr('Songs'), body: h('div', { class: 'content flush' }, list) };
}

// ---- now playing -------------------------------------------------------------------------

/** Above the tabs while music plays: the song, play / pause, next; a tap opens the full player. */
export function miniPlayer() {
  const el = h('div', { class: 'mini', hidden: true });
  const draw = () => {
    const it = music.current;
    el.hidden = !it;
    if (!it) return fill(el);
    fill(
      el,
      h(
        'button',
        { class: 'minimain', type: 'button', onclick: () => nowPlaying() },
        h('div', { class: 'w38' }, coverImg(it, 160)),
        h('div', { class: 'grow col' }, h('div', { class: 'subhead ellipsis' }, it.name), h('div', { class: 'caption dim ellipsis' }, it.albumArtist || it.artists?.[0] || '')),
      ),
      iconBtn(music.playing ? 'pause' : 'play', () => music.toggle(), tr('Play/Pause'), 'plain'),
      iconBtn('ff', () => music.next(), tr('Next'), 'plain'),
    );
  };
  let last = '';
  on('music', () => {
    const key = `${music.current?.id}|${music.playing}`;
    if (key !== last) {
      last = key;
      draw();
    }
  });
  draw();
  return el;
}

function nowPlaying() {
  sheet(
    (close) => {
      let showQueue = false;
      let scrubbing = false;
      const bg = h('div', { class: 'npbg' });
      const art = h('div', { class: 'npart' });
      const queue = h('div', { class: 'npqueue', hidden: true });
      const title = h('div', { class: 'title2 ellipsis' });
      const artist = h('div', { class: 'title3 dim ellipsis' });
      const seek = h('input', { class: 'seek white', type: 'range', min: 0, max: 1, step: 'any', value: 0 });
      const cur = h('span', {}, '0:00');
      const left = h('span', {}, '-0:00');
      const shuffleBtn = iconBtn('shuffle', () => music.toggleShuffle(), tr('Shuffle'), 'plain');
      const repeatBtn = iconBtn('repeat', () => music.cycleRepeat(), tr('Repeat'), 'plain');
      const playBtn = iconBtn('play', () => music.toggle(), tr('Play/Pause'), 'plain huge');
      const queueBtn = btn(tr('Queue'), () => {
        showQueue = !showQueue;
        queue.hidden = !showQueue;
        art.hidden = showQueue;
        queueBtn.classList.toggle('pinktext', showQueue);
        if (showQueue) drawQueue();
      }, { icon: 'list', kind: 'plain' });
      seek.addEventListener('input', () => {
        scrubbing = true;
        cur.textContent = clock(+seek.value);
      });
      seek.addEventListener('change', () => {
        scrubbing = false;
        music.seek(+seek.value);
      });
      const drawQueue = () =>
        fill(
          queue,
          music.queue.map((s, i) => {
            const r = songRow(s, () => music.jump(i));
            if (i < music.index) r.style.opacity = 0.5;
            return r;
          }),
        );
      const el = h(
        'div',
        { class: 'nowplaying' },
        bg,
        art,
        queue,
        h('div', { class: 'col' }, title, artist),
        h('div', { class: 'col gap4' }, seek, h('div', { class: 'ptimes' }, cur, left)),
        h(
          'div',
          { class: 'npbuttons' },
          shuffleBtn,
          iconBtn('prev', () => music.previous(), tr('Previous'), 'plain big'),
          playBtn,
          iconBtn('next', () => music.next(), tr('Next'), 'plain big'),
          repeatBtn,
        ),
        h('div', { class: 'row' }, btn(tr('Stop'), () => (music.stop(), close()), { icon: 'stop', kind: 'plain' }), h('div', { class: 'grow' }), queueBtn),
      );
      let lastId = null;
      const draw = () => {
        const it = music.current;
        if (!it) return close();
        if (it.id !== lastId) {
          lastId = it.id;
          const url = jf().poster(it, 800);
          fill(art, coverImg(it, 800));
          bg.style.backgroundImage = url ? `url("${url}")` : '';
          title.textContent = it.name;
          artist.textContent = it.albumArtist || it.artists?.[0] || '';
          if (showQueue) drawQueue();
        }
        art.classList.toggle('small', !music.playing);
        fill(playBtn, icon(music.playing ? 'pause' : 'play'));
        shuffleBtn.classList.toggle('pinktext', music.shuffled);
        fill(repeatBtn, icon(music.repeat === 'one' ? 'repeat1' : 'repeat'));
        repeatBtn.classList.toggle('pinktext', music.repeat !== 'off');
        time();
      };
      const time = () => {
        if (scrubbing) return;
        seek.max = Math.max(music.duration, 1);
        seek.value = music.position;
        cur.textContent = clock(music.position);
        left.textContent = '-' + clock(Math.max(0, music.duration - music.position));
      };
      draw();
      on('music', draw, el);
      on('music-time', time, el);
      return el;
    },
    { full: true, cls: 'npsheet' },
  );
}
