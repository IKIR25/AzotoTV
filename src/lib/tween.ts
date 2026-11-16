// Frame-driven animation, for what must NOT be a CSS transition.
// WebKitGTK runs opacity/transform transitions on its compositor thread, and when
// one ends the compositor can briefly show the pre-transition value again: on
// the TV the backdrop and the focused card visibly "bounced" back to the
// previous film for a few frames. Setting the value each frame from JS is a
// plain style update that is committed like any other, so there is no hand-off.

export const easeOut = (t: number) => 1 - (1 - t) ** 3;

type Lift = { v: number; stop?: () => void };

/** Animate `node`'s focus scale towards `to` (0 = rest, 1 = `scale`). */
function lift(node: HTMLElement, st: Lift, scale: number, to: number) {
  st.stop?.();
  st.stop = tween(st.v, to, 180, (x) => {
    st.v = x;
    node.style.transform = x ? `scale(${1 + (scale - 1) * x})` : '';
  });
}

/** Svelte action: grow the element a little while it has focus (`use:liftOnFocus={1.1}`). */
export function liftOnFocus(node: HTMLElement, scale = 1.08) {
  const st: Lift = { v: 0 };
  const up = () => lift(node, st, scale, 1);
  const down = () => lift(node, st, scale, 0);
  node.addEventListener('focus', up);
  node.addEventListener('blur', down);
  return {
    destroy() {
      st.stop?.();
      node.removeEventListener('focus', up);
      node.removeEventListener('blur', down);
    },
  };
}

/**
 * The same focus lift for whole classes of elements (`.btn`, top-bar buttons…)
 * without a `use:` on each one: the first matching selector sets the scale.
 */
export function startLift(rules: [selector: string, scale: number][]) {
  const state = new WeakMap<HTMLElement, Lift>();
  const on = (e: FocusEvent, to: number) => {
    const el = e.target;
    if (!(el instanceof HTMLElement)) return;
    const rule = rules.find(([sel]) => el.matches(sel));
    if (!rule) return;
    let st = state.get(el);
    if (!st) state.set(el, (st = { v: 0 }));
    lift(el, st, rule[1], to);
  };
  const focus = (e: FocusEvent) => on(e, 1);
  const blur = (e: FocusEvent) => on(e, 0);
  document.addEventListener('focusin', focus);
  document.addEventListener('focusout', blur);
  return () => {
    document.removeEventListener('focusin', focus);
    document.removeEventListener('focusout', blur);
  };
}

/**
 * Animate `from` -> `to` over `ms`, calling `step` every frame. Returns a cancel
 * function; the final `step(to)` always runs unless cancelled.
 */
export function tween(
  from: number,
  to: number,
  ms: number,
  step: (v: number) => void,
  done?: () => void,
  ease = easeOut,
): () => void {
  const t0 = performance.now();
  let raf = 0;
  const frame = (now: number) => {
    const t = Math.min(1, (now - t0) / ms);
    step(from + (to - from) * ease(t));
    if (t < 1) raf = requestAnimationFrame(frame);
    else done?.();
  };
  raf = requestAnimationFrame(frame);
  return () => cancelAnimationFrame(raf);
}
