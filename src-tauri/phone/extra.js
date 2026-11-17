// Giochi (the Mobile games: web versions open here, touch "phone modes"
// stream from the server in the apps), Richieste (Seerr: search, ask, watch
// them arrive) and the game screens streamed to the apps.

import { tr, h, fill, icon, btn, spinner, img, toast, progressLine, clock, native, can, pullToRefresh, on } from './ui.js';
import { api, token, account, seerr, tmdb, jf, login } from './api.js';
import { model, push, loadHome } from './model.js';
import { searchField, itemPage } from './watch.js';

// ---- the streamed screens (the Android app; the iPhone app has its own) ----------------------

/** What the native app needs to stream: the server, the key, the profile. */
function streamArgs(extra) {
  return JSON.stringify({ server: location.origin, token: token(), user: account()?.id || '', ...extra });
}

/** A Switch game's touch screen / a 3DS game's lower half, on the phone. */
export function openScreen(game) {
  if (can('openScreen')) return native.openScreen(streamArgs({ name: game.name, console: game.console || '' }));
  toast(tr('Only in the AzotoTV app for Android or iPhone'));
}

function playStreamed(g) {
  if (can('playGame')) return native.playGame(streamArgs({ id: g.id, name: g.name }));
  toast(tr('Only in the AzotoTV app for Android or iPhone'));
}

// ---- Giochi ------------------------------------------------------------------------------

export function gamesPage() {
  const grid = h('div', { class: 'grid2 games' });
  const body = h('div', { class: 'content' }, grid);
  const streams = can('playGame');
  const draw = () => {
    if (!model.games.length) return fill(grid, h('p', { class: 'dim tc span2 pad30' }, tr('No Mobile games on the TV')));
    fill(
      grid,
      model.games.map((g) => {
        const ok = !!g.web || (g.stream && streams);
        const kind = ['cover', 'header', 'hero'].find((k) => g.art.includes(k));
        const cell = h(
          'button',
          {
            class: 'gamecell' + (ok ? '' : ' off'),
            type: 'button',
            onclick: () => {
              if (g.web) openWeb(g.web);
              else if (g.stream) playStreamed(g);
              else toast(tr('Not playable on the phone yet'));
            },
          },
          h(
            'div',
            { class: 'gameart' },
            kind ? img(`/api/app/art/${g.id}/${kind}?k=${token()}`) : h('div', { class: 'imgbox broken' }, icon('pad')),
            !g.web && !g.stream ? h('span', { class: 'tag' }, tr('TV only')) : null,
          ),
          h('div', { class: 'subhead ellipsis' }, g.name),
        );
        return cell;
      }),
    );
  };
  draw();
  on('home', draw, body);
  const page = { title: tr('Games'), large: true, body };
  setTimeout(() => pullToRefresh(page.scroller, loadHome), 0);
  return page;
}

/** A game's web version: a new tab (the Android app opens it in the browser). */
function openWeb(url) {
  const a = h('a', { href: url, target: '_blank', rel: 'noopener' });
  document.body.append(a);
  a.click();
  a.remove();
}

// ---- Richieste ------------------------------------------------------------------------------

const STATUS = { 5: 'Available', 4: 'Partly available', 3: 'Coming', 2: 'Requested' };
const statusText = (s) => (STATUS[s] ? tr(STATUS[s]) : null);
const inFlight = (s) => s === 2 || s === 3 || s === 4;
const label = (it) => it.title || it.name || '';
const year = (it) => (it.releaseDate || it.firstAirDate || '').slice(0, 4);
const kindOf = (it) => it.mediaType || 'movie';

const STAGES = {
  unreleased: 'Not out yet',
  search: 'Looking for it',
  wait: 'Waiting',
  cloud: 'Getting ready',
  download: 'Downloading',
  unpack: 'Unpacking',
  import: 'Almost ready',
  error: 'Problem',
};

export function requestsPage() {
  const main = h('div', { class: 'col gap24' }, spinner('pad60'));
  const results = h('div', { class: 'sgrid padh', hidden: true });
  const search = searchField(tr('Search a film or a series'), async (q, still) => {
    main.hidden = !!q;
    results.hidden = !q;
    if (!q) return;
    const p = await seerr('GET', '/search', { query: q, page: '1' });
    if (!still()) return;
    fill(results, (p.results || []).filter((x) => x.mediaType === 'movie' || x.mediaType === 'tv').map((it) => sCard(it)));
  });
  const body = h('div', { class: 'content flush' }, main, results);
  const load = async () => {
    const a = account();
    let status;
    try {
      status = await api('/api/app/seerr/status', { query: { user: a.id } });
    } catch (e) {
      return fill(main, h('p', { class: 'dim pad30' }, e.message));
    }
    if (!status.enabled || !status.user) {
      return fill(
        main,
        h('p', { class: 'dim pad30' }, status.enabled ? tr("Your profile isn't linked to Seerr: ask whoever runs the server.") : tr("Requests aren't available on this server.")),
      );
    }
    let trending = [];
    let error = null;
    try {
      trending = ((await seerr('GET', '/discover/trending', { page: '1' })).results || []).filter((x) => x.mediaType === 'movie' || x.mediaType === 'tv');
    } catch (e) {
      error = e.message;
    }
    // the profile's own requests, newest first, with their details (titles, posters)
    const mine = [];
    try {
      const r = await seerr('GET', '/request', { take: '20', skip: '0', sort: 'added', filter: 'all' });
      for (const m of (r.results || []).map((x) => x.media).filter(Boolean)) {
        if (!m.tmdbId || !m.mediaType || mine.some((x) => x.id === m.tmdbId && kindOf(x) === m.mediaType)) continue;
        try {
          const d = await seerr('GET', `/${m.mediaType}/${m.tmdbId}`);
          mine.push({ ...d, mediaType: m.mediaType });
        } catch {}
      }
    } catch {}
    fill(
      main,
      error ? h('p', { class: 'dim padh' }, error) : null,
      mine.length ? h('div', { class: 'col gap8' }, h('h2', { class: 'title3 padh' }, tr('Your requests')), h('div', { class: 'hrow' }, mine.map((it) => sCard(it, true, 'w118')))) : null,
      trending.length ? h('div', { class: 'col gap8' }, h('h2', { class: 'title3 padh' }, tr('Trending')), h('div', { class: 'sgrid padh' }, trending.map((it) => sCard(it)))) : null,
    );
  };
  load();
  const page = { title: tr('Requests'), large: true, body, under: () => search };
  setTimeout(() => pullToRefresh(page.scroller, load), 0);
  return page;
}

function sCard(it, progress = false, cls = '') {
  const s = it.mediaInfo?.status;
  return h(
    'button',
    { class: 'poster ' + cls, type: 'button', onclick: () => push(requestDetail(it)) },
    h(
      'div',
      { class: 'pimg' },
      img(tmdb(it.posterPath)),
      statusText(s) ? h('div', { class: 'tl' }, h('span', { class: 'stag' + (s === 5 ? ' green' : '') }, statusText(s))) : null,
      progress && inFlight(s) ? h('div', { class: 'bl' }, dlBar(it, true)) : null,
    ),
    h('div', { class: 'caption b clamp2' }, label(it)),
    h('div', { class: 'caption2 dim' }, [kindOf(it) === 'tv' ? tr('Series') : tr('Film'), year(it)].filter(Boolean).join(' · ')),
  );
}

/** How far a request is (downloads.rs), refreshed every few seconds while it's on its way. */
function dlBar(it, compact = false) {
  const el = h('div', { class: 'dlbar' });
  let timer;
  const tick = async () => {
    if (!el.isConnected && el._seen) return;
    if (el.isConnected) el._seen = true;
    try {
      const r = await api('/api/app/dl', { query: { user: account().id, media: kindOf(it), tmdb: String(it.id) } });
      const p = r.progress;
      if (p) {
        const bits = [
          p.speed ? `${(p.speed / 1e6).toFixed(1)} MB/s` : null,
          p.eta ? tr('{time} to go', { time: clock(p.eta) }) : null,
          p.episodes?.length === 2 ? tr('{a} of {b} episodes', { a: p.episodes[0], b: p.episodes[1] }) : null,
          p.note,
        ].filter(Boolean);
        fill(
          el,
          compact ? null : h('div', { class: 'row' }, h('div', { class: 'subhead grow' }, tr(STAGES[p.stage] || 'Requested')), h('div', { class: 'subhead mono' }, `${Math.round(p.overall * 100)}%`)),
          progressLine(Math.max(0.02, p.overall)),
          !compact && bits.length ? h('div', { class: 'caption dim' }, bits.join(' · ')) : null,
        );
      }
    } catch {}
    timer = setTimeout(tick, compact ? 15000 : 3000);
  };
  setTimeout(tick, 0);
  return el;
}

function requestDetail(start) {
  let item = start;
  const kind = kindOf(start);
  let picked = new Set();
  let busy = false;
  let done = null;
  let library = null;
  const body = h('div', { class: 'content flush item' });

  const meta = () =>
    [
      kind === 'tv' ? tr('Series') : tr('Film'),
      year(item),
      item.runtime > 0 ? `${item.runtime} min` : null,
      item.voteAverage > 0 ? `★ ${item.voteAverage.toFixed(1)}` : null,
      item.genres?.length ? item.genres.slice(0, 2).map((g) => g.name).join(', ') : null,
    ]
      .filter(Boolean)
      .join(' · ');
  /** seasons not yet asked for */
  const missing = () => {
    const have = new Set((item.mediaInfo?.seasons || []).filter((s) => s.status >= 2).map((s) => s.seasonNumber));
    return (item.seasons || []).map((s) => s.seasonNumber).filter((n) => n > 0 && !have.has(n));
  };
  const canAsk = () => {
    const s = item.mediaInfo?.status ?? 1;
    return kind === 'movie' ? s <= 1 : s !== 5 && missing().length > 0;
  };
  const askLabel = () => {
    if (kind !== 'tv') return tr('Request');
    if (!picked.size) return tr('Request all seasons');
    return picked.size === 1 ? tr('Request season {n}', { n: [...picked][0] }) : tr('Request {n} seasons', { n: picked.size });
  };

  const draw = () => {
    const s = item.mediaInfo?.status;
    const seasons = (item.seasons || []).filter((x) => x.seasonNumber > 0);
    fill(
      body,
      h('div', { class: 'itemhead' }, img(tmdb(item.backdropPath, 'w1280'), 'backdrop')),
      h(
        'div',
        { class: 'itembody' },
        h(
          'div',
          { class: 'row gap16 top' },
          h('div', { class: 'w96' }, h('div', { class: 'pimg' }, img(tmdb(item.posterPath)))),
          h(
            'div',
            { class: 'col gap4 grow' },
            h('div', { class: 'title2' }, label(item)),
            h('div', { class: 'caption dim' }, meta()),
            statusText(s) ? h('div', { class: 'caption b ' + (s === 5 ? 'green' : 'accent') }, statusText(s)) : null,
          ),
        ),
        item.tagline ? h('p', { class: 'callout i' }, item.tagline) : null,
        library ? btn(tr('Watch'), () => push(itemPage(library)), { kind: 'primary green', icon: 'play', cls: 'big wide' }) : null,
        done
          ? h('div', { class: 'row gap8 green' }, icon('checkCircle'), done)
          : canAsk()
            ? btn(askLabel(), () => ask(), { kind: 'primary', icon: 'plus', cls: 'big wide' + (busy ? ' busy' : ''), disabled: busy })
            : null,
        inFlight(s) ? dlBar(item) : null,
        item.overview ? h('p', { class: 'callout dim' }, item.overview) : null,
        kind === 'tv' && seasons.length && canAsk()
          ? h(
              'div',
              { class: 'col' },
              h('div', { class: 'headline' }, tr('Seasons')),
              seasons.map((x) => {
                const asked = !missing().includes(x.seasonNumber);
                return h(
                  'button',
                  {
                    class: 'seasonrow',
                    type: 'button',
                    disabled: asked,
                    onclick: () => {
                      if (picked.has(x.seasonNumber)) picked.delete(x.seasonNumber);
                      else picked.add(x.seasonNumber);
                      draw();
                    },
                  },
                  icon(asked ? 'checkCircle' : picked.has(x.seasonNumber) ? 'dot' : 'circle', asked ? 'green' : 'accent'),
                  h('span', { class: 'grow' }, x.name || tr('Season {n}', { n: x.seasonNumber })),
                  x.episodeCount ? h('span', { class: 'caption dim' }, tr('{n} episodes', { n: x.episodeCount })) : null,
                );
              }),
            )
          : null,
      ),
    );
  };

  const refresh = async () => {
    try {
      item = { ...(await seerr('GET', `/${kind}/${item.id}`)), mediaType: kind };
    } catch {}
    const jid = item.mediaInfo?.jellyfinMediaId;
    if (jid) library = await jf().item(jid).catch(() => null);
    draw();
  };

  // never pressed in tests: it starts real downloads
  const ask = async () => {
    busy = true;
    draw();
    try {
      const seasons = kind === 'tv' ? (picked.size ? [...picked].sort((a, b) => a - b) : missing()) : undefined;
      await seerr('POST', '/request', {}, { mediaType: kind, mediaId: item.id, seasons });
      done = tr('Requested: it will show up in the library by itself');
      picked = new Set();
      await refresh();
    } catch (e) {
      toast(e.message);
    }
    busy = false;
    draw();
  };
  draw();
  refresh();
  return { title: '', short: label(item), body };
}
