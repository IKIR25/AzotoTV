// On-screen keyboard (controller text entry). Activating a focused <input>
// opens it; it edits that input in place and fires `input` events, so
// `bind:value` (and anything reacting to it, like live search) follows along.
// The view is `Keyboard.svelte`, mounted once in the layout.
import { tick } from 'svelte';
import { backToTrap, move, reveal } from './nav';
import type { PadExtra, RemoteExtra } from './pad';
import { input } from './input.svelte';

export type Shift = 'off' | 'once' | 'lock';

export const osk = $state<{
  target: HTMLInputElement | null;
  value: string;
  caret: number;
  shift: Shift;
  mode: 'abc' | 'sym';
}>({ target: null, value: '', caret: 0, shift: 'off', mode: 'abc' });

export function openKeyboard(el: HTMLInputElement) {
  // the computer's keyboard / mouse in use: type into the field itself
  if (input.desk) {
    el.focus({ preventScroll: true });
    return;
  }
  osk.target = el;
  osk.value = el.value;
  osk.caret = el.value.length;
  osk.shift = 'off';
  osk.mode = 'abc';
}

/** Focus is on the page next to a docked keyboard (search results), not on a key. */
const onPage = () => {
  const a = document.activeElement;
  return a instanceof HTMLElement && a.matches('[data-focusable]') && !a.closest('[data-nav-trap]') ? a : null;
};

/**
 * Close and put focus back on the input. `next` ("Fatto"/START) submits the
 * form if the input has `data-osk-submit`, else moves on to what's below it.
 * Closed from a search result, focus stays on that result (the page may draw
 * it anew without the keyboard: same `data-key`).
 */
export async function closeKeyboard(next = false) {
  const el = osk.target;
  const stay = onPage()?.dataset.key;
  osk.target = null;
  if (!el) return;
  await tick(); // the keyboard (and its focused key) is gone now
  const res = stay && document.querySelector<HTMLElement>(`[data-key="${CSS.escape(stay)}"]`);
  if (res) {
    res.focus({ preventScroll: true });
    reveal(res, false);
    return;
  }
  el.focus({ preventScroll: true });
  reveal(el, false);
  if (!next) return;
  if (el.form && 'oskSubmit' in el.dataset) el.form.requestSubmit();
  else move('down');
}

function commit(value: string, caret: number) {
  osk.value = value;
  osk.caret = caret;
  const el = osk.target;
  if (!el) return;
  el.value = value;
  el.dispatchEvent(new Event('input', { bubbles: true }));
}

/** The whole text at once (typed on a phone, see phone.svelte.ts). */
export function setText(value: string) {
  commit(value, value.length);
}

export function type(ch: string) {
  // shift only affects letters, and only a letter uses up a one-shot shift
  if (osk.shift !== 'off' && ch.toUpperCase() !== ch.toLowerCase()) {
    ch = ch.toUpperCase();
    if (osk.shift === 'once') osk.shift = 'off';
  }
  const { value: v, caret: c } = osk;
  commit(v.slice(0, c) + ch + v.slice(c), c + ch.length);
}

/** Delete before the caret; on an empty field B closes the keyboard instead. */
export function backspace() {
  const { value: v, caret: c } = osk;
  if (!v) return closeKeyboard();
  if (c > 0) commit(v.slice(0, c - 1) + v.slice(c), c - 1);
}

export function moveCaret(d: number) {
  osk.caret = Math.max(0, Math.min(osk.value.length, osk.caret + d));
}

/** off -> once (next letter only) -> lock -> off */
export function cycleShift() {
  osk.shift = osk.shift === 'off' ? 'once' : osk.shift === 'once' ? 'lock' : 'off';
}

export function toggleMode() {
  osk.mode = osk.mode === 'abc' ? 'sym' : 'abc';
}

/**
 * Controller buttons while the keyboard is open (A and the D-pad go through nav).
 * Returns false for buttons it leaves to the app: LB/RB on an empty field (the
 * caret can't move, so they switch tabs as usual) and the TV remote's media keys.
 */
export function oskPad(a: PadExtra | RemoteExtra): boolean {
  // on the results next to the keyboard: B goes back to the keys, START closes it
  if (onPage()) {
    if (a === 'b') return backToTrap();
    if (a === 'start') closeKeyboard(true);
    return a === 'start';
  }
  if ((a === 'lb' || a === 'rb') && !osk.value) return false;
  if (a === 'b') backspace();
  else if (a === 'y') type(' ');
  else if (a === 'x') cycleShift();
  else if (a === 'lb') moveCaret(-1);
  else if (a === 'rb') moveCaret(1);
  else if (a === 'start') closeKeyboard(true);
  else if (a === 'select') toggleMode();
  else return false;
  return true;
}
