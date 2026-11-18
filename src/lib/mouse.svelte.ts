// The computer's mouse. It shows up when it moves (input.pointer) and hides
// when a controller, the remote or the keyboard is used, or after a few
// seconds still. Hovering focuses what's under it, the same highlight a
// controller gives; a click is A, a right click is A held (pin to the Home,
// move a tile: never opens anything), the side "back" button is B. Button
// hints (`<kbd data-btn="B">`) are clickable. Nothing here runs without a
// mouse moving, so the TV never sees a pointer.
import { input } from './input.svelte';
import { poke } from './idle.svelte';
import { reachable } from './nav';
import { uiSound } from './sounds.svelte';
import { modal } from './stage.svelte';

const STILL_MS = 3000;
const SEL = '[data-focusable]';

const BTN: Record<string, string> = { A: 'a', B: 'b', X: 'x', Y: 'y', LB: 'lb', RB: 'rb', LT: 'lt', RT: 'rt', START: 'start', SELECT: 'select' };

export const pointer = $state({ x: 0, y: 0 });

export function startMouse(press: (action: string) => void) {
  const root = document.documentElement;
  let still: ReturnType<typeof setTimeout> | null = null;
  let travel = 0;

  const stop = $effect.root(() => {
    $effect(() => {
      root.classList.toggle('pointer', input.pointer);
    });
  });

  const wake = () => {
    root.classList.remove('still');
    if (still) clearTimeout(still);
    still = setTimeout(() => root.classList.add('still'), STILL_MS);
  };

  const target = (e: Event) => {
    const el = (e.target as Element | null)?.closest?.<HTMLElement>(SEL);
    return el && reachable(el) ? el : null;
  };

  const move = (e: MouseEvent) => {
    pointer.x = e.clientX;
    pointer.y = e.clientY;
    // a page moving under a still mouse sends moves without movement
    if (!e.movementX && !e.movementY) return;
    if (!input.pointer) {
      travel += Math.abs(e.movementX) + Math.abs(e.movementY);
      if (travel < 6) return;
      travel = 0;
      input.pointer = true;
      input.desk = true;
      hover(e);
    }
    poke();
    wake();
  };

  // hovering focuses (no scrolling: the page stays where the mouse put it).
  // The focused element may keep the focus by cancelling `navhover` (detail:
  // the hovered element), e.g. a Home tile being moved follows the mouse.
  const hover = (e: Event) => {
    if (!input.pointer) return;
    const el = target(e);
    if (!el || el === document.activeElement) return;
    const a = document.activeElement;
    if (a instanceof HTMLElement && !a.dispatchEvent(new CustomEvent('navhover', { detail: el, cancelable: true }))) return;
    el.focus({ preventScroll: true });
  };

  // a press that woke the screensaver does nothing else
  let swallow = false;
  const down = (e: MouseEvent) => {
    input.desk = true;
    input.pointer = true;
    wake();
    swallow = poke();
    if (swallow || e.button === 3 || e.button === 4) {
      e.preventDefault();
      e.stopPropagation();
    }
    if (!swallow && e.button === 3) press('b');
  };
  const up = (e: MouseEvent) => {
    if (e.button === 3 || e.button === 4) e.preventDefault();
  };

  const click = (e: MouseEvent) => {
    if (swallow) {
      swallow = false;
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    const hint = (e.target as Element | null)?.closest?.<HTMLElement>('kbd[data-btn]');
    if (hint) {
      const a = BTN[hint.dataset.btn ?? ''];
      if (a) press(a);
      return;
    }
    // outside an open menu / sheet: closes it, like B (and reaches nothing behind it)
    const trap = document.querySelector<HTMLElement>('[data-nav-trap]');
    if (trap && e.target instanceof Node && !trap.contains(e.target)) {
      const el = (e.target as Element).closest?.<HTMLElement>(SEL);
      if (!el || !reachable(el)) {
        e.preventDefault();
        e.stopPropagation();
        if (modal.close) {
          uiSound('back');
          modal.close();
        }
        return;
      }
    }
    const el = (e.target as Element | null)?.closest?.<HTMLElement>(SEL);
    if (!el) return;
    if (!(el instanceof HTMLInputElement)) uiSound('select');
  };

  // right click: A held, on what's under the mouse
  const menu = (e: MouseEvent) => {
    e.preventDefault();
    if (swallow) return;
    const el = target(e);
    if (!el) return;
    el.focus({ preventScroll: true });
    if (!el.dispatchEvent(new CustomEvent('navhold', { cancelable: true }))) uiSound('select');
  };

  const wheel = () => {
    if (poke()) return;
    wake();
  };

  window.addEventListener('mousemove', move, { passive: true });
  document.addEventListener('mouseover', hover);
  window.addEventListener('mousedown', down, true);
  window.addEventListener('mouseup', up, true);
  window.addEventListener('click', click, true);
  window.addEventListener('contextmenu', menu);
  window.addEventListener('wheel', wheel, { passive: true });
  return () => {
    stop();
    if (still) clearTimeout(still);
    window.removeEventListener('mousemove', move);
    document.removeEventListener('mouseover', hover);
    window.removeEventListener('mousedown', down, true);
    window.removeEventListener('mouseup', up, true);
    window.removeEventListener('click', click, true);
    window.removeEventListener('contextmenu', menu);
    window.removeEventListener('wheel', wheel);
  };
}
