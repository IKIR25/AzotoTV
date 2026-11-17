// The remote (a touchpad and the remote's buttons, typing into the TV's
// keyboard, the volume, power), the TV status card and the phone as a game
// controller.

import { tr, h, fill, icon, btn, on, buzz, onPress, cover, confirmBox, landscape, keepAwake, native, can } from './ui.js';
import { tv, login, key, typeText, volume, power, sendTv } from './api.js';
import { model } from './model.js';

// ---- what the TV is doing, in one line -----------------------------------------------

/** The TV status card (with "Continua qui" when the TV plays for one of this phone's profiles). */
export function tvStatusCard() {
  const continueHere = () => model.continueHere?.();
  const el = h('div', { class: 'card glass status' });
  const draw = () => {
    const s = tv.state;
    const mine = (id) => login?.users.some((u) => u.id === id);
    let title;
    if (!tv.connected) title = tr('Connecting to the TV…');
    else if (s.game) title = tr('On the TV: {name}', { name: s.game.name });
    else if (s.playing && !s.dark) title = tr('On the TV: {name}', { name: s.playing.name });
    else if (s.dark) title = tr('The TV is off');
    else if (s.picker) title = tr('The TV is on “Who’s watching?”');
    else title = s.user_name ? tr("{name} is on the TV", { name: s.user_name }) : tr('The TV is on');
    const sub = s.dark
      ? tr('You can still use AzotoTV on the phone')
      : s.user && !mine(s.user)
        ? tr('The remote will ask to switch to your profile')
        : tr('The remote is ready');
    const canContinue = continueHere && tv.connected && s.playing && s.user && mine(s.user);
    fill(
      el,
      icon(s.dark ? 'tvOff' : 'tv', 'statusicon'),
      h('div', { class: 'grow' }, h('div', { class: 'subhead' }, title), h('div', { class: 'caption dim' }, sub)),
      canContinue
        ? btn(tr('Continue here'), () => continueHere(), { kind: 'primary', icon: 'phone', cls: 'small' })
        : h('i', { class: 'dot ' + (tv.connected ? 'ok' : 'wait') }),
    );
  };
  draw();
  on('tv', draw, el);
  return el;
}

// ---- the remote tab ------------------------------------------------------------------------

export function remotePage() {
  const top = h('div', { class: 'remotetop' });
  const pad = touchpad((a) => key(a));
  const powerBtn = remoteButton('power', tr('Turn off'), () => power());
  powerBtn.classList.add('power');
  const body = h(
    'div',
    { class: 'remote' },
    top,
    pad,
    h(
      'div',
      { class: 'rrow' },
      remoteButton('back', tr('Back'), () => key('b')),
      remoteButton('home', 'Home', async () => {
        // in a game Home closes it (like holding Home on a pad): ask first
        if (tv.state.game) {
          const ok = await confirmBox(tr('Close the game?'), tr("It closes on the TV, like holding Home on the controller."), tr('Close {name}', { name: tv.state.game.name }));
          if (ok) key('home');
        } else key('home');
      }),
      remoteButton('menu', 'Menu', () => key('start')),
      remoteButton('playpause', tr('Play/Pause'), () => key('play')),
    ),
    h(
      'div',
      { class: 'rrow' },
      remoteButton('rew', tr('Rewind'), () => key('rew'), true),
      volumeRocker(),
      remoteButton('ff', tr('Forward'), () => key('ff'), true),
      powerBtn,
    ),
  );

  let keyboardOpen = null;
  const draw = () => {
    const s = tv.state;
    // the keyboard card stays while the TV's keyboard is open (redrawing would lose the focus)
    if (!!s.text !== keyboardOpen) {
      keyboardOpen = !!s.text;
      fill(top, s.text ? keyboardCard() : tvStatusCard());
    }
    powerBtn.querySelector('.rlabel').textContent = s.dark ? tr('Turn on') : tr('Turn off');
    powerBtn.classList.toggle('on', s.dark);
  };
  draw();
  on('tv', draw, body);

  return {
    title: tr('Remote'),
    body,
    actions: () => [h('button', { class: 'navbtn', type: 'button', 'aria-label': tr('Controller'), onclick: () => openPad() }, icon('pad'))],
    // the Android app's volume keys set the TV's volume while the remote is on screen
    onShow: () => can('setVolumeKeys') && native.setVolumeKeys(true),
    onHide: () => can('setVolumeKeys') && native.setVolumeKeys(false),
  };
}

function remoteButton(ic, label, action, repeat = false) {
  const b = h('button', { class: 'rbtn', type: 'button', 'aria-label': label }, h('span', { class: 'rcircle' }, icon(ic)), h('span', { class: 'rlabel' }, label));
  onPress(b, action, { repeat });
  return b;
}

function volumeRocker() {
  const value = h('span', { class: 'vval' }, '–');
  const minus = h('button', { class: 'vstep', type: 'button', 'aria-label': tr('Volume down') }, icon('minus'));
  const plus = h('button', { class: 'vstep', type: 'button', 'aria-label': tr('Volume up') }, icon('plus'));
  onPress(minus, () => volume(-1), { repeat: true });
  onPress(plus, () => volume(1), { repeat: true });
  const el = h('div', { class: 'rbtn vol' }, h('div', { class: 'rocker' }, minus, value, plus), h('span', { class: 'rlabel' }, tr('Volume')));
  const draw = () => (value.textContent = tv.state.volume == null ? '–' : String(tv.state.volume));
  draw();
  on('tv', draw, el);
  return el;
}

/**
 * Swipes become arrow presses (one per stretch of the finger, so a long swipe moves
 * several), a tap is OK, a long press is OK held (the TV's "hold A").
 */
function touchpad(fire) {
  const STEP = 46;
  const glow = h('div', { class: 'glow' });
  const hint = h('div', { class: 'tphint' }, icon('hand'), h('span', {}, tr('Swipe to move, tap for OK')));
  const el = h('div', { class: 'touchpad' }, hint, glow);
  let last = null;
  let moved = false;
  let held = false;
  let holdTimer;
  const local = (e) => {
    const r = el.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };
  el.addEventListener('pointerdown', (e) => {
    e.preventDefault();
    el.setPointerCapture(e.pointerId);
    last = local(e);
    moved = held = false;
    el.classList.add('touching');
    glow.style.transform = `translate(${last.x}px, ${last.y}px)`;
    clearTimeout(holdTimer);
    holdTimer = setTimeout(() => {
      if (moved) return;
      held = true;
      buzz(20);
      fire('a-hold');
    }, 550);
  });
  el.addEventListener('pointermove', (e) => {
    if (!last) return;
    const p = local(e);
    glow.style.transform = `translate(${p.x}px, ${p.y}px)`;
    const dx = p.x - last.x;
    const dy = p.y - last.y;
    if (Math.max(Math.abs(dx), Math.abs(dy)) < STEP) return;
    moved = true;
    clearTimeout(holdTimer);
    fire(Math.abs(dx) > Math.abs(dy) ? (dx > 0 ? 'right' : 'left') : dy > 0 ? 'down' : 'up');
    buzz();
    last = p;
  });
  const end = () => {
    if (!last) return;
    clearTimeout(holdTimer);
    if (!moved && !held) {
      buzz(14);
      fire('a');
    }
    last = null;
    el.classList.remove('touching');
  };
  el.addEventListener('pointerup', end);
  el.addEventListener('pointercancel', end);
  el.addEventListener('contextmenu', (e) => e.preventDefault());
  return el;
}

/** The TV's keyboard is open: type here, it shows there as you go. */
function keyboardCard() {
  const t = tv.state.text || {};
  const input = h('input', {
    class: 'field',
    type: t.secret ? 'password' : 'text',
    placeholder: t.secret ? tr('Password') : tr('Type…'),
    autocapitalize: 'off',
    autocomplete: 'off',
    autocorrect: 'off',
    spellcheck: 'false',
    enterkeyhint: 'done',
  });
  input.value = t.secret ? '' : t.value || '';
  let typedAt = 0;
  input.addEventListener('input', () => {
    typedAt = Date.now();
    typeText(input.value);
  });
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      typeText(input.value, true);
    }
  });
  const label = h('div', { class: 'kblabel caption dim' }, icon('keyboard'), h('span', {}, t.label ? tr('Type on the TV · {label}', { label: t.label }) : tr('Type on the TV')));
  const el = h('div', { class: 'card glass keyboard' }, label, h('div', { class: 'row gap' }, input, btn(tr('Done'), () => typeText(input.value, true), { kind: 'primary' })));
  // the TV's own changes (its on-screen keyboard), unless the user is typing here
  on(
    'tv',
    () => {
      const s = tv.state.text;
      if (!s) return;
      label.lastChild.textContent = s.label ? tr('Type on the TV · {label}', { label: s.label }) : tr('Type on the TV');
      if (!s.secret && Date.now() - typedAt > 1500 && s.value !== input.value) input.value = s.value;
    },
    el,
  );
  setTimeout(() => input.focus(), 50);
  return el;
}

// ---- the phone as a controller ------------------------------------------------------------------

/** Buttons as bits, in azotopad's order (phone.rs `pad_state`). */
export const PAD = { a: 0, b: 1, x: 2, y: 3, back: 4, start: 6, lb: 9, rb: 10, up: 11, down: 12, left: 13, right: 14 };

/** The controller's state: buttons go out at once, the sticks at most every 16 ms. */
export class PadState {
  bits = 0;
  axes = [0, 0, 0, 0];
  triggers = [0, 0];
  queued = false;

  set(b, on) {
    if (on) this.bits |= 1 << b;
    else this.bits &= ~(1 << b);
    this.flush();
  }
  trigger(i, on) {
    this.triggers[i] = on ? 1 : 0;
    this.flush();
  }
  stick(i, x, y) {
    this.axes[i * 2] = x;
    this.axes[i * 2 + 1] = y;
    if (this.queued) return;
    this.queued = true;
    setTimeout(() => this.queued && this.flush(), 16);
  }
  dpad(dirs) {
    for (const d of ['up', 'down', 'left', 'right']) {
      if (dirs.has(d)) this.bits |= 1 << PAD[d];
      else this.bits &= ~(1 << PAD[d]);
    }
    this.flush();
  }
  flush() {
    this.queued = false;
    const [lx, ly, rx, ry] = this.axes;
    sendTv({ t: 'pad', b: this.bits >>> 0, lx, ly, rx, ry, lt: this.triggers[0], rt: this.triggers[1] });
  }
  reset() {
    this.bits = 0;
    this.axes = [0, 0, 0, 0];
    this.triggers = [0, 0];
    this.flush();
  }
}

/**
 * Touches on a pad that may be turned sideways by CSS (an upright phone that can't
 * rotate the page): `delta(el, e)` is the finger's offset from `el`'s centre in the
 * element's own axes.
 */
function delta(el, e) {
  const r = el.getBoundingClientRect();
  const sx = e.clientX - (r.left + r.width / 2);
  const sy = e.clientY - (r.top + r.height / 2);
  return el.closest('.turned') ? { x: sy, y: -sx } : { x: sx, y: sy };
}

/** A button that reports down and up (so it can be held). */
export function padKey(label, change, cls = '') {
  const el = h('div', { class: 'pkey ' + cls }, label);
  const up = (e) => {
    if (!el.classList.contains('down')) return;
    el.classList.remove('down');
    change(false);
  };
  el.addEventListener('pointerdown', (e) => {
    e.preventDefault();
    el.setPointerCapture(e.pointerId);
    el.classList.add('down');
    buzz();
    change(true);
  });
  el.addEventListener('pointerup', up);
  el.addEventListener('pointercancel', up);
  el.addEventListener('lostpointercapture', up);
  return el;
}

/** A fixed stick: the knob follows the finger inside the ring; x and y in -1…1 (y down). */
export function stick(move, cls = '') {
  const knob = h('div', { class: 'knob' });
  const el = h('div', { class: 'stick ' + cls }, knob);
  let id = null;
  const at = (e) => {
    const max = el.offsetWidth * 0.32;
    let { x, y } = delta(el, e);
    const d = Math.hypot(x, y);
    if (d > max) {
      x = (x / d) * max;
      y = (y / d) * max;
    }
    knob.style.transform = `translate(${x}px, ${y}px)`;
    move(x / max, y / max);
  };
  el.addEventListener('pointerdown', (e) => {
    e.preventDefault();
    id = e.pointerId;
    el.setPointerCapture(id);
    at(e);
  });
  el.addEventListener('pointermove', (e) => e.pointerId === id && at(e));
  const end = (e) => {
    if (e.pointerId !== id) return;
    id = null;
    knob.style.transform = '';
    move(0, 0);
  };
  el.addEventListener('pointerup', end);
  el.addEventListener('pointercancel', end);
  return el;
}

/** An 8-way D-pad the thumb can slide on. */
export function dpad(change, cls = '') {
  const arrows = { up: icon('up'), down: icon('down'), left: icon('left'), right: icon('right') };
  for (const [d, a] of Object.entries(arrows)) a.classList.add('arrow', d);
  const el = h('div', { class: 'dpad ' + cls }, h('i', { class: 'arm v' }), h('i', { class: 'arm hz' }), Object.values(arrows));
  let dirs = new Set();
  let id = null;
  const table = [['left'], ['left', 'up'], ['up'], ['up', 'right'], ['right'], ['right', 'down'], ['down'], ['down', 'left']];
  const at = (e) => {
    const { x, y } = delta(el, e);
    let now = new Set();
    if (Math.hypot(x, y) > el.offsetWidth * 0.12) {
      const slice = Math.round((Math.atan2(y, x) + Math.PI) / (Math.PI / 4)) % 8;
      now = new Set(table[slice]);
    }
    if ([...now].join() !== [...dirs].join()) {
      if (now.size) buzz();
      dirs = now;
      for (const [d, a] of Object.entries(arrows)) a.classList.toggle('on', dirs.has(d));
      change(dirs);
    }
  };
  el.addEventListener('pointerdown', (e) => {
    e.preventDefault();
    id = e.pointerId;
    el.setPointerCapture(id);
    at(e);
  });
  el.addEventListener('pointermove', (e) => e.pointerId === id && at(e));
  const end = (e) => {
    if (e.pointerId !== id) return;
    id = null;
    dirs = new Set();
    for (const a of Object.values(arrows)) a.classList.remove('on');
    change(dirs);
  };
  el.addEventListener('pointerup', end);
  el.addEventListener('pointercancel', end);
  return el;
}

/** A B X Y where an Xbox pad has them (Y top, A bottom, X left, B right). */
export function faceButtons(set) {
  return h(
    'div',
    { class: 'face' },
    padKey('Y', (on) => set(PAD.y, on), 'round fy'),
    padKey('X', (on) => set(PAD.x, on), 'round fx'),
    padKey('B', (on) => set(PAD.b, on), 'round fb'),
    padKey('A', (on) => set(PAD.a, on), 'round fa'),
  );
}

/** The game's rumble on the phone's motor. */
function rumble(ms) {
  if (ms > 0) buzz(Math.min(ms, 2000));
  else
    try {
      if (native && native.vibrate) native.vibrate(1);
      else navigator.vibrate?.(0);
    } catch {}
}

/**
 * The phone as a pad, laid out like an Xbox controller: left stick and D-pad,
 * A B X Y and the right stick, shoulders and triggers on top, − and + in the
 * middle. The TV sees one more pad (one more player) until the phone leaves.
 * No Home button: held, it would close the game.
 */
export function openPad() {
  const pad = new PadState();
  const status = h('div', { class: 'padstatus' });
  const drawStatus = () => {
    status.textContent = tv.padError || (!tv.connected ? tr('Connecting…') : tv.player ? tr('Player {n}', { n: tv.player }) : tr('Connected'));
  };
  let wasConnected = tv.connected;
  let turnTimer;
  cover(
    (close) => {
      const area = h(
        'div',
        { class: 'padarea' },
        h(
          'div',
          { class: 'shoulders' },
          padKey('ZL', (on) => pad.trigger(0, on), 'sh'),
          padKey('L', (on) => pad.set(PAD.lb, on), 'sh'),
          h('div', { class: 'grow' }),
          padKey('R', (on) => pad.set(PAD.rb, on), 'sh'),
          padKey('ZR', (on) => pad.trigger(1, on), 'sh'),
        ),
        h(
          'div',
          { class: 'padmain' },
          h('div', { class: 'padleft' }, stick((x, y) => pad.stick(0, x, y), 'big'), dpad((d) => pad.dpad(d))),
          h(
            'div',
            { class: 'padmid' },
            status,
            h('div', { class: 'row gapbig' }, padKey('−', (on) => pad.set(PAD.back, on), 'round small'), padKey('+', (on) => pad.set(PAD.start, on), 'round small')),
            btn(tr('Exit'), () => close(), { icon: 'close' }),
          ),
          h('div', { class: 'padright' }, faceButtons((b, on) => pad.set(b, on)), stick((x, y) => pad.stick(1, x, y))),
        ),
      );
      const fit = () => area.classList.toggle('turned', innerHeight > innerWidth);
      fit();
      // the page turns once the phone does (or the Android app turns it)
      addEventListener('resize', fit);
      turnTimer = setInterval(fit, 500);
      area._unfit = () => removeEventListener('resize', fit);
      return area;
    },
    {
      cls: 'padcover',
      onClose: () => {
        clearInterval(turnTimer);
        pad.reset();
        sendTv({ t: 'leave' });
        landscape(false);
        keepAwake(false);
      },
    },
  );
  landscape(true);
  keepAwake(true);
  drawStatus();
  sendTv({ t: 'join' });
  const holder = document.querySelector('.padcover');
  on('player', drawStatus, holder);
  on('rumble', rumble, holder);
  // back from the background: the line was closed, so the TV dropped this pad
  on(
    'tv',
    () => {
      drawStatus();
      if (tv.connected && !wasConnected) sendTv({ t: 'join' });
      wasConnected = tv.connected;
    },
    holder,
  );
}
