// Spatial navigation shared by keyboard and gamepad.
// Any element with `data-focusable` takes part; focus moves to the nearest
// candidate in the requested direction. While an element with `data-nav-trap`
// exists (e.g. the on-screen keyboard), navigation stays inside it and inside
// any `data-nav-peer` element (the results next to the docked search keyboard).
import { openKeyboard } from './osk.svelte';
import { uiSound } from './sounds.svelte';
import { input } from './input.svelte';

export type Dir = 'up' | 'down' | 'left' | 'right';

const SEL = '[data-focusable]';

const center = (r: DOMRect) => ({ x: r.left + r.width / 2, y: r.top + r.height / 2 });

/**
 * Bring `el` into view by scrolling only its own vertical `.scroller` and
 * horizontal `.row` (scrollIntoView would also scroll every ancestor).
 */
export function reveal(el: HTMLElement, smooth: boolean) {
  const behavior: ScrollBehavior = smooth ? 'smooth' : 'instant';
  if (el.hasAttribute('data-scroll-top')) {
    document.querySelectorAll<HTMLElement>('.scroller').forEach((s) => s.scrollTo({ top: 0, behavior }));
    return;
  }
  const r = el.getBoundingClientRect();
  const v = el.closest<HTMLElement>('.scroller');
  if (v) {
    const b = v.getBoundingClientRect();
    // `data-snap="shelf"`: align the whole row (title + cards) to the top of
    // the scroller instead of centering the card (Home: one visible row).
    const shelf = v.dataset.snap === 'shelf' ? el.closest<HTMLElement>('.shelf') : null;
    const top = shelf
      ? v.scrollTop + (shelf.getBoundingClientRect().top - b.top)
      : v.scrollTop + (r.top + r.height / 2) - (b.top + b.height / 2);
    v.scrollTo({ top: Math.max(0, top), behavior });
  }
  const h = el.closest<HTMLElement>('.row');
  if (h) {
    const b = h.getBoundingClientRect();
    const left = h.scrollLeft + (r.left + r.width / 2) - (b.left + b.width / 2);
    h.scrollTo({ left: Math.max(0, left), behavior });
  }
}

// Moves in quick succession (stick held, or fast presses) scroll instantly:
// restarting a smooth scroll on every step piles up and lags on big grids.
let lastMove = 0;
const FAST_MS = 180;

function focusEl(el: HTMLElement) {
  const now = performance.now();
  const fast = now - lastMove < FAST_MS;
  lastMove = now;
  el.focus({ preventScroll: true });
  reveal(el, !fast);
}

// ---- focus memory per route (so "back" lands where you were) --------------

const remembered = new Map<string, string>();

export function rememberFocus(path: string, el: EventTarget | null) {
  if (el instanceof HTMLElement && el.dataset.key && el.closest('[data-remember]'))
    remembered.set(path, el.dataset.key);
}

/** Focus the remembered element for `path`, else the first content focusable. */
export function restoreFocus(path: string, fallbackSelector = '[data-remember] [data-focusable]') {
  const key = remembered.get(path);
  let el: HTMLElement | null = null;
  if (key) el = document.querySelector<HTMLElement>(`[data-key="${CSS.escape(key)}"]`);
  // nothing on the page to land on (e.g. an empty library): the tab you are on,
  // not the first button of the bar, so LB / RB keep going from there
  el ??=
    document.querySelector<HTMLElement>(fallbackSelector) ??
    document.querySelector<HTMLElement>(`${SEL}[aria-current="page"]`) ??
    document.querySelector<HTMLElement>(SEL);
  if (el) {
    el.focus({ preventScroll: true });
    reveal(el, false);
  }
}

// "Back the way you came": after moving A -> B, the opposite direction from B
// returns to A (if it's still there), even when geometry would pick another
// element (e.g. Up from a wide button under two narrower ones). Each element
// remembers how it was entered, per axis, so a sideways detour (Right, Left)
// doesn't erase where Up leads.
const OPPOSITE: Record<Dir, Dir> = { up: 'down', down: 'up', left: 'right', right: 'left' };
type Entry = { from: HTMLElement; dir: Dir };
const entered = new WeakMap<HTMLElement, { v?: Entry; h?: Entry }>();
const axis = (d: Dir) => (d === 'up' || d === 'down' ? 'v' : 'h');

/** Where navigation may go: the trap and its peers, or the whole page. */
function navRoots(): HTMLElement[] {
  const trap = document.querySelector<HTMLElement>('[data-nav-trap]');
  if (!trap) return [document.body];
  // peers sit next to the on-screen keyboard only, never next to a menu
  if (!trap.classList.contains('osk')) return [trap];
  return [trap, ...document.querySelectorAll<HTMLElement>('[data-nav-peer]')];
}

/** `el` can be reached now (not behind an open keyboard / menu). */
export function reachable(el: HTMLElement): boolean {
  return navRoots().some((r) => r.contains(el));
}

// the trap's element last moved away from (the key you left the keyboard on)
let lastInTrap: HTMLElement | null = null;

/** From a peer (the results), back to where you left the trap (the keyboard). */
export function backToTrap(): boolean {
  const [trap, ...peers] = navRoots();
  const a = document.activeElement;
  if (!peers.length || !(a instanceof HTMLElement) || trap.contains(a)) return false;
  const el = lastInTrap && trap.contains(lastInTrap) ? lastInTrap : trap.querySelector<HTMLElement>(SEL);
  if (!el) return false;
  go(a, el, 'left');
  return true;
}

export function move(dir: Dir) {
  const before = document.activeElement;
  // The focused element may take the move itself by cancelling `navmove`
  // (e.g. the Home slideshow: left / right change the slide).
  if (before instanceof HTMLElement && !before.dispatchEvent(new CustomEvent('navmove', { detail: dir, cancelable: true }))) {
    uiSound('move');
    return;
  }
  const roots = navRoots();
  if (before instanceof HTMLElement) {
    if (roots.length > 1 && roots[0].contains(before)) lastInTrap = before;
    const e = entered.get(before)?.[axis(dir)];
    if (e && e.dir === OPPOSITE[dir] && e.from.isConnected && roots.some((r) => r.contains(e.from))) {
      const r = e.from.getBoundingClientRect();
      if (r.width > 0 && r.height > 0) return go(before, e.from, dir);
    }
  }
  moveSpatial(dir, roots);
  const after = document.activeElement;
  if (after instanceof HTMLElement && after !== before) {
    uiSound('move');
    if (before instanceof HTMLElement) note(before, after, dir);
  }
}

function note(from: HTMLElement, to: HTMLElement, dir: Dir) {
  entered.set(to, { ...entered.get(to), [axis(dir)]: { from, dir } });
}

function go(from: HTMLElement, to: HTMLElement, dir: Dir) {
  uiSound('move');
  focusEl(to);
  note(from, to, dir);
}

function moveSpatial(dir: Dir, roots: HTMLElement[]) {
  const items = roots.flatMap((root) => [...root.querySelectorAll<HTMLElement>(SEL)]).filter((e) => {
    const r = e.getBoundingClientRect();
    return r.width > 0 && r.height > 0;
  });
  if (!items.length) return;

  const active = document.activeElement;
  const home = active instanceof HTMLElement && active.matches(SEL) ? roots.find((r) => r.contains(active)) : undefined;
  const cur = home ? (active as HTMLElement) : null;
  if (!cur || !home) return focusEl(items[0]);

  const cr = cur.getBoundingClientRect();
  const c = center(cr);
  const horizontal = dir === 'left' || dir === 'right';

  type Cand = { el: HTMLElement; main: number; cross: number; h: number };
  const cands: Cand[] = [];
  // trap <-> peer (keyboard <-> results) only sideways, and from any height
  const across: Cand[] = [];
  for (const el of items) {
    if (el === cur) continue;
    const r = el.getBoundingClientRect();
    const p = center(r);
    const dx = p.x - c.x;
    const dy = p.y - c.y;
    const main = dir === 'right' ? dx : dir === 'left' ? -dx : dir === 'down' ? dy : -dy;
    // Up / down: how far the target is sideways from where you are (0 if it
    // spans it). By centres, a wide target (the search box) lost to the top
    // bar's buttons from any card that wasn't right under the box's middle.
    const cross = horizontal ? Math.abs(dy) : Math.max(0, r.left - c.x, c.x - r.right);
    if (main <= 1) continue;
    if (roots.length > 1 && !home.contains(el)) {
      if (horizontal) across.push({ el, main, cross, h: r.height });
      continue;
    }
    // Horizontal moves must stay on (roughly) the same row.
    if (horizontal && cross > Math.max(cr.height, r.height) * 0.6) continue;
    // Vertical moves must reach a genuinely different row: the focused card is
    // scaled/lifted, so same-row neighbours can look a few px "below" it.
    if (!horizontal && main < Math.min(cr.height, r.height) * 0.5) continue;
    cands.push({ el, main, cross, h: r.height });
  }

  let pool = cands;
  if (!horizontal && cands.length) {
    // Vertical: go to the NEAREST row (never skip one, even if its only card is
    // far to the side), then pick the card closest in x within that row.
    const nearest = Math.min(...cands.map((k) => k.main));
    pool = cands.filter((k) => k.main <= nearest + Math.max(cr.height, k.h) * 0.5);
  }
  let best: HTMLElement | null = null;
  let bestScore = Infinity;
  for (const k of pool) {
    const score = horizontal ? k.main + k.cross * 2.5 : k.cross * 4 + k.main;
    if (score < bestScore) {
      bestScore = score;
      best = k.el;
    }
  }
  if (!best && across.length) {
    // off the edge of the keyboard or the results: into the other one. Into
    // the keyboard = back to the key you left it on; into the results = the
    // one nearest in height.
    const back = lastInTrap && roots[0].contains(lastInTrap) && across.find((k) => k.el === lastInTrap);
    best = back ? back.el : across.reduce((a, b) => (b.cross * 2.5 + b.main < a.cross * 2.5 + a.main ? b : a)).el;
  }
  if (best) focusEl(best);
}

export function activate() {
  const a = document.activeElement;
  if (!(a instanceof HTMLElement && a.matches(SEL))) return;
  uiSound('select');
  if (a instanceof HTMLInputElement) openKeyboard(a);
  else a.click();
}

/** A held down: the focused element may take it (`navhold`, cancelable), else it's a plain A. */
export function hold() {
  const a = document.activeElement;
  if (a instanceof HTMLElement && !a.dispatchEvent(new CustomEvent('navhold', { cancelable: true }))) {
    uiSound('select');
    return;
  }
  activate();
}

export function focusFirst() {
  const first = document.querySelector<HTMLElement>(SEL);
  if (first) first.focus({ preventScroll: true });
}

// --- Keyboard ---------------------------------------------------------------

/** A text field the computer's keyboard types into for real (no on-screen keyboard). */
function textField(el: Element | null): el is HTMLInputElement | HTMLTextAreaElement {
  return (
    el instanceof HTMLTextAreaElement ||
    (el instanceof HTMLInputElement && !['button', 'checkbox', 'radio', 'range', 'submit'].includes(el.type))
  );
}

// Enter / Space held this long = A held (pin to the Home, move a tile...)
const HOLD_MS = 500;

/**
 * The computer's keyboard, as a controller: arrows move, Enter / Space = A
 * (held = A held), Esc / Backspace = B, Tab / Shift+Tab = RB / LB (tabs),
 * F1 = START, [ ] = LT / RT, the Menu key = A held, media keys like the
 * remote's, F11 = full screen. In a text field the keys type: Enter goes on
 * (submits the form, or moves down), Tab moves between fields, Up / Down
 * leave it, Esc goes back. `fullscreen` toggles the window's full screen.
 */
export function startKeyboard(press: (action: string) => void, fullscreen: () => void) {
  const ARROWS: Record<string, Dir> = { ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right' };
  const KEYS: Record<string, string> = {
    Escape: 'b',
    Backspace: 'b',
    F1: 'start',
    '[': 'lt',
    ']': 'rt',
    ContextMenu: 'a-hold',
    MediaPlayPause: 'play',
    MediaStop: 'stop',
    MediaTrackNext: 'next',
    MediaTrackPrevious: 'prev',
  };
  let held: { key: string; timer: ReturnType<typeof setTimeout> | null } | null = null;

  const down = (e: KeyboardEvent) => {
    if (e.ctrlKey || e.altKey || e.metaKey) return;
    input.desk = true;
    input.pointer = false; // the keyboard leads now: hovering doesn't move the focus
    if (e.key === 'F11') {
      e.preventDefault();
      if (!e.repeat) fullscreen();
      return;
    }
    const a = document.activeElement;
    if (textField(a)) {
      if (e.key === 'Escape') press('b');
      else if (e.key === 'ArrowUp' || e.key === 'ArrowDown') press(ARROWS[e.key]);
      else if (e.key === 'Tab') press(e.shiftKey ? 'up' : 'down');
      else if (e.key === 'Enter') {
        if (a.form && 'oskSubmit' in a.dataset) a.form.requestSubmit();
        else press('down');
      } else if (/^F\d+$/.test(e.key) || e.key.startsWith('Media')) press(KEYS[e.key] ?? '');
      else return; // letters, Backspace, [ ], Left / Right, Home / End: typing
      e.preventDefault();
      return;
    }
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault(); // no native click: A fires on release, or as a hold
      if (e.repeat || held) return;
      const key = e.key;
      held = {
        key,
        timer: setTimeout(() => {
          if (held?.key !== key) return;
          held.timer = null;
          press('a-hold');
        }, HOLD_MS),
      };
      return;
    }
    const action = ARROWS[e.key] ?? (e.key === 'Tab' ? (e.shiftKey ? 'lb' : 'rb') : KEYS[e.key]);
    if (!action) return;
    e.preventDefault();
    press(action);
  };
  const up = (e: KeyboardEvent) => {
    if (!held || e.key !== held.key) return;
    e.preventDefault();
    const short = held.timer !== null;
    if (held.timer) clearTimeout(held.timer);
    held = null;
    if (short) press('a');
  };
  window.addEventListener('keydown', down);
  window.addEventListener('keyup', up);
  return () => {
    window.removeEventListener('keydown', down);
    window.removeEventListener('keyup', up);
    if (held?.timer) clearTimeout(held.timer);
  };
}
