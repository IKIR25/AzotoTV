// "Invia" (a link to the TV, photos to the Galleria) and the Galleria: the TV's
// Immich albums, adding photos and videos, a full-screen viewer, "Sulla TV".

import { tr, h, fill, icon, btn, iconBtn, toast, spinner, img, sheet, cover, promptBox, native, can, on, pullToRefresh } from './ui.js';
import { api, token, sendLink, showOnTV, thumb } from './api.js';
import { model, push } from './model.js';

const isAndroid = /Android/i.test(navigator.userAgent);
const isIOS = /iPhone|iPad/.test(navigator.userAgent);

// ---- Invia --------------------------------------------------------------------------

export function sendPage() {
  const input = h('input', { class: 'field', type: 'url', placeholder: 'https://…', autocapitalize: 'off', autocorrect: 'off', spellcheck: 'false', enterkeyhint: 'send' });
  const send = btn(tr('Send to the TV'), () => go(), { kind: 'primary', icon: 'tv' });
  const busy = (on) => {
    send.disabled = on;
    send.classList.toggle('busy', on);
  };
  const go = async () => {
    const v = input.value.trim();
    if (!v) return;
    busy(true);
    await sendLink(v);
    busy(false);
  };
  input.addEventListener('keydown', (e) => e.key === 'Enter' && go());
  const paste = navigator.clipboard?.readText
    ? btn(tr('Paste'), async () => {
        try {
          input.value = (await navigator.clipboard.readText()) || input.value;
        } catch {}
      }, { icon: 'paste' })
    : null;

  const body = h(
    'div',
    { class: 'content' },
    h(
      'div',
      { class: 'card glass col' },
      h('div', { class: 'headline row gap8' }, icon('link'), tr('Send a link to the TV')),
      h('p', { class: 'footnote dim' }, tr('YouTube opens in your YouTube on the TV; other videos (Instagram, TikTok…) the server opens.')),
      input,
      h('div', { class: 'row' }, paste, h('div', { class: 'grow' }), send),
    ),
    h(
      'button',
      { class: 'card glass', type: 'button', onclick: () => push(photosPage()) },
      icon('gallery', 'cardicon'),
      h('div', { class: 'grow' }, h('div', { class: 'headline' }, tr('Photos and videos')), h('div', { class: 'caption dim' }, tr('Add them to a Gallery album and watch them on the TV'))),
      icon('chevron', 'dim'),
    ),
    shareHelp(),
  );
  // a link shared from another app (the Android app's share menu)
  on('send-link', (link) => (input.value = link), body);
  return { title: tr('Send'), large: true, body };
}

/** How to send from the other apps: the Android app's share menu, or getting the app. */
function shareHelp() {
  if (native) return h('p', { class: 'footnote dim pad8' }, icon('share'), ' ', tr('From the other apps: Share › AzotoTV'));
  if (isAndroid)
    return h(
      'div',
      { class: 'card glass col' },
      h('div', { class: 'headline' }, tr('The Android app')),
      h('p', { class: 'footnote dim' }, tr('With the app you can send links and photos from any app (Share › AzotoTV) and the volume keys set the TV’s volume.')),
      h('a', { class: 'btn primary', href: '/AzotoTV.apk', download: 'AzotoTV.apk' }, tr('Download the Android app')),
    );
  if (isIOS) return h('p', { class: 'footnote dim pad8' }, tr('There is also an iPhone app: ask whoever runs the TV.'));
  return null;
}

// ---- the Galleria: albums --------------------------------------------------------------------

export function photosPage() {
  const grid = h('div', { class: 'grid2' }, spinner());
  const body = h('div', { class: 'content' }, grid);
  const load = async () => {
    try {
      const albums = await api('/api/albums');
      fill(
        grid,
        h(
          'button',
          { class: 'albumcell', type: 'button', onclick: () => create() },
          h('div', { class: 'albumcover newalbum' }, icon('plus')),
          h('div', { class: 'subhead' }, tr('New album')),
          h('div', { class: 'caption' }, ' '),
        ),
        albums.map((a) => albumCell(a, () => push(albumPage(a)))),
      );
    } catch (e) {
      fill(grid, h('p', { class: 'dim span2 pad30' }, e.message));
    }
  };
  const create = async () => {
    const name = await promptBox(tr('New album'), tr('Album name'), tr('Create'));
    if (!name) return;
    try {
      const a = await api('/api/albums', { body: { name } });
      load();
      push(albumPage(a, true));
    } catch (e) {
      toast(e.message);
    }
  };
  load();
  const page = { title: tr('Gallery'), large: true, body, onShow: () => page.loaded && load() };
  setTimeout(() => {
    page.loaded = true;
    pullToRefresh(page.scroller, load);
  }, 0);
  return page;
}

function albumCell(a, open) {
  return h(
    'button',
    { class: 'albumcell', type: 'button', onclick: open },
    a.cover ? img(thumb(a.cover), 'albumcover') : h('div', { class: 'albumcover' }, icon('photos')),
    h('div', { class: 'subhead ellipsis' }, a.name),
    h('div', { class: 'caption dim ellipsis' }, a.owner ? tr('{n} items · by {owner}', { n: a.count, owner: a.owner }) : tr('{n} items', { n: a.count })),
  );
}

// ---- one album ----------------------------------------------------------------------------------

/** Picks photos and videos (the Android app hands over shared ones first). */
function picker(onFiles) {
  const input = h('input', { type: 'file', accept: 'image/*,video/*', multiple: true, hidden: true });
  input.addEventListener('change', () => {
    const files = [...input.files];
    input.value = '';
    if (files.length) onFiles(files);
  });
  document.body.append(input);
  return input;
}

export function albumPage(album, fresh = false) {
  const grid = h('div', { class: 'photogrid' });
  const status = h('div', { class: 'uploadslot' });
  const empty = h('div', { class: 'emptyslot' });
  const more = h('div', { class: 'moreslot' });
  const body = h('div', { class: 'content flush' }, status, empty, grid, more);
  let assets = [];
  let next = 1;
  let loading = false;
  let loaded = false;
  const uploader = new Uploader(status, (ids) => showOnTV({ ids }), () => reload());
  const input = picker((files) => uploader.run(files, album));

  const more_ = async (replace) => {
    if (next == null || loading) return;
    loading = true;
    fill(more, spinner());
    try {
      const p = await api(`/api/app/album/${album.id}`, { query: { page: next } });
      assets = replace ? p.assets : assets.concat(p.assets);
      next = p.next ?? null;
      loaded = true;
      draw(replace ? assets : p.assets, replace);
    } catch (e) {
      toast(e.message);
    }
    loading = false;
    fill(more);
  };
  const reload = () => {
    next = 1;
    more_(true);
  };
  const draw = (list, replace) => {
    const cells = list.map((a) =>
      h(
        'button',
        { class: 'photo', type: 'button', onclick: () => viewer(assets, a.id, (id) => showOnTV({ ids: [id] })) },
        img(thumb(a.id)),
        a.video ? h('span', { class: 'vlen' }, icon('play'), length(a.seconds)) : null,
      ),
    );
    if (replace) fill(grid, cells);
    else grid.append(...cells);
    fill(
      empty,
      loaded && !assets.length && !uploader.busy
        ? h(
            'div',
            { class: 'center gap16 pad40' },
            icon('gallery', 'huge dim'),
            h('div', { class: 'headline' }, tr('The album is empty')),
            btn(tr('Add photos and videos'), () => input.click(), { kind: 'primary', icon: 'plus', cls: 'big' }),
          )
        : null,
    );
    page.refreshBar?.();
  };
  // the next page once the end is near
  const io = new IntersectionObserver((e) => e[0].isIntersecting && more_(false), { rootMargin: '600px' });
  io.observe(more);

  const page = {
    title: album.name,
    body,
    actions: () => [
      assets.length ? iconBtn('tv', () => showOnTV({ album: album.id }), tr('Show the album on the TV'), 'navbtn') : null,
      iconBtn('plus', () => !uploader.busy && input.click(), tr('Add photos and videos'), 'navbtn'),
    ],
    dispose: () => {
      io.disconnect();
      input.remove();
    },
  };
  reload();
  // just made: straight to the picker (needs the tap that made it, still fresh)
  if (fresh) setTimeout(() => input.click(), 0);
  return page;
}

const length = (s) => {
  const n = Math.round(s || 0);
  return `${Math.floor(n / 60)}:${String(n % 60).padStart(2, '0')}`;
};

/** Sends photos and videos to an album, one at a time, and says how it went. */
class Uploader {
  busy = false;
  constructor(el, show, done) {
    this.el = el;
    this.show = show;
    this.done = done;
  }

  async run(files, album) {
    this.busy = true;
    const ids = [];
    let dup = 0;
    let failed = 0;
    let lastError = '';
    const bar = h('div', { class: 'pbar' }, h('div'));
    const label = h('div', { class: 'subhead' });
    fill(this.el, h('div', { class: 'card glass col' }, label, bar));
    for (let i = 0; i < files.length; i++) {
      label.textContent = tr('Sending {i} of {n}…', { i: i + 1, n: files.length });
      try {
        const r = await upload(album.id, files[i], (f) => (bar.firstChild.style.width = ((i + f) / files.length) * 100 + '%'));
        ids.push(r.id);
        if (r.duplicate) dup++;
      } catch (e) {
        failed++;
        lastError = e.message;
      }
    }
    this.busy = false;
    const added = ids.length - dup;
    const parts = [];
    if (added > 0) parts.push(added === 1 ? tr('1 item added') : tr('{n} items added', { n: added }));
    if (dup > 0) parts.push(dup === 1 ? tr('1 was already there') : tr('{n} were already there', { n: dup }));
    if (failed > 0) parts.push(tr('{n} not sent', { n: failed }) + (lastError ? `: ${lastError}` : ''));
    fill(
      this.el,
      h(
        'div',
        { class: 'card glass col' },
        h('div', { class: 'subhead row gap8' }, icon(failed ? 'warn' : 'checkCircle', failed ? '' : 'green'), parts.join(' · ')),
        ids.length ? btn(tr('Show on the TV'), () => this.show(ids), { kind: 'primary', icon: 'tv', cls: 'wide' }) : null,
      ),
    );
    if (ids.length) api('/api/added', { body: { album: album.name, n: added } }).catch(() => {});
    this.done?.(ids);
    return ids;
  }
}

function upload(album, file, onProgress) {
  return new Promise((resolve, reject) => {
    const q = new URLSearchParams({ album, name: file.name || 'foto.jpg', modified: String(file.lastModified || Date.now()) });
    const x = new XMLHttpRequest();
    x.open('POST', `/api/upload?${q}`);
    x.setRequestHeader('x-azoto-key', token());
    x.setRequestHeader('content-type', file.type || 'application/octet-stream');
    x.upload.onprogress = (e) => e.lengthComputable && onProgress(e.loaded / e.total);
    x.onload = () => {
      let j = {};
      try {
        j = JSON.parse(x.responseText);
      } catch {}
      if (x.status >= 200 && x.status < 300) resolve(j);
      else reject(new Error(j.error || tr('Error {code}', { code: x.status })));
    };
    x.onerror = () => reject(new Error(tr("The TV isn't answering")));
    x.send(file);
  });
}

// ---- full screen: swipe through the album, videos play here ------------------------------------------

function viewer(assets, startId, onTV) {
  let chrome = true;
  let current = startId;
  cover(
    (close) => {
      const track = h('div', { class: 'vtrack' });
      const caption = h('div', { class: 'vcaption' });
      const top = h('div', { class: 'vtop' }, iconBtn('close', () => close(), tr('Close'), 'glassbtn'), h('div', { class: 'grow' }), btn(tr('On the TV'), () => onTV(current), { icon: 'tv' }));
      const el = h('div', { class: 'viewer' }, track, top, caption);
      const slides = assets.map((a) => {
        const s = h('div', { class: 'slide', 'data-id': a.id });
        s._asset = a;
        return s;
      });
      track.append(...slides);
      // only the pictures near the one on screen are loaded
      const fillSlide = (s) => {
        if (s._filled) return;
        s._filled = true;
        const a = s._asset;
        if (a.video) {
          const v = h('video', { src: `/api/app/original/${a.id}?k=${token()}`, controls: true, playsinline: true, preload: 'metadata', poster: thumb(a.id, true) });
          s.append(v);
        } else {
          const i = h('img', { src: thumb(a.id, true), alt: '', draggable: 'false' });
          s.append(spinner('abs'), i);
          i.onload = () => s.querySelector('.spinner')?.remove();
          zoomable(s, i, (on) => track.classList.toggle('locked', on));
        }
      };
      const onSlide = () => {
        const i = Math.round(track.scrollLeft / track.clientWidth);
        const s = slides[i];
        if (!s) return;
        for (const j of [i - 1, i, i + 1]) slides[j] && fillSlide(slides[j]);
        if (s.dataset.id !== current) {
          for (const v of track.querySelectorAll('video')) if (!s.contains(v)) v.pause();
          current = s.dataset.id;
        }
        const a = s._asset;
        const when = taken(a.taken);
        const sub = [when, a.place].filter(Boolean).join(' · ');
        fill(caption, a.caption || sub ? h('div', { class: 'vcap' }, a.caption ? h('div', { class: 'subhead' }, a.caption) : null, sub ? h('div', { class: 'caption dim' }, sub) : null) : null);
        caption.classList.toggle('lift', !!a.video);
      };
      track.addEventListener('scroll', () => requestAnimationFrame(onSlide), { passive: true });
      track.addEventListener('click', (e) => {
        if (e.target.closest('video')) return;
        chrome = !chrome;
        el.classList.toggle('bare', !chrome);
      });
      requestAnimationFrame(() => {
        const i = Math.max(0, assets.findIndex((a) => a.id === startId));
        track.scrollLeft = i * track.clientWidth;
        onSlide();
      });
      return el;
    },
    { cls: 'black', onClose: () => document.querySelectorAll('.viewer video').forEach((v) => v.pause()) },
  );
}

function taken(s) {
  if (!s) return '';
  const d = new Date(s);
  if (isNaN(d)) return '';
  return d.toLocaleDateString(document.documentElement.lang || 'it', { day: 'numeric', month: 'long', year: 'numeric' });
}

/** Pinch and double-tap zoom on a photo; `lock` stops the swiping while zoomed. */
function zoomable(box, image, lock) {
  let scale = 1;
  let x = 0;
  let y = 0;
  const pts = new Map();
  let start = null;
  let lastTap = 0;
  const apply = () => {
    image.style.transform = `translate(${x}px, ${y}px) scale(${scale})`;
    lock(scale > 1.01);
  };
  box.classList.add('zoomable');
  box.addEventListener('pointerdown', (e) => {
    pts.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pts.size === 2) {
      const [a, b] = [...pts.values()];
      start = { d: Math.hypot(a.x - b.x, a.y - b.y), scale };
    } else if (pts.size === 1 && scale > 1) {
      start = { px: e.clientX, py: e.clientY, x, y };
    }
    const now = Date.now();
    if (pts.size === 1 && now - lastTap < 280) {
      scale = scale > 1 ? 1 : 2.5;
      x = y = 0;
      apply();
      lastTap = 0;
    } else lastTap = now;
  });
  box.addEventListener('pointermove', (e) => {
    if (!pts.has(e.pointerId)) return;
    pts.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pts.size === 2 && start?.d) {
      const [a, b] = [...pts.values()];
      scale = Math.max(1, Math.min(4, (start.scale * Math.hypot(a.x - b.x, a.y - b.y)) / start.d));
      if (scale === 1) x = y = 0;
      apply();
    } else if (pts.size === 1 && scale > 1 && start?.px != null) {
      x = start.x + (e.clientX - start.px);
      y = start.y + (e.clientY - start.py);
      apply();
    }
  });
  const up = (e) => {
    pts.delete(e.pointerId);
    start = null;
  };
  box.addEventListener('pointerup', up);
  box.addEventListener('pointercancel', up);
}

// ---- shared from another app (the Android app's share menu) ------------------------------------------

/** Photos and videos shared to the app: pick an album (or make one), they go there, then "Sulla TV". */
export function sharedPhotos() {
  const n = can('sharedCount') ? native.sharedCount() : 0;
  sheet((close) => {
    const list = h('div', { class: 'menulist' }, spinner());
    const status = h('div', { class: 'uploadslot' });
    const what = h('div', { class: 'subhead row gap8' }, icon('photos'), n === 1 ? tr('1 item to add') : n > 1 ? tr('{n} items to add', { n }) : tr('Photos and videos to add'));
    const uploader = new Uploader(status, (ids) => showOnTV({ ids }));
    let target = null;
    const input = picker(async (files) => {
      fill(list);
      fill(what, icon('photos'), h('span', {}, tr('In “{name}”', { name: target.name })));
      await uploader.run(files, target);
    });
    const choose = (a) => {
      target = a;
      // the Android app answers the picker with the shared files at once
      input.click();
    };
    const name = h('input', { class: 'field', placeholder: tr('Album name'), autocapitalize: 'sentences' });
    const make = async () => {
      if (!name.value.trim()) return;
      try {
        choose(await api('/api/albums', { body: { name: name.value.trim() } }));
      } catch (e) {
        toast(e.message);
      }
    };
    api('/api/albums')
      .then((albums) =>
        fill(
          list,
          h('div', { class: 'menutitle left' }, tr('New album')),
          h('div', { class: 'row gap8 padh' }, name, btn(tr('Create'), make, { kind: 'primary' })),
          h('div', { class: 'menutitle left' }, tr('Or in an album')),
          albums.map((a) =>
            h(
              'button',
              { class: 'menuitem albumrow', type: 'button', onclick: () => choose(a) },
              a.cover ? img(thumb(a.cover), 'mini') : h('div', { class: 'imgbox mini broken' }),
              h('div', { class: 'grow' }, h('div', {}, a.name), h('div', { class: 'caption dim' }, tr('{n} items', { n: a.count }))),
            ),
          ),
        ),
      )
      .catch((e) => fill(list, h('p', { class: 'dim pad16' }, e.message)));
    return h('div', { class: 'menu' }, h('div', { class: 'menutitle' }, tr('Add to the Gallery')), h('div', { class: 'pad16' }, what), status, list, btn(tr('Done'), () => close(), { cls: 'wide' }));
  });
}
