// AzotoTV on the phone: a remote, typing into the TV's keyboard, "Manda alla
// TV" and adding photos to the Galleria. Served by AzotoTV itself
// (src-tauri/src/phone.rs); the same page runs in Safari / Chrome, as an
// iPhone home-screen app and inside the Android app (which adds
// `window.AzotoApp`: vibration, shared links and photos, volume keys).
'use strict';

const app = document.getElementById('app');
const native = window.AzotoApp || null;

// ---- the phone's key ------------------------------------------------------

// what the address asks for: #c=<code> (the TV's QR code), #send=<link>, #share
// (Android); read before `keep` rewrites the address
const hash = new URLSearchParams(location.hash.slice(1));
const wants = { code: hash.get('c'), send: hash.get('send'), share: hash.has('share') };

const TOKEN_KEY = 'azoto.token';
let token = '';
try {
  token = localStorage.getItem(TOKEN_KEY) || '';
} catch {}
// the home-screen app opens with its key in the address (see manifest.webmanifest)
const urlKey = new URLSearchParams(location.search).get('k');
if (urlKey) token = urlKey;

function keep(t) {
  token = t;
  try {
    t ? localStorage.setItem(TOKEN_KEY, t) : localStorage.removeItem(TOKEN_KEY);
  } catch {}
  // "Add to Home Screen" takes this address and this manifest
  history.replaceState(null, '', t ? `/?k=${t}` : '/');
  document.getElementById('manifest').href = t ? `/manifest.webmanifest?k=${t}` : '/manifest.webmanifest';
}
if (token) keep(token);
else if (location.hash) history.replaceState(null, '', location.pathname + location.search);

/**
 * What the phone gives the page (sizes, safe areas, home-screen mode), into
 * the TV's log once per start: for layout problems that only show on a real
 * phone (iOS has changed how home-screen apps are laid out more than once).
 */
function reportScreen() {
  const probe = document.createElement('div');
  probe.style.cssText =
    'position:fixed;visibility:hidden;padding:env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left)';
  document.body.append(probe);
  const cs = getComputedStyle(probe);
  const inset = [cs.paddingTop, cs.paddingRight, cs.paddingBottom, cs.paddingLeft].map(parseFloat);
  probe.remove();
  const vv = window.visualViewport;
  const info = {
    standalone: !!navigator.standalone || matchMedia('(display-mode: standalone)').matches,
    inner: [innerWidth, innerHeight],
    client: [document.documentElement.clientWidth, document.documentElement.clientHeight],
    screen: [screen.width, screen.height],
    visual: vv ? [Math.round(vv.width), Math.round(vv.height), Math.round(vv.offsetTop)] : null,
    inset,
    body: Math.round(document.body.getBoundingClientRect().height),
    dpr: devicePixelRatio,
    ua: navigator.userAgent,
  };
  fetch('/api/diag', { method: 'POST', headers: { 'x-azoto-key': token, 'content-type': 'application/json' }, body: JSON.stringify(info) }).catch(() => {});
}

// an app, not a page: no pinch zoom (Safari ignores user-scalable=no)
for (const ev of ['gesturestart', 'gesturechange']) document.addEventListener(ev, (e) => e.preventDefault(), { passive: false });
document.addEventListener('touchmove', (e) => e.touches.length > 1 && e.preventDefault(), { passive: false });


// ---- helpers --------------------------------------------------------------

function h(tag, props, ...kids) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(props || {})) {
    if (v == null || v === false) continue;
    if (k.startsWith('on')) el.addEventListener(k.slice(2), v);
    else if (k === 'class') el.className = v;
    else if (k === 'html') el.innerHTML = v;
    else el.setAttribute(k, v === true ? '' : v);
  }
  for (const kid of kids.flat()) if (kid != null && kid !== false) el.append(kid);
  return el;
}

const ICONS = {
  up: '<path d="M6 15l6-6 6 6"/>',
  down: '<path d="M6 9l6 6 6-6"/>',
  left: '<path d="M15 6l-6 6 6 6"/>',
  right: '<path d="M9 6l6 6-6 6"/>',
  back: '<path d="M9 14l-5-5 5-5"/><path d="M4 9h10.5a5.5 5.5 0 010 11H11"/>',
  home: '<path d="M3 10.5L12 3l9 7.5"/><path d="M5 9v11h14V9"/>',
  menu: '<path d="M4 7h16M4 12h16M4 17h16"/>',
  power: '<path d="M12 3v9"/><path d="M6.3 6.8a8 8 0 1011.4 0"/>',
  play: '<path d="M7 5v14l6-7z"/><path d="M15 5v14M19 5v14"/>',
  rew: '<path d="M11 6l-7 6 7 6z"/><path d="M20 6l-7 6 7 6z"/>',
  ff: '<path d="M4 6l7 6-7 6z"/><path d="M13 6l7 6-7 6z"/>',
  minus: '<path d="M5 12h14"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  remote: '<rect x="7" y="2.5" width="10" height="19" rx="5"/><circle cx="12" cy="8.5" r="2.2"/><path d="M10 14h.01M14 14h.01M10 17.5h.01M14 17.5h.01"/>',
  photo: '<rect x="3" y="4" width="18" height="16" rx="3"/><circle cx="9" cy="10" r="2"/><path d="M21 16l-5-5-9 9"/>',
  send: '<path d="M4 12l16-8-6 16-3-7z"/><path d="M11 13l9-9"/>',
  album: '<rect x="3" y="4" width="18" height="16" rx="3"/><path d="M12 9v6M9 12h6"/>',
  keyboard: '<rect x="2.5" y="6" width="19" height="12" rx="2.5"/><path d="M6 10h.01M10 10h.01M14 10h.01M18 10h.01M7 14h10"/>',
  pad: '<path d="M7.5 8h9a5 5 0 0 1 4.8 6.4l-1.1 3.6a2 2 0 0 1-3.5.6L14.8 16H9.2l-1.9 2.6a2 2 0 0 1-3.5-.6l-1.1-3.6A5 5 0 0 1 7.5 8z"/><path d="M8 11v3M6.5 12.5h3M15.5 12h.01M17.5 13.5h.01"/>',
  close: '<path d="M6 6l12 12M18 6L6 18"/>',
};
const icon = (name) =>
  h('span', {
    html: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${ICONS[name]}</svg>`,
    style: 'display:contents',
  });

function buzz(ms = 8) {
  try {
    if (native && native.vibrate) native.vibrate(ms);
    else if (navigator.vibrate) navigator.vibrate(ms);
  } catch {}
}

class Unpaired extends Error {}

async function api(path, { method = 'GET', body } = {}) {
  const r = await fetch(path, {
    method,
    headers: { 'x-azoto-key': token, ...(body ? { 'content-type': 'application/json' } : {}) },
    body: body ? JSON.stringify(body) : undefined,
  }).catch(() => {
    throw new Error('La TV non risponde: sei sulla Wi-Fi di casa?');
  });
  const data = await r.json().catch(() => ({}));
  if (r.status === 401) {
    keep('');
    throw new Unpaired(data.error || 'Telefono non collegato');
  }
  if (!r.ok) throw new Error(data.error || `Errore ${r.status}`);
  return data;
}

let toastTimer;
/** A short note at the bottom of the screen. */
function toast(text) {
  let el = document.querySelector('.ptoast');
  if (!el) document.body.append((el = h('div', { class: 'ptoast' })));
  el.textContent = text;
  el.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (el.hidden = true), 3500);
}

function show(...nodes) {
  app.replaceChildren(...nodes);
  window.scrollTo(0, 0);
}

// an Android phone in the browser: the app does more (share menu, volume keys)
const androidBrowser = !native && /Android/i.test(navigator.userAgent);
const getApp = () =>
  androidBrowser
    ? h('a', { class: 'btn wide', href: '/AzotoTV.apk', download: 'AzotoTV.apk' }, 'Scarica l’app Android')
    : null;

function logo() {
  return h('div', { class: 'logo' }, h('span', {}, 'AZOTO'), h('b', {}, 'TV'));
}

const initial = (name) => (name || '?').trim().charAt(0).toUpperCase();

// ---- pairing -----------------------------------------------------------------

/** What this phone is, for the TV's list ("iPhone", the Android phone's own name). */
function deviceLabel() {
  try {
    if (native && native.deviceName) return native.deviceName();
  } catch {}
  const ua = navigator.userAgent;
  if (/iPhone/.test(ua)) return 'iPhone';
  if (/iPad/.test(ua)) return 'iPad';
  const m = ua.match(/Android [\d.]+; ([^;)]+)/);
  if (m && m[1].trim() !== 'K') return m[1].trim();
  return /Android/.test(ua) ? 'Telefono Android' : 'Telefono';
}

/**
 * The 4-digit code shown by the TV (START › Collega un telefono, in a
 * profile): pairs the phone with that profile. `adding`: the phone is already
 * paired and gets one more profile.
 */
function pairScreen(prefill, error, adding = false) {
  const err = h('p', { class: 'err' }, error || '');
  const input = h('input', {
    class: 'field code',
    inputmode: 'numeric',
    autocomplete: 'one-time-code',
    maxlength: '4',
    placeholder: '••••',
    value: prefill || '',
  });
  const go = h('button', { class: 'btn primary wide', type: 'submit' }, adding ? 'Aggiungi' : 'Collega');
  const form = h(
    'form',
    {
      class: 'center',
      onsubmit: async (e) => {
        e.preventDefault();
        const code = input.value.trim();
        if (!/^\d{4}$/.test(code)) return (err.textContent = 'Il codice ha 4 cifre');
        go.disabled = true;
        err.textContent = '';
        try {
          const r = await api('/api/pair', { method: 'POST', body: { code, device: deviceLabel() } });
          keep(r.token);
          me = { label: r.label, users: r.users };
          buzz(20);
          main();
        } catch (e) {
          err.textContent = e.message;
          go.disabled = false;
        }
      },
    },
    adding ? h('button', { class: 'back', type: 'button', onclick: () => accountsScreen() }, icon('left'), 'Indietro') : logo(),
    h('h1', {}, adding ? 'Aggiungi un profilo' : 'Collega il telefono'),
    h(
      'p',
      { class: 'muted' },
      adding
        ? 'Sulla TV, dal profilo da aggiungere: START (o il suo avatar) › «Collega un telefono». Scrivi qui il codice che vedi.'
        : 'Sulla TV, dal tuo profilo: START (o il tuo avatar) › «Collega un telefono». Scrivi qui il codice che vedi.',
    ),
    input,
    err,
    go,
    adding ? null : getApp(),
  );
  show(form);
  if (prefill && /^\d{4}$/.test(prefill)) form.requestSubmit();
  else input.focus();
}

/** The profiles this phone is paired with (the TV obeys it while one of them is on). */
function accountsScreen() {
  show(
    h(
      'div',
      { class: 'center' },
      h('button', { class: 'back', onclick: () => main() }, icon('left'), 'Indietro'),
      h('h1', {}, 'Profili collegati'),
      h('p', { class: 'muted' }, 'Il telefono comanda la TV quando c’è uno di questi profili, e dalla scelta del profilo.'),
      h('div', { class: 'accounts' }, ...me.users.map((u) => h('div', { class: 'account' }, h('span', { class: 'avatar' }, initial(u.name)), u.name))),
      h('button', { class: 'btn primary wide', onclick: () => pairScreen('', '', true) }, icon('plus'), 'Aggiungi un profilo'),
      h('p', { class: 'muted small' }, 'Per scollegare un profilo: sulla TV, da quel profilo, «Collega un telefono».'),
    ),
  );
}

// ---- the connection to the TV ----------------------------------------------

let me = null;
const tv = { text: null, volume: null, dark: false };
let ws = null;
let wsRetry = null;
let connected = false;
const onTv = new Set(); // redraws that follow the TV's state
let onPlayer = null; // the controller screen waiting for its player number

function connect() {
  if (!token || (ws && ws.readyState <= 1)) return;
  clearTimeout(wsRetry);
  ws = new WebSocket(`${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws?k=${token}`);
  ws.onopen = () => setConnected(true);
  ws.onmessage = (e) => {
    let m;
    try {
      m = JSON.parse(e.data);
    } catch {
      return;
    }
    if (m.t === 'player') return onPlayer && onPlayer(m);
    // a game rumbles this phone's controller (0 = stop)
    if (m.t === 'rumble') return buzz(m.ms);
    Object.assign(tv, m);
    onTv.forEach((f) => f());
  };
  ws.onclose = () => {
    ws = null;
    setConnected(false);
    if (document.visibilityState === 'visible') wsRetry = setTimeout(connect, 1500);
  };
}

function setConnected(on) {
  connected = on;
  onTv.forEach((f) => f());
}

document.addEventListener('visibilitychange', () => {
  if (document.visibilityState === 'visible' && me) connect();
});

function sendTv(msg) {
  if (ws && ws.readyState === 1) ws.send(JSON.stringify(msg));
}

const key = (a) => sendTv({ t: 'key', a });

// the Android app's volume keys (only while the remote is on screen)
window.azotoVolume = (d) => sendTv({ t: 'vol', d });

// ---- main screen ------------------------------------------------------------

let tab = 'remote';
let page = null;

function main() {
  onTv.clear();
  const dot = h('span', { class: 'dot' });
  const top = h(
    'header',
    { class: 'top' },
    logo(),
    dot,
    h(
      'button',
      { class: 'who', onclick: () => accountsScreen() },
      h('span', { class: 'avatar' }, initial(me.users[0] && me.users[0].name)),
      h('span', { class: 'name' }, me.users.map((u) => u.name).join(' · ')),
    ),
  );
  const banner = h('div', { class: 'banner', hidden: true }, 'Non collegato alla TV, riprovo…');
  const other = h('div', { class: 'banner info', hidden: true });
  page = h('main', { class: 'page' });
  const tabs = h('nav', { class: 'tabs' });
  const pill = h('button', { class: 'typepill', hidden: true, onclick: () => go('remote') }, icon('keyboard'), ' Scrivi sulla TV');
  const TABS = [
    ['remote', 'Telecomando', 'remote'],
    ['photos', 'Foto', 'photo'],
    ['send', 'Manda alla TV', 'send'],
  ];
  function go(t) {
    tab = t;
    for (const b of tabs.children) b.classList.toggle('on', b.dataset.tab === t);
    native && native.setVolumeKeys && native.setVolumeKeys(t === 'remote');
    onTv.clear();
    onTv.add(common);
    page.replaceChildren(t === 'remote' ? remote() : t === 'photos' ? photos() : sendPage());
    common();
    window.scrollTo(0, 0);
  }
  for (const [t, label, ic] of TABS) tabs.append(h('button', { class: 'tab', 'data-tab': t, onclick: () => go(t) }, icon(ic), label));
  function common() {
    dot.classList.toggle('on', connected);
    banner.hidden = connected;
    pill.hidden = !(tv.text && tab !== 'remote');
    const elsewhere = connected && !tv.picker && tv.user && !me.users.some((u) => u.id === tv.user);
    other.hidden = !elsewhere;
    if (elsewhere) other.textContent = `Sulla TV c’è ${tv.user_name ? `il profilo di ${tv.user_name}` : 'un altro profilo'}: se premi qualcosa, la TV chiede di passare al tuo.`;
  }
  show(top, banner, other, page, tabs, pill);
  connect();
  if (wants.send) {
    const link = wants.send;
    wants.send = null;
    go('send');
    // shared from another app: back to it once the TV has it
    sendLink(link).then((ok) => ok && native && native.sharedDone && setTimeout(() => native.sharedDone(), 1200));
  } else if (wants.share) {
    wants.share = false;
    go('photos');
  } else go(tab);
}

// ---- remote -------------------------------------------------------------------

/** A button that sends `action` on press; arrows repeat while held, OK held = hold. */
function press(el, action, { repeat = false, hold = false } = {}) {
  let timer = null;
  let held = false;
  const down = (e) => {
    e.preventDefault();
    el.setPointerCapture && el.setPointerCapture(e.pointerId);
    el.classList.add('down');
    buzz();
    held = false;
    if (hold) {
      timer = setTimeout(() => {
        held = true;
        el.classList.add('held');
        buzz(35);
        action === 'a' ? key('a-hold') : action();
      }, 550);
      return;
    }
    typeof action === 'function' ? action() : key(action);
    if (repeat) {
      const tick = () => {
        typeof action === 'function' ? action() : key(action);
        timer = setTimeout(tick, 110);
      };
      timer = setTimeout(tick, 350);
    }
  };
  const up = (e) => {
    if (!el.classList.contains('down')) return;
    el.classList.remove('down', 'held');
    clearTimeout(timer);
    if (hold && !held && e.type === 'pointerup') key(action);
  };
  el.addEventListener('pointerdown', down);
  el.addEventListener('pointerup', up);
  el.addEventListener('pointercancel', up);
  el.addEventListener('contextmenu', (e) => e.preventDefault());
  return el;
}

function remote() {
  const power = h('button', { class: 'round power', 'aria-label': 'Accendi / spegni' }, icon('power'));
  const powerLabel = h('span', {}, 'Spegni');
  press(power, () => sendTv({ t: 'power' }));

  const pad = h(
    'div',
    { class: 'dpad' },
    press(h('button', { class: 'k up', 'aria-label': 'Su' }, icon('up')), 'up', { repeat: true }),
    press(h('button', { class: 'k left', 'aria-label': 'Sinistra' }, icon('left')), 'left', { repeat: true }),
    press(h('button', { class: 'ok' }, 'OK'), 'a', { hold: true }),
    press(h('button', { class: 'k right', 'aria-label': 'Destra' }, icon('right')), 'right', { repeat: true }),
    press(h('button', { class: 'k downk', 'aria-label': 'Giù' }, icon('down')), 'down', { repeat: true }),
  );

  const labeled = (btn, label) => h('div', { class: 'label-under' }, btn, label);
  const vol = h('div', { class: 'v' });
  const volume = h(
    'div',
    { class: 'vol' },
    press(h('button', { class: 'round', 'aria-label': 'Volume giù' }, icon('minus')), () => sendTv({ t: 'vol', d: -1 }), { repeat: true }),
    vol,
    press(h('button', { class: 'round', 'aria-label': 'Volume su' }, icon('plus')), () => sendTv({ t: 'vol', d: 1 }), { repeat: true }),
  );

  // the TV's keyboard is open: type here instead
  const field = h('input', {
    class: 'field',
    type: 'text',
    enterkeyhint: 'done',
    autocomplete: 'off',
    autocapitalize: 'off',
    spellcheck: 'false',
  });
  let typedAt = 0;
  field.addEventListener('input', () => {
    typedAt = Date.now();
    sendTv({ t: 'text', v: field.value });
  });
  const done = () => {
    sendTv({ t: 'text', v: field.value, enter: true });
    field.blur();
    buzz(15);
  };
  field.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      done();
    }
  });
  const lbl = h('span', {}, 'Scrivi sulla TV');
  const textcard = h(
    'div',
    { class: 'textcard', hidden: true },
    h('div', { class: 'lbl' }, lbl, icon('keyboard')),
    h('div', { class: 'line' }, field, h('button', { class: 'btn primary', onclick: done }, 'Fatto')),
  );

  function update() {
    vol.replaceChildren(tv.volume == null ? '—' : String(tv.volume), h('small', {}, 'volume TV'));
    power.classList.toggle('off', !!tv.dark);
    powerLabel.textContent = tv.dark ? 'Accendi' : 'Spegni';
    const t = tv.text;
    const was = !textcard.hidden;
    textcard.hidden = !t;
    if (!t) {
      if (was && document.activeElement === field) field.blur();
      return;
    }
    lbl.textContent = t.label ? `Scrivi sulla TV · ${t.label}` : 'Scrivi sulla TV';
    const type = t.secret ? 'password' : 'text';
    if (field.type !== type) field.type = type;
    // a password never comes back from the TV; and don't fight the user's typing with the echo
    if (!t.secret && (document.activeElement !== field || Date.now() - typedAt > 1500)) {
      if (field.value !== t.value) field.value = t.value;
    }
  }
  // this phone as a controller, for the menus and the games; louder while
  // the TV's Controller screen is open or a game for more players is on
  const gameName = h('b');
  const gameSub = h('small');
  const gameCard = h(
    'button',
    { class: 'gamecard', onclick: () => controller() },
    h('span', { class: 'gi' }, icon('pad')),
    h('span', { class: 'gt' }, gameName, gameSub),
  );
  const updateGame = () => {
    const g = tv.game;
    const multi = g && g.players > 1;
    gameCard.classList.toggle('quiet', !(tv.controllers || multi));
    if (tv.controllers) {
      gameName.textContent = 'La TV cerca controller';
      gameSub.textContent = 'Tocca qui per entrare col telefono';
    } else if (multi) {
      gameName.textContent = `${g.name} · fino a ${g.players} giocatori`;
      gameSub.textContent = 'Tocca qui per giocare col telefono';
    } else {
      gameName.textContent = 'Usa come controller';
      gameSub.textContent = 'Il telefono diventa un controller per la TV e i giochi';
    }
  };
  onTv.add(updateGame);
  updateGame();
  onTv.add(update);
  update();

  return h(
    'section',
    { class: 'remote' },
    gameCard,
    textcard,
    h(
      'div',
      { class: 'remote-top' },
      labeled(power, powerLabel),
      labeled(press(h('button', { class: 'round', 'aria-label': 'Menu' }, icon('menu')), 'start'), 'Menu'),
    ),
    pad,
    h(
      'div',
      { class: 'row' },
      labeled(press(h('button', { class: 'round' }, icon('back')), 'b'), 'Indietro'),
      labeled(press(h('button', { class: 'round' }, icon('home')), 'home'), 'Home'),
      labeled(press(h('button', { class: 'round' }, icon('play')), 'play'), 'Play/Pausa'),
    ),
    h(
      'div',
      { class: 'row' },
      labeled(press(h('button', { class: 'round' }, icon('rew')), 'rew', { repeat: true }), 'Riavvolgi'),
      volume,
      labeled(press(h('button', { class: 'round' }, icon('ff')), 'ff', { repeat: true }), 'Avanti'),
    ),
  );
}

// ---- the phone as a game controller ---------------------------------------------

/** Keep getting a finger's moves even when it slides off the element. */
function capture(el, e) {
  try {
    el.setPointerCapture(e.pointerId);
  } catch {}
}

// buttons as bits, in azotopad's order (src-tauri: phone.rs `pad_state`)
const BTN = { a: 0, b: 1, x: 2, y: 3, back: 4, start: 6, lb: 9, rb: 10, up: 11, down: 12, left: 13, right: 14 };

/** Where a finger is, from an element's centre, in the element's own axes (the pad may be turned, see app.css). */
function fromCentre(el, e) {
  const r = el.getBoundingClientRect();
  const sx = e.clientX - (r.left + r.width / 2);
  const sy = e.clientY - (r.top + r.height / 2);
  return turned() ? [sy, -sx] : [sx, sy];
}
const turned = () => matchMedia('(orientation: portrait)').matches;

/** A stick: drag from anywhere in its circle; `onMove(x, y)` in -1..1 (y down). */
function stickEl(onMove) {
  const knob = h('i', { class: 'knob' });
  const ring = h('div', { class: 'ring' }, knob);
  const el = h('div', { class: 'stick' }, ring);
  let id = null;
  const move = (e) => {
    const max = ring.offsetWidth * 0.32;
    let [dx, dy] = fromCentre(el, e);
    const d = Math.hypot(dx, dy);
    if (d > max) [dx, dy] = [(dx / d) * max, (dy / d) * max];
    knob.style.transform = `translate(${dx}px, ${dy}px)`;
    onMove(dx / max, dy / max);
  };
  el.addEventListener('pointerdown', (e) => {
    if (id !== null) return;
    e.preventDefault();
    capture(el, e);
    id = e.pointerId;
    el.classList.add('down');
    move(e);
  });
  el.addEventListener('pointermove', (e) => e.pointerId === id && move(e));
  const up = (e) => {
    if (e.pointerId !== id) return;
    id = null;
    el.classList.remove('down');
    knob.style.transform = '';
    onMove(0, 0);
  };
  el.addEventListener('pointerup', up);
  el.addEventListener('pointercancel', up);
  return el;
}

/** The D-pad: one surface, 8 directions; the thumb can slide from one to the next. */
function dpadEl(onChange) {
  const arms = { up: h('i', { class: 'arm up' }), down: h('i', { class: 'arm down' }), left: h('i', { class: 'arm left' }), right: h('i', { class: 'arm right' }) };
  const el = h('div', { class: 'cross' }, ...Object.values(arms), h('i', { class: 'hub' }));
  let id = null;
  let cur = '';
  const set = (dirs) => {
    const key = dirs.join();
    if (key === cur) return;
    cur = key;
    for (const [k, a] of Object.entries(arms)) a.classList.toggle('on', dirs.includes(k));
    if (dirs.length) buzz(8);
    onChange(dirs);
  };
  const move = (e) => {
    const [dx, dy] = fromCentre(el, e);
    if (Math.hypot(dx, dy) < el.offsetWidth * 0.1) return set([]);
    // within 22.5° of an axis: one direction; between two: both (a diagonal)
    const t = 0.414;
    const dirs = [];
    if (dy < -Math.abs(dx) * t) dirs.push('up');
    if (dy > Math.abs(dx) * t) dirs.push('down');
    if (dx < -Math.abs(dy) * t) dirs.push('left');
    if (dx > Math.abs(dy) * t) dirs.push('right');
    set(dirs);
  };
  el.addEventListener('pointerdown', (e) => {
    if (id !== null) return;
    e.preventDefault();
    capture(el, e);
    id = e.pointerId;
    move(e);
  });
  el.addEventListener('pointermove', (e) => e.pointerId === id && move(e));
  const up = (e) => {
    if (e.pointerId !== id) return;
    id = null;
    set([]);
  };
  el.addEventListener('pointerup', up);
  el.addEventListener('pointercancel', up);
  return el;
}

/**
 * A full-screen pad over the page, laid out like an Xbox pad: left stick and
 * D-pad, A B X Y and right stick, shoulders and triggers on top, − and + in
 * the middle. The TV sees one more pad, i.e. one more player, in its menus
 * and in every game until the phone leaves (✕). No Home button: held, it
 * would close the game. Held upright, the pad turns itself sideways (the
 * phone may be locked in portrait).
 */
function controller() {
  let bits = 0;
  const axes = [0, 0, 0, 0]; // lx ly rx ry
  const trig = [0, 0];
  let queued = false;
  const flush = () => {
    queued = false;
    sendTv({ t: 'pad', b: bits, lx: axes[0], ly: axes[1], rx: axes[2], ry: axes[3], lt: trig[0], rt: trig[1] });
  };
  // buttons go out at once (a quick tap is a press and a release within one
  // frame); the sticks at most every 16 ms
  const changed = () => {
    if (!queued) {
      queued = true;
      setTimeout(() => queued && flush(), 16);
    }
  };

  const status = h('span', { class: 'who-play' }, 'Mi collego…');
  const button = (label, cls, onDown, onUp) => {
    const el = h('button', { class: `cb ${cls}` }, label);
    el.addEventListener('pointerdown', (e) => {
      e.preventDefault();
      capture(el, e);
      el.classList.add('down');
      buzz(10);
      onDown();
      flush();
    });
    const up = () => {
      if (!el.classList.contains('down')) return;
      el.classList.remove('down');
      onUp();
      flush();
    };
    el.addEventListener('pointerup', up);
    el.addEventListener('pointercancel', up);
    el.addEventListener('contextmenu', (e) => e.preventDefault());
    return el;
  };
  const btn = (label, key, cls = '') =>
    button(label, cls, () => (bits |= 1 << BTN[key]), () => (bits &= ~(1 << BTN[key])));
  const trigger = (label, i) => button(label, 'trig', () => (trig[i] = 1), () => (trig[i] = 0));
  const stick = (i) =>
    stickEl((x, y) => {
      axes[i] = x;
      axes[i + 1] = y;
      changed();
    });
  const dpad = dpadEl((dirs) => {
    for (const d of ['up', 'down', 'left', 'right']) bits = dirs.includes(d) ? bits | (1 << BTN[d]) : bits & ~(1 << BTN[d]);
    flush();
  });

  const close = (note) => {
    sendTv({ t: 'leave' });
    onPlayer = null;
    onTv.delete(watchLink);
    el.remove();
    native && native.setLandscape && native.setLandscape(false);
    if (note) toast(note);
  };
  const place = (cls, node) => h('div', { class: `spot ${cls}` }, node);
  const el = h(
    'div',
    { class: 'ctl' },
    h(
      'div',
      { class: 'ctl-top' },
      h('div', { class: 'shoulders' }, trigger('ZL', 0), btn('L', 'lb')),
      h('div', { class: 'mid' }, status, h('button', { class: 'cx', 'aria-label': 'Esci', onclick: () => close() }, icon('close'))),
      h('div', { class: 'shoulders' }, btn('R', 'rb'), trigger('ZR', 1)),
    ),
    h(
      'div',
      { class: 'ctl-main' },
      place('p-ls', stick(0)),
      place('p-dp', dpad),
      place('p-sys', h('div', { class: 'sys' }, btn('−', 'back', 'small'), btn('+', 'start', 'small'))),
      place('p-rs', stick(2)),
      place('p-face', h('div', { class: 'face' }, btn('Y', 'y', 'fy'), btn('X', 'x', 'fx'), btn('B', 'b', 'fb'), btn('A', 'a', 'fa'))),
    ),
  );
  // the TV forgets the pad when the connection drops: join again once back
  let linked = connected;
  const watchLink = () => {
    if (connected && !linked) sendTv({ t: 'join' });
    if (!connected) status.textContent = 'Mi ricollego…';
    linked = connected;
  };
  onTv.add(watchLink);
  onPlayer = (m) => {
    if (m.error) return close(m.error);
    status.textContent = m.player ? `Giocatore ${m.player}` : 'Collegato (oltre il 4° giocatore)';
    buzz(30);
  };
  document.body.append(el);
  native && native.setLandscape && native.setLandscape(true);
  sendTv({ t: 'join' });
}

// ---- photos -------------------------------------------------------------------

let albums = null;
const thumb = (id) => `/api/thumb/${id}?k=${token}`;
const count = (n) => (n === 1 ? '1 elemento' : `${n} elementi`);

function photos() {
  const box = h('section', {});
  const err = h('p', { class: 'err' });
  const name = h('input', { class: 'field', placeholder: 'Nome del nuovo album', maxlength: '80', enterkeyhint: 'done' });
  const create = h('button', { class: 'btn primary', type: 'submit' }, 'Crea');
  const form = h(
    'form',
    {
      class: 'newalbum',
      hidden: true,
      onsubmit: async (e) => {
        e.preventDefault();
        if (!name.value.trim()) return;
        create.disabled = true;
        try {
          const a = await api('/api/albums', { method: 'POST', body: { name: name.value.trim() } });
          albums = null;
          openAlbum(a);
        } catch (e) {
          err.textContent = e.message;
          create.disabled = false;
        }
      },
    },
    name,
    create,
  );
  const grid = h('div', { class: 'albums' });
  box.append(
    h(
      'div',
      { class: 'head' },
      h('h1', {}, 'Album'),
      h(
        'button',
        {
          class: 'btn',
          onclick: () => {
            form.hidden = !form.hidden;
            if (!form.hidden) name.focus();
          },
        },
        icon('plus'),
        'Nuovo album',
      ),
    ),
    wants.shareCount ? h('p', { class: 'muted small', style: 'margin-top:.6rem' }, `Scegli l'album per ${shareLabel()}.`) : '',
    form,
    err,
    grid,
  );
  const fill = () =>
    grid.replaceChildren(
      ...albums.map((a) =>
        h(
          'button',
          { class: 'album', onclick: () => openAlbum(a) },
          h('div', { class: 'cover' }, a.cover ? h('img', { src: thumb(a.cover), alt: '', loading: 'lazy' }) : icon('album')),
          h('div', { class: 'n' }, a.name),
          h('div', { class: 'c' }, count(a.count)),
        ),
      ),
    );
  if (albums) fill();
  else {
    grid.append(h('p', { class: 'muted' }, 'Carico gli album…'));
    api('/api/albums').then(
      (list) => {
        albums = list;
        fill();
      },
      (e) => {
        if (e instanceof Unpaired) return pairScreen('', e.message);
        grid.replaceChildren();
        err.textContent = e.message;
      },
    );
  }
  return box;
}

// photos shared from another Android app wait in the app until an album is picked
function shareCount() {
  try {
    return native && native.sharedCount ? native.sharedCount() : 0;
  } catch {
    return 0;
  }
}
const shareLabel = () => (wants.shareCount === 1 ? 'la foto condivisa' : `le ${wants.shareCount} foto condivise`);
if (wants.share) wants.shareCount = shareCount();

function openAlbum(a) {
  const picker = h('input', { type: 'file', accept: 'image/*,video/*', multiple: true, hidden: true });
  const status = h('div');
  const err = h('p', { class: 'err' });
  const addBtn = h('button', { class: 'btn primary wide', onclick: () => picker.click() }, icon('plus'), wants.shareCount ? `Aggiungi ${shareLabel()}` : 'Aggiungi foto e video');
  const showBtn = h(
    'button',
    {
      class: 'btn wide',
      onclick: async () => {
        err.textContent = '';
        try {
          const r = await api('/api/show', { method: 'POST', body: { album: a.id } });
          buzz(20);
          showBtn.textContent = r.ask ? '✓ Conferma il tuo profilo sulla TV' : '✓ Sulla TV';
        } catch (e) {
          err.textContent = e.message;
        }
      },
    },
    icon('play'),
    'Mostra l’album sulla TV',
  );
  picker.addEventListener('change', () => {
    const files = [...picker.files];
    picker.value = '';
    if (files.length) uploadAll(a, files, status, addBtn);
  });
  page.replaceChildren(
    h(
      'section',
      {},
      h('button', { class: 'back', onclick: () => page.replaceChildren(photos()) }, icon('left'), 'Album'),
      h(
        'div',
        { class: 'hero' },
        h('div', { class: 'cover' }, a.cover ? h('img', { src: thumb(a.cover), alt: '' }) : null),
        h('div', {}, h('h1', {}, a.name), h('p', { class: 'muted small' }, count(a.count))),
      ),
      h('div', { class: 'actions' }, addBtn, a.count ? showBtn : null, picker),
      err,
      status,
    ),
  );
  window.scrollTo(0, 0);
}

/** A file's type: some phones leave it empty (HEIC, some videos). */
function typeOf(f) {
  if (f.type) return f.type;
  const ext = (f.name.split('.').pop() || '').toLowerCase();
  return (
    { heic: 'image/heic', heif: 'image/heif', jpg: 'image/jpeg', jpeg: 'image/jpeg', png: 'image/png', webp: 'image/webp', gif: 'image/gif', mp4: 'video/mp4', mov: 'video/quicktime', '3gp': 'video/3gpp', mkv: 'video/x-matroska' }[ext] ||
    'application/octet-stream'
  );
}

function uploadOne(album, file, onProgress) {
  return new Promise((resolve, reject) => {
    const q = new URLSearchParams({ album: album.id, name: file.name || 'foto.jpg', modified: String(file.lastModified || Date.now()) });
    const x = new XMLHttpRequest();
    x.open('POST', `/api/upload?${q}`);
    x.setRequestHeader('x-azoto-key', token);
    x.setRequestHeader('content-type', typeOf(file));
    x.upload.onprogress = (e) => e.lengthComputable && onProgress(e.loaded / e.total);
    x.onload = () => {
      let data = {};
      try {
        data = JSON.parse(x.responseText);
      } catch {}
      if (x.status === 401) {
        keep('');
        return reject(new Unpaired(data.error || 'Telefono non collegato'));
      }
      x.status >= 200 && x.status < 300 ? resolve(data) : reject(new Error(data.error || `Errore ${x.status}`));
    };
    x.onerror = () => reject(new Error('Invio interrotto'));
    x.send(file);
  });
}

async function uploadAll(album, files, status, addBtn) {
  wants.shareCount = 0;
  addBtn.disabled = true;
  const label = h('p', {});
  const bar = h('i');
  const thumbs = h('div', { class: 'thumbs' });
  const note = h('p', { class: 'muted small' }, 'Tieni aperta questa pagina finché finisce.');
  const failures = h('p', { class: 'err' });
  status.replaceChildren(h('div', { class: 'progress' }, label, h('div', { class: 'bar' }, bar), thumbs, note, failures));
  const total = files.reduce((s, f) => s + (f.size || 1), 0);
  let doneBytes = 0;
  const ids = [];
  let failed = 0;
  for (const [i, f] of files.entries()) {
    label.textContent = `Invio ${i + 1} di ${files.length}…`;
    try {
      const r = await uploadOne(album, f, (p) => (bar.style.width = `${Math.round(((doneBytes + p * (f.size || 1)) / total) * 100)}%`));
      ids.push(r.id);
      if (ids.length <= 10) thumbs.append(f.type.startsWith('image/') && f.size < 15e6 ? h('img', { src: URL.createObjectURL(f), alt: '' }) : h('div', { class: 'ph' }));
    } catch (e) {
      if (e instanceof Unpaired) return pairScreen('', e.message);
      failed++;
      failures.textContent = `${failed} non inviat${failed === 1 ? 'a' : 'e'}: ${e.message}`;
    }
    doneBytes += f.size || 1;
    bar.style.width = `${Math.round((doneBytes / total) * 100)}%`;
  }
  albums = null;
  addBtn.disabled = false;
  note.remove();
  if (!ids.length) return (label.textContent = 'Nessuna foto inviata');
  buzz(30);
  label.textContent = `✓ ${ids.length === 1 ? '1 elemento aggiunto' : `${ids.length} elementi aggiunti`} a «${album.name}»`;
  api('/api/added', { method: 'POST', body: { album: album.name, n: ids.length } }).catch(() => {});
  const err = h('p', { class: 'err' });
  const btn = h(
    'button',
    {
      class: 'btn primary wide',
      onclick: async () => {
        err.textContent = '';
        try {
          const r = await api('/api/show', { method: 'POST', body: { ids } });
          buzz(20);
          btn.textContent = r.ask ? '✓ Conferma il tuo profilo sulla TV' : '✓ Sulla TV';
        } catch (e) {
          err.textContent = e.message;
        }
      },
    },
    icon('play'),
    'Mostrale subito sulla TV',
  );
  status.firstChild.append(btn, err);
}

// ---- send a link -------------------------------------------------------------

let sendMsg = null;

function sendPage() {
  const field = h('textarea', { class: 'field', placeholder: 'Incolla un link: YouTube, Instagram, TikTok…', rows: '3' });
  const msg = h('p', { class: 'okmsg' });
  sendMsg = msg;
  const btn = h(
    'button',
    {
      class: 'btn primary wide',
      onclick: async () => {
        if (!field.value.trim()) return field.focus();
        btn.disabled = true;
        if (await sendLink(field.value)) field.value = '';
        btn.disabled = false;
      },
    },
    icon('send'),
    'Manda alla TV',
  );
  const how = native
    ? h('div', { class: 'hint' }, 'Dalle altre app: tocca ', h('b', {}, 'Condividi'), ' e scegli ', h('b', {}, 'AzotoTV'), '. Il video parte subito sulla TV.')
    : h('div', { class: 'hint' }, 'Nell’app (YouTube, Instagram…) tocca ', h('b', {}, 'Condividi › Copia link'), ', poi incollalo qui.');
  return h('section', { class: 'send' }, h('h1', {}, 'Manda alla TV'), field, btn, msg, how, androidBrowser ? h('p', { class: 'muted small' }, 'Con l’app Android basta Condividi › AzotoTV, senza copiare il link.') : null, getApp());
}

async function sendLink(text) {
  const msg = sendMsg;
  if (msg) {
    msg.className = 'okmsg';
    msg.textContent = 'Mando…';
  }
  try {
    const r = await api('/api/send', { method: 'POST', body: { url: text } });
    buzz(25);
    if (msg) msg.textContent = r.ask ? '✓ Mandato: sulla TV conferma il passaggio al tuo profilo' : r.kind === 'youtube' ? '✓ Parte su YouTube sulla TV' : '✓ Mandato alla TV';
    return true;
  } catch (e) {
    if (e instanceof Unpaired) {
      pairScreen('', e.message);
      return false;
    }
    if (msg) {
      msg.className = 'err';
      msg.textContent = e.message;
    }
    return false;
  }
}

// ---- start ------------------------------------------------------------------

async function start() {
  if (!token) return pairScreen(wants.code);
  try {
    me = await api('/api/me');
  } catch (e) {
    if (e instanceof Unpaired) return pairScreen(wants.code, wants.code ? '' : e.message);
    // the TV is off or out of reach: try again soon
    show(h('div', { class: 'center' }, logo(), h('h1', {}, 'La TV non risponde'), h('p', { class: 'muted' }, e.message), h('button', { class: 'btn primary wide', onclick: start }, 'Riprova')));
    return;
  }
  setTimeout(reportScreen, 1500);
  // already paired, and the QR code of another profile: add it
  if (wants.code) {
    const code = wants.code;
    wants.code = null;
    return pairScreen(code, '', true);
  }
  main();
}

start();
