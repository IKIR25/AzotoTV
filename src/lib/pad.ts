// Controller input arrives from Rust (see src-tauri/src/pad.rs) as `pad`
// events, so the D-pad works regardless of what the WebView's Gamepad API
// supports and regardless of window focus. The TV remote (remote.rs) sends
// the same actions as `remote` events, plus a few of its own.
import { listen } from '@tauri-apps/api/event';
import { input } from './input.svelte';
import { activate, hold, move } from './nav';

export type PadExtra = 'b' | 'x' | 'y' | 'start' | 'select' | 'lb' | 'rb' | 'lt' | 'rt';
export type RemoteExtra = 'home' | 'play' | 'stop' | 'ff' | 'rew' | 'next' | 'prev' | 'chup' | 'chdown';

/**
 * Navigation (D-pad / left stick / arrows) and A (OK) are handled here;
 * everything else goes to `onExtra`. While `blocked()` is true (mpv owns the
 * screen) events are ignored.
 */
let handler: ((action: string, remote: boolean) => void) | null = null;

/** An action from a phone (see phone.svelte.ts), handled like the TV remote's. */
export function inject(action: string) {
  handler?.(action, true);
}

export function startPad(onExtra: (a: PadExtra | RemoteExtra) => void, blocked: () => boolean) {
  const handle = (action: string, remote: boolean) => {
    input.remote = remote;
    if (blocked()) return;
    switch (action) {
      case 'up':
      case 'down':
      case 'left':
      case 'right':
        move(action);
        break;
      case 'a':
        activate();
        break;
      case 'a-hold':
        hold();
        break;
      default:
        onExtra(action as PadExtra | RemoteExtra);
    }
  };
  const pad = listen<string>('pad', (e) => handle(e.payload, false));
  const remote = listen<string>('remote', (e) => handle(e.payload, true));
  handler = handle;
  return () => {
    handler = null;
    void pad.then((f) => f());
    void remote.then((f) => f());
  };
}
