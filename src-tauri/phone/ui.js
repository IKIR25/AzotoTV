// The phone page's building blocks: translations, elements, icons, the
// navigation (a stack of pages per tab, full-screen covers, sheets), toasts.
// The page mirrors the iPhone app screen for screen; the same file runs in
// Safari / Chrome, as an iPhone home-screen app and inside the Android app
// (which adds `window.AzotoApp`, see android/).

// ---- language ---------------------------------------------------------------

// /i18n.js sets window.I18N = { lang, dict }: English text -> the TV's language.
// A note after a bar ("Play|game") only tells two translations apart.
const I18N = window.I18N || { lang: 'en', dict: {} };
document.documentElement.lang = I18N.lang;

export function tr(en, values) {
  let text = I18N.dict[en] || en.split('|')[0];
  if (values) for (const name in values) text = text.split(`{${name}}`).join(String(values[name]));
  return text;
}

export const lang = I18N.lang;
export const native = window.AzotoApp || null;
/** Does the Android app offer `name` (newer versions add more)? */
export const can = (name) => !!(native && typeof native[name] === 'function');

// ---- elements ---------------------------------------------------------------

export function h(tag, props, ...kids) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(props || {})) {
    if (v == null || v === false) continue;
    if (k.startsWith('on')) el.addEventListener(k.slice(2), v);
    else if (k === 'class') el.className = v;
    else if (k === 'html') el.innerHTML = v;
    else if (k === 'style' && typeof v === 'object') Object.assign(el.style, v);
    else el.setAttribute(k, v === true ? '' : v);
  }
  for (const kid of kids.flat()) if (kid != null && kid !== false) el.append(kid);
  return el;
}

/** Replaces `el`'s children. */
export function fill(el, ...kids) {
  el.replaceChildren(...kids.flat().filter((k) => k != null && k !== false));
  return el;
}

export function buzz(ms = 8) {
  try {
    if (native && native.vibrate) native.vibrate(ms);
    else if (navigator.vibrate) navigator.vibrate(ms);
  } catch {}
}

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ---- icons (24x24 strokes) ----------------------------------------------------

const I = {
  up: '<path d="M6 15l6-6 6 6"/>',
  down: '<path d="M6 9l6 6 6-6"/>',
  left: '<path d="M15 6l-6 6 6 6"/>',
  right: '<path d="M9 6l6 6-6 6"/>',
  back: '<path d="M9 14l-5-5 5-5"/><path d="M4 9h10.5a5.5 5.5 0 010 11H11"/>',
  home: '<path d="M3 10.5L12 3l9 7.5"/><path d="M5 9v11h14V9"/>',
  grid: '<rect x="4" y="4" width="7" height="7" rx="2" fill="currentColor"/><rect x="13" y="4" width="7" height="7" rx="2" fill="currentColor"/><rect x="4" y="13" width="7" height="7" rx="2" fill="currentColor"/><rect x="13" y="13" width="7" height="7" rx="2" fill="currentColor"/>',
  menu: '<path d="M4 7h16M4 12h16M4 17h16"/>',
  power: '<path d="M12 3v9"/><path d="M6.3 6.8a8 8 0 1011.4 0"/>',
  playpause: '<path d="M4 5v14l7-7z" fill="currentColor"/><path d="M15 5v14M20 5v14"/>',
  play: '<path d="M7 4.5v15l12-7.5z" fill="currentColor" stroke-linejoin="round"/>',
  pause: '<path d="M8 5v14M16 5v14" stroke-width="3.2"/>',
  stop: '<rect x="6" y="6" width="12" height="12" rx="2" fill="currentColor"/>',
  rew: '<path d="M11 6l-7 6 7 6z" fill="currentColor"/><path d="M20 6l-7 6 7 6z" fill="currentColor"/>',
  ff: '<path d="M4 6l7 6-7 6z" fill="currentColor"/><path d="M13 6l7 6-7 6z" fill="currentColor"/>',
  prev: '<path d="M18 6l-9 6 9 6z" fill="currentColor"/><path d="M6 5v14" stroke-width="2.6"/>',
  next: '<path d="M6 6l9 6-9 6z" fill="currentColor"/><path d="M18 5v14" stroke-width="2.6"/>',
  back10: '<path d="M4 12a8 8 0 108-8H8"/><path d="M10 1.5L7.5 4 10 6.5"/><text x="12" y="15.5" font-size="7.5" font-weight="700" text-anchor="middle" fill="currentColor" stroke="none">10</text>',
  fwd30: '<path d="M20 12a8 8 0 11-8-8h4"/><path d="M14 1.5L16.5 4 14 6.5"/><text x="12" y="15.5" font-size="7.5" font-weight="700" text-anchor="middle" fill="currentColor" stroke="none">30</text>',
  minus: '<path d="M5 12h14"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  remote: '<rect x="7" y="2.5" width="10" height="19" rx="5"/><circle cx="12" cy="8.5" r="2.2"/><path d="M10 14h.01M14 14h.01M10 17.5h.01M14 17.5h.01"/>',
  photo: '<rect x="3" y="4" width="18" height="16" rx="3"/><circle cx="9" cy="10" r="2"/><path d="M21 16l-5-5-9 9"/>',
  photos: '<rect x="6" y="3" width="15" height="13" rx="2.5"/><path d="M3 7v11a3 3 0 003 3h11"/><circle cx="11" cy="8" r="1.6"/><path d="M21 13l-4-4-6 6"/>',
  send: '<path d="M21 3L10 14"/><path d="M21 3l-7 18-4-7-7-4z" stroke-linejoin="round"/>',
  link: '<path d="M10 14a4 4 0 005.7 0l3-3a4 4 0 00-5.7-5.7l-1 1"/><path d="M14 10a4 4 0 00-5.7 0l-3 3a4 4 0 005.7 5.7l1-1"/>',
  share: '<path d="M12 3v12"/><path d="M8 7l4-4 4 4"/><path d="M5 12v6a3 3 0 003 3h8a3 3 0 003-3v-6"/>',
  paste: '<rect x="6" y="4" width="12" height="17" rx="2.5"/><path d="M9 4V3h6v1"/>',
  keyboard: '<rect x="2.5" y="6" width="19" height="12" rx="2.5"/><path d="M6 10h.01M10 10h.01M14 10h.01M18 10h.01M7 14h10"/>',
  pad: '<path d="M7.5 8h9a5 5 0 0 1 4.8 6.4l-1.1 3.6a2 2 0 0 1-3.5.6L14.8 16H9.2l-1.9 2.6a2 2 0 0 1-3.5-.6l-1.1-3.6A5 5 0 0 1 7.5 8z" fill="currentColor"/><path d="M8 11v3M6.5 12.5h3" stroke="var(--bg)"/><path d="M15.5 12h.01M17.5 13.5h.01" stroke="var(--bg)" stroke-width="2.4"/>',
  close: '<path d="M6 6l12 12M18 6L6 18"/>',
  check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
  checkCircle: '<circle cx="12" cy="12" r="9" fill="currentColor"/><path d="M8 12.5l3 3 5-6" stroke="var(--on-fill, #fff)"/>',
  circle: '<circle cx="12" cy="12" r="9"/>',
  dot: '<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="4.5" fill="currentColor"/>',
  tv: '<rect x="3" y="5" width="18" height="12" rx="2.5"/><path d="M8 21h8"/>',
  tvOff: '<rect x="3" y="5" width="18" height="12" rx="2.5"/><path d="M8 21h8M3 3l18 18"/>',
  phone: '<rect x="7" y="2.5" width="10" height="19" rx="2.5"/><path d="M11 18.5h2"/>',
  live: '<circle cx="12" cy="12" r="2.2" fill="currentColor"/><path d="M8.5 8.5a5 5 0 000 7M15.5 8.5a5 5 0 010 7M5.6 5.6a9 9 0 000 12.8M18.4 5.6a9 9 0 010 12.8"/>',
  film: '<rect x="3" y="7" width="15" height="13" rx="2"/><path d="M6 4h15v13"/><path d="M3 11h15"/>',
  music: '<path d="M9 18V5l11-2v13"/><circle cx="6" cy="18" r="3" fill="currentColor"/><circle cx="17" cy="16" r="3" fill="currentColor"/>',
  youtube: '<rect x="2.5" y="5" width="19" height="14" rx="4" fill="currentColor"/><path d="M10 9v6l5-3z" fill="var(--on-fill, #000)" stroke="none"/>',
  gallery: '<rect x="3" y="5" width="15" height="13" rx="2.5"/><path d="M6 21h12a3 3 0 003-3V8"/><circle cx="8" cy="9.5" r="1.5"/><path d="M18 14l-4-4-8 8"/>',
  requests: '<circle cx="10.5" cy="10.5" r="6.5"/><path d="M15.5 15.5L21 21"/><path d="M10.5 7.5v6M7.5 10.5h6"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="M16.5 16.5L21 21"/>',
  heart: '<path d="M12 20s-7.5-4.6-9-9.3C2 7.4 4.2 4.5 7.3 4.5c2 0 3.5 1.1 4.7 2.8 1.2-1.7 2.7-2.8 4.7-2.8 3.1 0 5.3 2.9 4.3 6.2C19.5 15.4 12 20 12 20z"/>',
  heartFill: '<path d="M12 20s-7.5-4.6-9-9.3C2 7.4 4.2 4.5 7.3 4.5c2 0 3.5 1.1 4.7 2.8 1.2-1.7 2.7-2.8 4.7-2.8 3.1 0 5.3 2.9 4.3 6.2C19.5 15.4 12 20 12 20z" fill="currentColor"/>',
  restart: '<path d="M5 5v14"/><path d="M19 5l-10 7 10 7z" fill="currentColor"/>',
  captions: '<rect x="3" y="5" width="18" height="14" rx="3"/><path d="M10.5 10.2a2.5 2.5 0 100 3.6M17 10.2a2.5 2.5 0 100 3.6"/>',
  speaker: '<path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4z" fill="currentColor"/><path d="M15.5 9a4 4 0 010 6M18.5 6.5a8 8 0 010 11"/>',
  list: '<path d="M9 6h11M9 12h11M9 18h11"/><path d="M4.5 6h.01M4.5 12h.01M4.5 18h.01" stroke-width="2.6"/>',
  sort: '<path d="M7 4v16M3.5 7.5L7 4l3.5 3.5"/><path d="M17 20V4M13.5 16.5L17 20l3.5-3.5"/>',
  shuffle: '<path d="M3 7h3.5c4 0 6 10 10.5 10H21"/><path d="M3 17h3.5c1.6 0 2.8-1.6 3.8-3.5M13.7 10.5c1-1.9 2.2-3.5 3.8-3.5H21"/><path d="M18.5 4.5L21 7l-2.5 2.5M18.5 14.5L21 17l-2.5 2.5"/>',
  repeat: '<path d="M4 11V9a3 3 0 013-3h13"/><path d="M17 3l3 3-3 3"/><path d="M20 13v2a3 3 0 01-3 3H4"/><path d="M7 21l-3-3 3-3"/>',
  repeat1: '<path d="M4 11V9a3 3 0 013-3h13"/><path d="M17 3l3 3-3 3"/><path d="M20 13v2a3 3 0 01-3 3H4"/><path d="M7 21l-3-3 3-3"/><path d="M11.5 10.5L13 9.5v5" stroke-width="1.8"/>',
  album: '<rect x="3" y="3" width="13" height="13" rx="2"/><path d="M8 21h10a3 3 0 003-3V8"/>',
  mic: '<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5.5 11a6.5 6.5 0 0013 0M12 17.5V21"/>',
  note: '<path d="M10 18V5l9-2v3l-9 2"/><circle cx="7" cy="18" r="3" fill="currentColor"/>',
  hand: '<path d="M9 11V5.5a1.5 1.5 0 013 0V11"/><path d="M12 10V4.5a1.5 1.5 0 013 0V11"/><path d="M15 10.5V6.5a1.5 1.5 0 013 0v7.5a7 7 0 01-7 7h-.5a6 6 0 01-4.6-2.2L3.4 15a1.5 1.5 0 012.2-2L9 15.5V8a1.5 1.5 0 013 0"/>',
  lock: '<rect x="5" y="11" width="14" height="10" rx="2.5" fill="currentColor"/><path d="M8 11V8a4 4 0 018 0v3"/>',
  wifiOff: '<path d="M2 8.5a15 15 0 0120 0M5.5 12a10 10 0 0113 0M9 15.5a5 5 0 016 0"/><path d="M12 19h.01" stroke-width="3"/><path d="M3 3l18 18" stroke="var(--red)"/>',
  person: '<circle cx="12" cy="8" r="4"/><path d="M4 21a8 8 0 0116 0"/>',
  personPlus: '<circle cx="10" cy="8" r="4"/><path d="M2 21a8 8 0 0114.5-4.6"/><path d="M19 14v6M16 17h6"/>',
  logout: '<path d="M14 4h4a2 2 0 012 2v12a2 2 0 01-2 2h-4"/><path d="M10 8l-4 4 4 4M6 12h10"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v6"/><path d="M12 7.5h.01" stroke-width="2.6"/>',
  warn: '<path d="M12 3l10 18H2z" stroke-linejoin="round"/><path d="M12 10v5"/><path d="M12 18h.01" stroke-width="2.6"/>',
  stack: '<rect x="3" y="7" width="14" height="14" rx="2.5"/><path d="M7 3.5h12a2 2 0 012 2v12"/>',
  delete: '<path d="M9 5h11a1 1 0 011 1v12a1 1 0 01-1 1H9l-6-7z"/><path d="M12 9.5l5 5M17 9.5l-5 5"/>',
  chevron: '<path d="M9 5l7 7-7 7"/>',
  more: '<path d="M5 12h.01M12 12h.01M19 12h.01" stroke-width="3"/>',
};

/** An icon (`name` from the table above) as an inline SVG. */
export function icon(name, cls) {
  const span = document.createElement('span');
  span.className = 'ic' + (cls ? ' ' + cls : '');
  span.innerHTML = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${I[name] || ''}</svg>`;
  return span;
}

// ---- the brand ----------------------------------------------------------------

/** "AZOTO TV", like the TV's logo. */
export function wordmark(size = 34) {
  return h('div', { class: 'wordmark', style: { fontSize: size + 'px' } }, h('span', {}, 'AZOTO'), h('b', {}, 'TV'));
}

const initial = (name) => (name || '?').trim().charAt(0).toUpperCase();

/** A profile's round picture (Jellyfin's), or its initial. */
export function avatar(id, name, size = 64, hasImage = true) {
  const el = h('div', { class: 'avatar', style: { width: size + 'px', height: size + 'px', fontSize: size * 0.42 + 'px' } }, initial(name));
  if (hasImage && id) {
    const img = h('img', { src: `/api/app/avatar/${id}`, alt: '' });
    img.onload = () => el.replaceChildren(img);
    img.onerror = () => {};
  }
  return el;
}

// ---- small pieces ---------------------------------------------------------------

export function spinner(cls) {
  return h('div', { class: 'spinner' + (cls ? ' ' + cls : '') });
}

/** A thin progress line (0-1), or nothing. */
export function progressLine(value) {
  if (value == null) return null;
  return h('div', { class: 'pline' }, h('div', { style: { width: Math.max(0, Math.min(1, value)) * 100 + '%' } }));
}

/** "1:23:45" / "12:34" */
export function clock(seconds) {
  if (!isFinite(seconds) || seconds < 0) return '0:00';
  const s = Math.floor(seconds);
  const pad = (n) => String(n).padStart(2, '0');
  return s >= 3600 ? `${Math.floor(s / 3600)}:${pad(Math.floor(s / 60) % 60)}:${pad(s % 60)}` : `${Math.floor(s / 60)}:${pad(s % 60)}`;
}

/** A button: `btn('Label', fn, {icon, kind: 'primary'|'glass'|'plain', cls})`. */
export function btn(label, onclick, opts = {}) {
  const kind = opts.kind || 'glass';
  return h(
    'button',
    { class: `btn ${kind}${opts.cls ? ' ' + opts.cls : ''}`, type: 'button', disabled: opts.disabled, 'aria-label': opts.aria, onclick },
    opts.icon ? icon(opts.icon) : null,
    label ? h('span', {}, label) : null,
  );
}

/** A round icon button. */
export function iconBtn(name, onclick, aria, cls) {
  return h('button', { class: 'ibtn' + (cls ? ' ' + cls : ''), type: 'button', 'aria-label': aria, onclick }, icon(name));
}

/** Calls `fn` on a press (pointer down) with a click under the finger; `repeat` keeps
 * calling while held. */
export function onPress(el, fn, { repeat = false } = {}) {
  let timer;
  const stop = () => {
    clearTimeout(timer);
    el.classList.remove('down');
  };
  el.addEventListener('pointerdown', (e) => {
    e.preventDefault();
    el.classList.add('down');
    buzz();
    fn();
    if (repeat) {
      const again = (wait) => (timer = setTimeout(() => (fn(), again(160)), wait));
      again(450);
    }
  });
  for (const ev of ['pointerup', 'pointercancel', 'pointerleave']) el.addEventListener(ev, stop);
  el.addEventListener('contextmenu', (e) => e.preventDefault());
  return el;
}

/** Holding a finger on `el` (like iOS's context menus) calls `fn`; the tap that follows is ignored. */
export function onLong(el, fn) {
  let timer;
  let fired = false;
  let at = null;
  el.addEventListener('pointerdown', (e) => {
    fired = false;
    at = { x: e.clientX, y: e.clientY };
    timer = setTimeout(() => {
      fired = true;
      buzz(20);
      fn();
    }, 500);
  });
  el.addEventListener('pointermove', (e) => at && Math.hypot(e.clientX - at.x, e.clientY - at.y) > 10 && clearTimeout(timer));
  for (const ev of ['pointerup', 'pointercancel', 'pointerleave']) el.addEventListener(ev, () => clearTimeout(timer));
  el.addEventListener('click', (e) => fired && (e.stopImmediatePropagation(), e.preventDefault()), true);
  el.addEventListener('contextmenu', (e) => e.preventDefault());
  return el;
}

/** Videos and songs stay silent in tests (localStorage azoto.mute). */
export const testMute = (() => {
  try {
    return !!localStorage.getItem('azoto.mute');
  } catch {
    return false;
  }
})();

let hlsLoading;
/** hls.js (for browsers without HLS of their own), loaded the first time a stream needs it. */
export function loadHls() {
  if (window.Hls) return Promise.resolve(window.Hls);
  hlsLoading ||= new Promise((resolve, reject) => {
    const s = document.createElement('script');
    s.src = '/hls.min.js';
    s.onload = () => resolve(window.Hls);
    s.onerror = reject;
    document.head.append(s);
  });
  return hlsLoading;
}

/**
 * Plays `url` in `media` (an HLS playlist through hls.js where the browser has no
 * HLS of its own). Resolves a function that stops it.
 */
export async function attachStream(media, url, { onFatal } = {}) {
  media.muted = testMute;
  const hls = /\.m3u8(\?|$)/.test(url) || url.includes('/master.m3u8') || url.includes('/main.m3u8') || url.includes('m3u8');
  if (hls && !media.canPlayType('application/vnd.apple.mpegurl')) {
    const Hls = await loadHls();
    if (Hls.isSupported()) {
      const player = new Hls({ maxBufferLength: 30, backBufferLength: 30 });
      player.on(Hls.Events.ERROR, (_, d) => {
        if (!d.fatal) return;
        if (d.type === Hls.ErrorTypes.MEDIA_ERROR) player.recoverMediaError();
        else onFatal?.(d);
      });
      player.loadSource(url);
      player.attachMedia(media);
      return () => player.destroy();
    }
  }
  media.src = url;
  return () => {
    media.removeAttribute('src');
    media.load();
  };
}

/** Lazy images: an <img> that fades in, with a placeholder colour behind. */
export function img(src, cls = '', alt = '') {
  const box = h('div', { class: 'imgbox ' + cls });
  if (src) {
    const i = h('img', { src, alt, loading: 'lazy', decoding: 'async' });
    i.onload = () => box.classList.add('loaded');
    i.onerror = () => box.classList.add('broken');
    box.append(i);
  } else box.classList.add('broken');
  return box;
}

// ---- events -----------------------------------------------------------------------

const listeners = {};
/** Listens to `name` while `el` is on the page (gone once it's removed). */
export function on(name, fn, el) {
  (listeners[name] ||= []).push({ fn, el, seen: false });
}
export function emit(name, value) {
  const list = listeners[name];
  if (!list) return;
  listeners[name] = list.filter((l) => {
    if (l.el) {
      if (l.el.isConnected) l.seen = true;
      else if (l.seen) return false;
      else return true; // not on the page yet
    }
    try {
      l.fn(value);
    } catch (e) {
      console.error(e);
    }
    return true;
  });
}

// ---- toast ---------------------------------------------------------------------------

let toastTimer;
/** A short note at the bottom ("Sulla TV"). */
export function toast(text) {
  let el = document.querySelector('.toast');
  if (!el) document.body.append((el = h('div', { class: 'toast' })));
  el.textContent = text;
  el.classList.add('on');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => el.classList.remove('on'), 3000);
}

// ---- layers: pages, covers, sheets (each one is a step back) ----------------------------

// Every page pushed on a tab, full-screen cover and sheet is a layer with its own
// history entry, so the browser's / Android's back closes the top one.
const layers = [];

function addLayer(layer) {
  layers.push(layer);
  history.pushState({ azoto: layers.length }, '');
}

window.addEventListener('popstate', () => {
  // back one step, or several at once (history.go(-n))
  const keep = history.state?.azoto || 0;
  while (layers.length > keep) {
    try {
      layers.pop().close();
    } catch (e) {
      console.error(e);
    }
  }
});

/** Closes `layer` (the top one, or one under it: the ones above go too). */
function closeLayer(layer) {
  const i = layers.indexOf(layer);
  // gone, or already on its way (a second close before the history step lands)
  if (i < 0 || layer.closing) return;
  for (const l of layers.slice(i)) l.closing = true;
  const n = layers.length - i;
  history.go(-n);
}

/** Forgets every layer (the app is drawn again, e.g. for another profile). */
export function clearLayers() {
  layers.length = 0;
  history.replaceState({ azoto: 0 }, '');
}

/** The Android app's back key: true when the page went back by itself. */
window.azotoBack = () => {
  if (!layers.length) return false;
  history.back();
  return true;
};

// ---- tabs and their page stacks -----------------------------------------------------------

/**
 * A tab's navigation stack. `root(page)` sets its first page; `push(page)`
 * slides one in. A page: { title, large (big title), actions: () => [els],
 * body: el, onShow, onHide, dispose }.
 */
export class Stack {
  constructor(el) {
    this.el = el;
    this.pages = [];
  }

  root(page) {
    for (const p of this.pages) p.dispose?.();
    this.pages = [];
    this.el.replaceChildren();
    this._mount(page);
  }

  push(page) {
    const below = this.pages[this.pages.length - 1];
    if (below) {
      below.view.classList.add('under');
      below.onHide?.();
    }
    this._mount(page);
    page.view.classList.add('enter');
    requestAnimationFrame(() => requestAnimationFrame(() => page.view.classList.remove('enter')));
    const layer = { close: () => this._pop(page) };
    page.layer = layer;
    addLayer(layer);
  }

  /** Back one page (from the back button). */
  back() {
    const top = this.pages[this.pages.length - 1];
    if (top?.layer) closeLayer(top.layer);
  }

  /** Back to the first page. */
  home() {
    const second = this.pages[1];
    if (second?.layer) closeLayer(second.layer);
  }

  get top() {
    return this.pages[this.pages.length - 1];
  }

  _mount(page) {
    const prev = this.pages[this.pages.length - 1];
    const bar = h('header', { class: 'navbar' + (page.large ? ' large' : '') });
    const scroller = h('div', { class: 'scroll' });
    page.view = h('section', { class: 'page' }, bar, scroller);
    page.stack = this;
    page.refreshBar = () => {
      fill(
        bar,
        h(
          'div',
          { class: 'navrow' },
          prev
            ? h('button', { class: 'navback', type: 'button', onclick: () => this.back() }, icon('left'), h('span', {}, prev.short || prev.title || tr('Back')))
            : h('div', { class: 'navlead' }, page.lead ? page.lead() : null),
          h('div', { class: 'navtitle' }, page.large ? '' : page.title || ''),
          h('div', { class: 'navactions' }, page.actions ? page.actions() : null),
        ),
        page.large ? h('h1', { class: 'bigtitle' }, page.title) : null,
        page.under ? page.under() : null,
      );
    };
    page.refreshBar();
    scroller.append(page.body);
    // the big title shrinks into the bar once scrolled
    if (page.large) {
      scroller.addEventListener('scroll', () => bar.classList.toggle('scrolled', scroller.scrollTop > 30), { passive: true });
    }
    page.scroller = scroller;
    this.pages.push(page);
    this.el.append(page.view);
    page.onShow?.();
  }

  _pop(page) {
    const i = this.pages.indexOf(page);
    if (i < 0) return;
    for (const p of this.pages.splice(i)) {
      p.dispose?.();
      p.onHide?.();
      p.view.classList.add('leave');
      setTimeout(() => p.view.remove(), 260);
    }
    const top = this.top;
    if (top) {
      top.view.classList.remove('under');
      top.onShow?.();
    }
  }
}

/** Sets the page's title (and redraws its bar). */
export function retitle(page, title) {
  page.title = title;
  page.refreshBar?.();
}

// ---- full-screen covers (players, the controller, the photo viewer) -----------------------

/**
 * A full-screen layer over everything. `build(close)` returns its element;
 * `onClose` runs when it goes (back key or `close()`).
 */
export function cover(build, { onClose, cls = '' } = {}) {
  const el = h('div', { class: 'cover ' + cls });
  let open = true;
  const layer = {
    close: () => {
      if (!open) return;
      open = false;
      try {
        onClose?.();
      } catch (e) {
        console.error(e);
      }
      el.classList.add('leave');
      setTimeout(() => el.remove(), 220);
    },
  };
  const close = () => {
    if (open) closeLayer(layer);
  };
  el.append(build(close));
  document.body.append(el);
  addLayer(layer);
  return { el, close };
}

// ---- sheets and dialogs ------------------------------------------------------------------

/**
 * A sheet from the bottom. `build(close)` returns its content.
 * `full`: as tall as the screen (the PIN pad).
 */
export function sheet(build, { full = false, onClose, cls = '' } = {}) {
  const panel = h('div', { class: 'sheet' + (full ? ' full' : '') + (cls ? ' ' + cls : '') });
  const shade = h('div', { class: 'shade' }, panel);
  let open = true;
  const layer = {
    close: () => {
      if (!open) return;
      open = false;
      onClose?.();
      shade.classList.remove('on');
      setTimeout(() => shade.remove(), 260);
    },
  };
  const close = () => {
    if (open) closeLayer(layer);
  };
  shade.addEventListener('click', (e) => e.target === shade && close());
  panel.append(h('div', { class: 'grabber' }), build(close));
  document.body.append(shade);
  requestAnimationFrame(() => requestAnimationFrame(() => shade.classList.add('on')));
  addLayer(layer);
  return { panel, close };
}

/**
 * A list of choices (like iOS menus / action sheets): items are
 * { label, icon, danger, checked, run } or null for a divider. `title`/`message` above.
 */
export function menu(items, { title, message, onClose } = {}) {
  let chosen = null;
  sheet(
    (close) =>
    h(
      'div',
      { class: 'menu' },
      title ? h('div', { class: 'menutitle' }, title) : null,
      message ? h('div', { class: 'menumsg' }, message) : null,
      h(
        'div',
        { class: 'menulist' },
        items.map((it) =>
          it == null
            ? h('div', { class: 'menusep' })
            : h(
                'button',
                {
                  class: 'menuitem' + (it.danger ? ' danger' : '') + (it.checked ? ' checked' : ''),
                  type: 'button',
                  disabled: it.disabled,
                  onclick: () => {
                    chosen = it;
                    close();
                  },
                },
                h('span', { class: 'mcheck' }, it.checked ? icon('check') : null),
                h('span', { class: 'mlabel' }, it.label),
                it.icon ? icon(it.icon) : null,
              ),
        ),
      ),
      h('button', { class: 'menuitem cancel', type: 'button', onclick: () => close() }, h('span', { class: 'mlabel' }, tr('Cancel'))),
    ),
    {
      // the choice runs once the sheet (and its history step) is gone
      onClose: () =>
        setTimeout(() => {
          chosen?.run?.();
          onClose?.(chosen);
        }, 0),
    },
  );
}

/** "Are you sure?": resolves true when the action button is pressed. */
export function confirmBox(title, message, action, { danger = true } = {}) {
  return new Promise((done) => menu([{ label: action, danger }], { title, message, onClose: (chosen) => done(!!chosen) }));
}

/** Asks for a line of text (a new album's name). Resolves the text, or null. */
export function promptBox(title, placeholder, action) {
  return new Promise((done) => {
    let result = null;
    sheet(
      (close) => {
        const input = h('input', { class: 'field', placeholder, autocapitalize: 'sentences', enterkeyhint: 'done' });
        const go = () => {
          const v = input.value.trim();
          if (!v) return;
          result = v;
          close();
        };
        input.addEventListener('keydown', (e) => e.key === 'Enter' && go());
        setTimeout(() => input.focus(), 250);
        return h(
          'div',
          { class: 'prompt' },
          h('div', { class: 'menutitle' }, title),
          input,
          h('div', { class: 'row gap' }, btn(tr('Cancel'), () => close(), { cls: 'grow' }), btn(action, go, { kind: 'primary', cls: 'grow' })),
        );
      },
      { onClose: () => setTimeout(() => done(result), 0) },
    );
  });
}

// ---- the screen -------------------------------------------------------------------------------

/** Sideways (the controller, players) or upright again; the screen stays on while sideways. */
export async function landscape(on) {
  if (native && native.setLandscape) {
    try {
      native.setLandscape(on);
    } catch {}
    return;
  }
  try {
    if (on) {
      if (!document.fullscreenElement && document.documentElement.requestFullscreen) await document.documentElement.requestFullscreen({ navigationUI: 'hide' });
      await screen.orientation?.lock?.('landscape');
    } else {
      screen.orientation?.unlock?.();
      if (document.fullscreenElement) await document.exitFullscreen();
    }
  } catch {}
}

let wakeLock = null;
export async function keepAwake(on) {
  try {
    if (on && !wakeLock && navigator.wakeLock) wakeLock = await navigator.wakeLock.request('screen');
    if (!on && wakeLock) {
      await wakeLock.release();
      wakeLock = null;
    }
  } catch {}
}

/** Pull down at the top of `scroller` to call `fn` (like iOS's refresh). */
export function pullToRefresh(scroller, fn) {
  let start = null;
  let pulled = 0;
  const mark = h('div', { class: 'ptr' }, spinner());
  scroller.prepend(mark);
  scroller.addEventListener(
    'touchstart',
    (e) => {
      start = scroller.scrollTop <= 0 ? e.touches[0].clientY : null;
      pulled = 0;
    },
    { passive: true },
  );
  scroller.addEventListener(
    'touchmove',
    (e) => {
      if (start == null) return;
      pulled = Math.max(0, e.touches[0].clientY - start);
      mark.style.height = Math.min(pulled * 0.5, 70) + 'px';
      mark.classList.toggle('ready', pulled > 110);
    },
    { passive: true },
  );
  scroller.addEventListener('touchend', async () => {
    if (start == null) return;
    start = null;
    if (pulled > 110) {
      mark.classList.add('busy');
      mark.style.height = '48px';
      try {
        await fn();
      } catch {}
    }
    mark.classList.remove('busy', 'ready');
    mark.style.height = '0';
  });
}
