// Which kind of input was used last: the controller, the TV remote (see
// src-tauri/src/remote.rs) or the computer's keyboard / mouse (`desk`, see
// mouse.ts), and which kind of controller (pad.rs `pad-style`). On-screen
// hints name the buttons of the one in use.
import { listen } from '@tauri-apps/api/event';
import { showToast } from './stage.svelte';
import { t } from '$lib/i18n';

export type PadStyle = 'xbox' | 'pad' | 'nintendo' | 'playstation';
export const input = $state<{ remote: boolean; style: PadStyle; desk: boolean; pointer: boolean }>({
  remote: false,
  style: 'pad',
  /** the keyboard or the mouse was used last: hints name keys, text is typed for real */
  desk: false,
  /** the mouse moved last: its pointer is shown and hovering focuses */
  pointer: false,
});

// in a window on a computer, the keyboard and mouse until a controller speaks up
import('@tauri-apps/api/core')
  .then(({ invoke }) => invoke<boolean>('tv_mode'))
  .then((tv) => {
    if (!tv) input.desk = true;
  })
  .catch(() => {});

listen<PadStyle>('pad-style', (e) => (input.style = e.payload)).catch(() => {});
// a controller's battery is low (controllers.rs), when the overlay toast isn't available
listen<string>('controller-battery', (e) => showToast(e.payload, 5000)).catch(() => {});

export type Btn = 'A' | 'B' | 'X' | 'Y' | 'LB' | 'RB' | 'LT' | 'RT' | 'START' | 'SELECT' | 'HOME';

const REMOTE: Partial<Record<Btn, string>> = { A: 'OK', B: t('Back'), LB: 'CH−', RB: 'CH+' };
// the keys that do the same (nav.ts startKeyboard)
const DESK: Partial<Record<Btn, string>> = { A: 'Enter', B: 'Esc', LB: '⇧Tab', RB: 'Tab', LT: '[', RT: ']', START: 'F1' };
const NAMES: Record<PadStyle, Partial<Record<Btn, string>>> = {
  pad: {},
  xbox: { SELECT: 'BACK', HOME: 'Xbox' },
  nintendo: { LB: 'L', RB: 'R', LT: 'ZL', RT: 'ZR', START: '+', SELECT: '−' },
  playstation: { A: '✕', B: '○', X: '□', Y: '△', LB: 'L1', RB: 'R1', LT: 'L2', RT: 'R2', START: 'OPTIONS', SELECT: 'CREATE', HOME: 'PS' },
};

/** A button's name as printed on the controller in use, or the remote's / keyboard's equivalent while that's in use. */
export function key(b: Btn): string {
  return (input.desk ? DESK[b] : input.remote ? REMOTE[b] : undefined) ?? NAMES[input.style][b] ?? b;
}
