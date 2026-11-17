// AzotoTV on the phone, served by AzotoTV itself (src-tauri/src/phone.rs) and
// laid out like the iPhone app: sign in (a profile's PIN, or the TV's code),
// then Home (the profile's apps: TV, Live TV, Music, Gallery, Games,
// Requests, playing here on the phone), Remote and Send.

import { tr, h, fill, icon, wordmark, avatar, spinner, btn, toast, on, emit, sheet, menu, confirmBox, buzz, Stack, native, can, pullToRefresh, retitle, clearLayers } from './ui.js';
import { api, login, store, account, deviceLabel, tv, linkStart, linkStop, Unauthorized, Refused, sendLink, jf } from './api.js';
import { model, APPS, setStacks, loadHome } from './model.js';
import { remotePage, openPad, tvStatusCard } from './remote.js';
import { sendPage, photosPage, sharedPhotos } from './photos.js';
import { tvAppPage, livePage, openVideo, openLive } from './watch.js';
import { musicPage, miniPlayer, music } from './music.js';
import { gamesPage, requestsPage, openScreen } from './extra.js';

const root = document.getElementById('app');

// what the address asks for: #c=<code> (the TV's QR code), #send=<link>, #share
// (the Android app's share menu); read before the address is rewritten
const hash = new URLSearchParams(location.hash.slice(1));
const wants = { code: hash.get('c'), send: hash.get('send'), share: hash.has('share') };
history.replaceState({ azoto: 0 }, '', location.pathname + location.search);

// no pinch-zoom or rubber-banding: it's an app (the photo viewer zooms by itself)
for (const ev of ['gesturestart', 'gesturechange']) document.addEventListener(ev, (e) => e.preventDefault(), { passive: false });
document.addEventListener('touchmove', (e) => e.touches.length > 1 && e.preventDefault(), { passive: false });

let server = '';
let phase = '';

// ---- getting going -------------------------------------------------------------

/** At start and when coming back: find the TV, then the profile's home or "Chi sei?". */
async function connect() {
  if (phase !== 'home' || !login) setPhase('searching');
  let hello;
  try {
    hello = await api('/api/hello');
  } catch {
    linkStop();
    return setPhase('away');
  }
  server = hello.server || '';
  if (login && login.server && server && login.server !== server) return profiles();
  if (login) {
    try {
      // the first phone page kept only the key: ask whose it is
      if (!login.users.length || !login.server) {
        const me = await api('/api/me');
        store({ token: login.token, server, users: me.users, active: login.active || me.users[0]?.id || '' });
      }
      if (!login) return profiles();
      await loadHome();
      linkStart();
      if (phase !== 'home') setPhase('home');
      return;
    } catch (e) {
      if (e instanceof Unauthorized) store(null);
      else if (e instanceof Refused) signOutLocally(login.active);
      else {
        linkStop();
        return setPhase('away');
      }
    }
  }
  profiles();
}

function setPhase(p) {
  phase = p;
  if (p === 'searching') fill(root, h('div', { class: 'center fill gap20' }, wordmark(), spinner()));
  else if (p === 'away') fill(root, awayView());
  else if (p === 'home') mainView();
}

function awayView() {
  const retry = btn(
    tr('Retry'),
    async () => {
      retry.disabled = true;
      await connect();
      retry.disabled = false;
    },
    { kind: 'primary', cls: 'big' },
  );
  return h(
    'div',
    { class: 'center fill gap20 pad40' },
    wordmark(),
    icon('wifiOff', 'huge dim'),
    h('div', { class: 'title3' }, tr("Can't find the TV")),
    h('p', { class: 'dim tc' }, tr('AzotoTV works on the home Wi-Fi. Connect to it and try again.')),
    retry,
  );
}

function signOutLocally(user) {
  if (!login) return;
  const users = user ? login.users.filter((u) => u.id !== user) : [];
  store(users.length ? { ...login, users, active: login.active === user ? users[0].id : login.active } : null);
}

// ---- "Chi sei?" --------------------------------------------------------------------------

async function profiles() {
  linkStop();
  phase = 'profiles';
  const grid = h('div', { class: 'profgrid' }, spinner());
  fill(
    root,
    h(
      'div',
      { class: 'profiles' },
      h('div', { class: 'topleft' }, login ? btn(tr('Cancel'), () => setPhase('home')) : null),
      wordmark(),
      h('h1', { class: 'bigtitle tc' }, tr('Who are you?')),
      grid,
      h('div', { class: 'codebox' }, btn(tr('I have a code from the TV'), () => codeSheet(), { icon: 'tv', cls: 'wide' })),
    ),
  );
  const load = async () => {
    try {
      const list = await api('/api/app/profiles');
      if (!list.length) {
        fill(
          grid,
          h(
            'div',
            { class: 'empty tc' },
            icon('lock', 'huge dim'),
            h('div', { class: 'headline' }, tr('On the phone you open profiles with their PIN.')),
            h('p', { class: 'dim' }, tr('On the TV open your profile, then START › Profile PIN. Or use the code the TV shows in START › Connect a phone.')),
          ),
        );
        return;
      }
      fill(
        grid,
        list.map((p) =>
          h(
            'button',
            { class: 'prof', type: 'button', onclick: () => (buzz(), pinSheet(p)) },
            h('div', { class: 'profpic' }, avatar(p.id, p.name, 92, p.image), login?.users.some((u) => u.id === p.id) ? icon('checkCircle', 'signedin') : null),
            h('div', { class: 'headline ellipsis' }, p.name),
          ),
        ),
      );
    } catch (e) {
      fill(grid, h('div', { class: 'empty tc' }, h('p', { class: 'dim' }, e.message), btn(tr('Retry'), load)));
    }
  };
  load();
  if (wants.code) {
    const c = wants.code;
    wants.code = null;
    codeSheet(c);
  }
}

/**
 * Four digits on a phone-style pad: a profile's PIN, or the code the TV shows
 * in START › Connect a phone.
 */
function keypadSheet({ who, prompt, submit, prefill = '' }) {
  sheet(
    (close) => {
      let pin = '';
      let busy = false;
      const dots = h('div', { class: 'pindots' });
      const msg = h('div', { class: 'pinmsg dim' }, prompt);
      const drawDots = () => fill(dots, [0, 1, 2, 3].map((i) => h('i', { class: i < pin.length ? 'on' : '' })));
      drawDots();
      const press = async (k) => {
        if (busy) return;
        buzz();
        if (k === '⌫') pin = pin.slice(0, -1);
        else if (pin.length < 4) pin += k;
        drawDots();
        if (pin.length < 4) return;
        busy = true;
        pad.classList.add('busy');
        try {
          await submit(pin);
          close();
        } catch (e) {
          msg.textContent = e.message;
          msg.className = 'pinmsg red';
          pin = '';
          drawDots();
          dots.classList.remove('shake');
          void dots.offsetWidth;
          dots.classList.add('shake');
          buzz(60);
        }
        busy = false;
        pad.classList.remove('busy');
      };
      const pad = h(
        'div',
        { class: 'keypad' },
        ['1', '2', '3', '4', '5', '6', '7', '8', '9', '', '0', '⌫'].map((k) =>
          k === '' ? h('span') : h('button', { class: 'key' + (k === '⌫' ? ' del' : ''), type: 'button', onclick: () => press(k) }, k === '⌫' ? icon('delete') : k),
        ),
        h('div', { class: 'keyspin' }, spinner()),
      );
      if (prefill) setTimeout(() => [...prefill.slice(0, 4)].forEach(press), 300);
      return h('div', { class: 'pinsheet' }, who, msg, dots, h('div', { class: 'grow' }), pad);
    },
    { full: true },
  );
}

function pinSheet(p) {
  keypadSheet({
    who: h('div', { class: 'center gap8' }, avatar(p.id, p.name, 84, p.image), h('div', { class: 'title2' }, p.name)),
    prompt: tr('Enter the PIN'),
    submit: async (pin) => {
      const r = await api('/api/app/login', { body: { user: p.id, pin, device: deviceLabel(p.name) } });
      signedIn({ token: r.token, server: r.server || server, users: r.users, active: p.id });
    },
  });
}

function codeSheet(prefill = '') {
  keypadSheet({
    who: h('div', { class: 'center gap8' }, h('div', { class: 'codeicon' }, icon('tv')), h('div', { class: 'title2' }, tr('Code from the TV'))),
    prompt: tr('On the TV: open your profile, then START › Connect a phone'),
    prefill,
    submit: async (code) => {
      const before = new Set((login?.users || []).map((u) => u.id));
      const r = await api('/api/pair', { body: { code, device: deviceLabel() } });
      const added = r.users.find((u) => !before.has(u.id)) || r.users[r.users.length - 1];
      signedIn({ token: r.token, server: r.server || server, users: r.users, active: added?.id || r.users[0]?.id });
    },
  });
}

async function signedIn(l) {
  store(l);
  model.tab = 'home';
  try {
    await loadHome();
  } catch (e) {
    toast(e.message);
  }
  linkStart();
  setPhase('home');
}

// ---- the signed-in app: tabs -------------------------------------------------------------

const TABS = [
  ['home', 'grid', 'Home'],
  ['remote', 'remote', 'Remote'],
  ['send', 'send', 'Send'],
];

let tabEls = {};

function mainView() {
  for (const s of Object.values(model.stacks)) for (const p of s.pages) p.dispose?.();
  clearLayers();
  const stacks = {};
  const panes = {};
  for (const [name] of TABS) {
    panes[name] = h('div', { class: 'pane', 'data-tab': name, hidden: name !== model.tab });
    stacks[name] = new Stack(panes[name]);
  }
  setStacks(stacks);
  tabEls = {};
  const bar = h(
    'nav',
    { class: 'tabbar' },
    TABS.map(([name, ic, label]) =>
      (tabEls[name] = h('button', { class: 'tab' + (name === model.tab ? ' on' : ''), type: 'button', onclick: () => selectTab(name, true) }, icon(ic), h('span', {}, tr(label)))),
    ),
  );
  fill(root, h('div', { class: 'main' }, Object.values(panes), h('div', { class: 'bottom' }, miniPlayer(), bar)));
  stacks.home.root(homePage());
  stacks.remote.root(remotePage());
  stacks.send.root(sendPage());
  selectTab(model.tab);
  handleWants();
}

/** Shows a tab; tapping the tab that's on goes back to its first page. */
export function selectTab(name, tapped) {
  if (tapped && model.tab === name) model.stacks[name]?.home();
  const was = model.tab;
  model.tab = name;
  for (const [n, el] of Object.entries(tabEls)) el.classList.toggle('on', n === name);
  for (const pane of root.querySelectorAll('.pane')) pane.hidden = pane.dataset.tab !== name;
  if (was !== name) {
    model.stacks[was]?.top?.onHide?.();
    model.stacks[name]?.top?.onShow?.();
  }
  emit('tab', name);
}

/** The address asked for something (the TV's QR code, the Android share menu). */
function handleWants() {
  if (wants.code) {
    const c = wants.code;
    wants.code = null;
    codeSheet(c);
  }
  if (wants.send) {
    const link = wants.send;
    wants.send = null;
    selectTab('send');
    emit('send-link', link);
    sendLink(link).then((ok) => ok && can('sharedDone') && setTimeout(() => native.sharedDone(), 900));
  }
  if (wants.share) {
    wants.share = false;
    sharedPhotos();
  }
}

// ---- Home ---------------------------------------------------------------------------------

const hello = () => (account() ? tr('Hi, {name}', { name: account().name }) : 'AzotoTV');

function homePage() {
  const screen = h('div', { class: 'screenslot' });
  const tiles = h('div', { class: 'tiles' });
  const controller = h('button', { class: 'card ctl', type: 'button', onclick: () => openPad() });
  const body = h('div', { class: 'content home' }, tvStatusCard(), screen, tiles, controller);

  const drawTiles = () => {
    if (!model.apps.length) {
      fill(tiles, h('p', { class: 'dim pad30 span2' }, tr('This profile has no apps on the TV. Add them from the Store, on the TV.')));
      return;
    }
    fill(
      tiles,
      model.apps.map((id) => {
        const a = APPS[id];
        return h(
          'button',
          { class: 'tile', type: 'button', style: { background: `linear-gradient(135deg, ${a.colors[0]}, ${a.colors[1]})` }, onclick: () => openApp(id) },
          icon(a.icon, 'tileicon'),
          h('span', { class: 'tiletitle' }, tr(a.title)),
        );
      }),
    );
  };

  const drawCards = () => {
    const s = tv.state;
    const g = s.game;
    fill(
      screen,
      g && g.screen
        ? h(
            'button',
            { class: 'card accent', type: 'button', onclick: () => openScreen(g) },
            icon(g.console === '3ds' ? 'pad' : 'hand', 'cardicon'),
            h(
              'div',
              { class: 'grow' },
              h('div', { class: 'headline' }, g.console === '3ds' ? tr('3DS bottom screen') : tr('Touch screen')),
              h(
                'div',
                { class: 'caption dim' },
                g.console === '3ds' ? tr('The phone becomes the lower half of the 3DS: touch screen and controls') : tr('See {game} on the phone and touch it like on the Switch', { game: g.name }),
              ),
            ),
            icon('chevron', 'dim'),
          )
        : null,
    );
    const loud = s.controllers || (g?.players || 1) > 1;
    controller.classList.toggle('loud', loud);
    fill(
      controller,
      icon('pad', 'cardicon'),
      h(
        'div',
        { class: 'grow' },
        h(
          'div',
          { class: 'headline' },
          s.controllers ? tr('The TV is looking for controllers') : g && g.players > 1 ? tr('{game} · up to {n} players', { game: g.name, n: g.players }) : tr('Use as a controller'),
        ),
        h('div', { class: 'caption dim' }, loud ? tr('Tap here to play with the phone') : tr('The phone becomes a controller for the TV and its games')),
      ),
      icon('chevron', 'dim'),
    );
  };

  drawTiles();
  drawCards();
  on('home', drawTiles, body);
  on('tv', drawCards, body);
  const page = { title: hello(), short: 'Home', large: true, body, actions: () => [profileButton()] };
  on('home', () => retitle(page, hello()), body);
  setTimeout(() => pullToRefresh(page.scroller, () => loadHome()), 0);
  return page;
}

function openApp(id) {
  const push = (p) => model.stacks.home.push(p);
  switch (id) {
    case 'giochi':
      return push(gamesPage());
    case 'galleria':
      return push(photosPage());
    case 'tv':
      return push(tvAppPage());
    case 'live':
      return push(livePage());
    case 'music':
      return push(musicPage());
    case 'richieste':
      return push(requestsPage());
    default:
      return push(comingSoon(id));
  }
}

/** YouTube: plays on the phone in a later version. */
function comingSoon(id) {
  const a = APPS[id];
  return {
    title: '',
    short: tr(a.title),
    body: h(
      'div',
      { class: 'center fill gap16 pad40' },
      icon(a.icon, 'huge'),
      h('h1', { class: 'bigtitle' }, tr(a.title)),
      h('p', { class: 'dim tc' }, tr('Soon you can watch it here on the phone too, with the TV off. For now use the remote.')),
    ),
  };
}

/** The avatar at the top: switch profile, add one, sign out. */
function profileButton() {
  const a = account();
  if (!a) return null;
  return h(
    'button',
    {
      class: 'avatarbtn',
      type: 'button',
      'aria-label': tr('Profiles'),
      onclick: () =>
        menu([
          ...login.users.map((u) => ({ label: u.name, checked: u.id === login.active, run: () => switchTo(u) })),
          null,
          { label: tr('Add a profile'), icon: 'personPlus', run: () => profiles() },
          {
            label: tr('Sign out of this profile'),
            icon: 'logout',
            danger: true,
            run: async () => {
              const ok = await confirmBox(tr("Sign out of {name}'s profile?", { name: a.name }), tr("To come back you'll need the PIN or the TV's code again."), tr('Sign out'));
              if (ok) signOut();
            },
          },
        ]),
    },
    avatar(a.id, a.name, 34),
  );
}

async function switchTo(u) {
  if (!login || login.active === u.id) return;
  store({ ...login, active: u.id });
  music.stop();
  try {
    await loadHome();
  } catch (e) {
    toast(e.message);
  }
  mainView();
}

async function signOut() {
  const a = account();
  if (!a) return;
  try {
    await api('/api/app/logout', { body: { user: a.id } });
  } catch {}
  music.stop();
  signOutLocally(a.id);
  if (!login) return profiles();
  try {
    await loadHome();
  } catch {}
  mainView();
}

// ---- "Continua qui" (on the TV status card) ------------------------------------------------

/** What the TV plays (for one of this phone's profiles) stops there and goes on here. */
async function continueHere() {
  const u = login?.users.find((x) => x.id === tv.state.user);
  if (!u) return;
  if (login.active !== u.id) await switchTo(u);
  try {
    const r = await api('/api/app/tv/take', { body: { user: u.id } });
    if (r.item) openVideo(await jf().item(r.item), { startTicks: r.ticks });
    else if (r.channel) openLive(r.channel);
  } catch (e) {
    toast(e.message);
  }
}

model.continueHere = continueHere;

// ---- start ------------------------------------------------------------------------------------

document.addEventListener('visibilitychange', () => {
  if (document.visibilityState === 'visible') {
    if (phase === 'home' || phase === 'away') connect();
  } else if (!music.playing) {
    linkStop();
  }
});

// the Android app hands over things shared to it while it's open
window.azotoShared = (what) => {
  if (what && what.startsWith('#send=')) wants.send = decodeURIComponent(what.slice(6));
  else if (what === '#share') wants.share = true;
  if (phase === 'home') handleWants();
};

connect();
