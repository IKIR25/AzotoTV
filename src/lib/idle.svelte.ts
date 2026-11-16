// Idle levels, by time since the last controller or keyboard input:
//   2 min  `saver.still`: the home slideshow stops, so the screen stops changing
//   5 min  `saver.on`: the screensaver
//   +15    `saver.dark`: screens off (DPMS), and the TV too if it's showing AzotoTV
// Every redraw wakes the NVIDIA GPU to full clocks (~33 W instead of ~5 W) and
// it takes 10-35 s to come back down, so anything that changes the screen every
// few seconds keeps it at full power for as long as the TV sits idle. That kept
// the server hot and its fans loud. The input that ends the screensaver or the
// dark screen only wakes it up and is not acted on.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

const SAVER_MS = 5 * 60_000;

export const saver = $state({ still: false, on: false, dark: false });
let last = performance.now();

function setDark(on: boolean) {
  saver.dark = on;
  invoke('display_power', { on: !on }).catch(() => {});
}

/** "Spegni" in the START menu: screens and TV off now (the server stays on); any input wakes them. */
export function sleepNow() {
  saver.on = false;
  saver.still = true;
  setDark(true);
  invoke('tv_power', { on: false }).catch(() => {});
}

/** Any input: restarts the idle clock. True if it woke the screensaver (swallow the input). */
export function poke(): boolean {
  last = performance.now();
  saver.still = false;
  if (!saver.on && !saver.dark) return false;
  if (saver.dark) {
    setDark(false);
    // the TV back on and onto AzotoTV's input (tv.rs; nothing if it already is)
    invoke('tv_power', { on: true }).catch(() => {});
  }
  saver.on = false;
  return true;
}

/**
 * `busy()`: someone is watching or playing (video, game): the idle clock stays at 0.
 * `saverOk()`: the screensaver may show (a profile is in, not on the picker).
 * `darkOk()`: the screens may switch off (not while music plays: HDMI audio goes with them).
 */
export function startIdle(busy: () => boolean, saverOk: () => boolean, darkOk: () => boolean) {
  // screens on at start: a crash while dark would leave the TV's output off
  invoke('display_power', { on: true }).catch(() => {});
  let after = SAVER_MS;
  invoke<number | null>('dev_saver_secs')
    .then((s) => s && (after = s * 1000))
    .catch(() => {});
  const timer = setInterval(() => {
    const now = performance.now();
    if (busy()) {
      last = now;
      saver.still = false;
      return;
    }
    const idle = now - last;
    if (!saver.still && idle >= after * 0.4) saver.still = true;
    if (!saver.on && !saver.dark && saverOk() && idle >= after) saver.on = true;
    if (!saver.dark && darkOk() && idle >= after * 4) {
      saver.on = false;
      setDark(true);
      invoke('tv_power', { on: false, onlyOurs: true }).catch(() => {});
    }
  }, 1000);
  // keyboard: wake without acting (capture, ahead of the navigation handler)
  const onKey = (e: KeyboardEvent) => {
    if (poke()) {
      e.preventDefault();
      e.stopImmediatePropagation();
    }
  };
  window.addEventListener('keydown', onKey, true);
  // the TV came to AzotoTV's input (tv.rs: turned on there, or switched to it
  // with its remote): wake up as if it were input
  const unTv = listen('tv-on', () => {
    last = performance.now();
    saver.still = false;
    saver.on = false;
    if (saver.dark) setDark(false);
  });
  // a Moonlight client came (stream.rs): wake on the laptop panel, the TV stays
  // as it is (tv_power/display_power do nothing while streaming); it left: back
  // to screens off if the TV was off
  const unStream = listen<{ on: boolean; dark: boolean }>('stream', (e) => {
    last = performance.now();
    saver.still = false;
    saver.on = false;
    if (e.payload.on) {
      if (saver.dark) setDark(false);
    } else if (e.payload.dark) {
      saver.still = true;
      setDark(true);
    }
  });
  return () => {
    clearInterval(timer);
    window.removeEventListener('keydown', onKey, true);
    unTv.then((un) => un());
    unStream.then((un) => un());
    if (saver.dark) setDark(false);
  };
}
