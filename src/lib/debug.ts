// Debug tracing, active only when the app runs with AZOTOTV_DEBUG=1.
import { invoke } from '@tauri-apps/api/core';

let on = false;
export const initDebug = () =>
  invoke<boolean>('debug_enabled')
    .then((v) => (on = v))
    .catch(() => {});

export const dlog = (line: string) => {
  if (on) void invoke('dev_log', { line }).catch(() => {});
};

/** Per-frame animation timeline after an action: lit focus rings + backdrop layer opacities. */
export function timeline(label: string, ms = 1400) {
  if (!on) return;
  const start = performance.now();
  const rows: string[] = [];
  let last = '';
  const frame = () => {
    const t = Math.round(performance.now() - start);
    const rings = [...document.querySelectorAll<HTMLElement>('.ring')]
      .map((r, i) => [i, Number(getComputedStyle(r).opacity)] as const)
      .filter(([, o]) => o > 0.02)
      .map(([i, o]) => `${i}@${o.toFixed(2)}`)
      .join(',');
    const layers = [...document.querySelectorAll<HTMLElement>('.bd .layer')]
      .map((l) => Number(getComputedStyle(l).opacity).toFixed(2))
      .join('/');
    const state = `ring[${rings}] bd[${layers}]`;
    if (state !== last) {
      rows.push(`${t}ms ${state}`);
      last = state;
    }
    if (t < ms) requestAnimationFrame(frame);
    else dlog(`timeline ${label}:\n    ${rows.join('\n    ')}`);
  };
  requestAnimationFrame(frame);
}
