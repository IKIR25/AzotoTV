// Which kind of input was used last: the controller or the TV remote (see
// src-tauri/src/remote.rs), and which kind of controller (pad.rs `pad-style`).
// On-screen hints name the buttons of the one in use.
import { listen } from '@tauri-apps/api/event';
import { showToast } from './stage.svelte';

export type PadStyle = 'xbox' | 'pad' | 'nintendo' | 'playstation';
export const input = $state<{ remote: boolean; style: PadStyle }>({ remote: false, style: 'pad' });

listen<PadStyle>('pad-style', (e) => (input.style = e.payload)).catch(() => {});
// a controller's battery is low (controllers.rs), when the overlay toast isn't available
listen<string>('controller-battery', (e) => showToast(e.payload, 5000)).catch(() => {});

export type Btn = 'A' | 'B' | 'X' | 'Y' | 'LB' | 'RB' | 'LT' | 'RT' | 'START' | 'SELECT' | 'HOME';

const REMOTE: Partial<Record<Btn, string>> = { A: 'OK', B: 'Indietro', LB: 'CH−', RB: 'CH+' };
const NAMES: Record<PadStyle, Partial<Record<Btn, string>>> = {
  pad: {},
  xbox: { SELECT: 'BACK', HOME: 'Xbox' },
  nintendo: { LB: 'L', RB: 'R', LT: 'ZL', RT: 'ZR', START: '+', SELECT: '−' },
  playstation: { A: '✕', B: '○', X: '□', Y: '△', LB: 'L1', RB: 'R1', LT: 'L2', RT: 'R2', START: 'OPTIONS', SELECT: 'CREATE', HOME: 'PS' },
};

/** A button's name as printed on the controller in use, or the remote's equivalent while it's in use. */
export function key(b: Btn): string {
  return (input.remote ? REMOTE[b] : undefined) ?? NAMES[input.style][b] ?? b;
}
