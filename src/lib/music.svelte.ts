// Music: queue + transport for the background player (`music.rs`, a headless
// mpv). The UI keeps the screen and the controller while songs play; the
// "In riproduzione" page (/music) and the top-bar mini player show this state.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { goto } from '$app/navigation';
import type { Item } from './jf';
import { showToast } from './stage.svelte';
import { saver } from './idle.svelte';

export type Repeat = 'off' | 'all' | 'one';

export const music = $state({
  /** mpv is playing (or paused on) the queue */
  active: false,
  /** play order (shuffled when `shuffle`) */
  queue: [] as Item[],
  index: 0,
  time: 0,
  /** the position on screen: stays put once idle (a redraw every second kept
   *  the GPU at full power, see idle.svelte.ts); `time` is always exact */
  shown: 0,
  duration: 0,
  paused: false,
  shuffle: false,
  repeat: 'off' as Repeat,
});

/** The queue as it was given, to go back to when shuffle is turned off. */
let original: Item[] = [];

interface State {
  active: boolean;
  item_id: string | null;
  index: number;
  time: number;
  duration: number;
  paused: boolean;
}

function shuffled<T>(a: T[]): T[] {
  const b = [...a];
  for (let i = b.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [b[i], b[j]] = [b[j], b[i]];
  }
  return b;
}

const cmd = (...args: (string | number | boolean)[]) => invoke('music_cmd', { args }).catch(() => {});

function applyRepeat() {
  cmd('set_property', 'loop-playlist', music.repeat === 'all' ? 'inf' : 'no');
  cmd('set_property', 'loop-file', music.repeat === 'one' ? 'inf' : 'no');
}

/**
 * Play `items` starting at `start` (with `shuffle`: in random order, `start`
 * first if given) and open the player screen unless `open: false`.
 */
export async function playTracks(items: Item[], start = 0, o: { shuffle?: boolean; open?: boolean } = {}) {
  if (!items.length) return;
  original = items;
  let queue = items;
  let index = Math.min(start, items.length - 1);
  if (o.shuffle) {
    const first = start > 0 ? [items[start]] : [];
    queue = [...first, ...shuffled(items.filter((_, i) => i !== start || !first.length))];
    index = 0;
  }
  music.queue = queue;
  music.index = index;
  music.shuffle = !!o.shuffle;
  music.time = music.shown = 0;
  music.duration = (queue[index].RunTimeTicks ?? 0) / 1e7;
  music.paused = false;
  try {
    await invoke('music_play', { ids: queue.map((x) => x.Id), index });
    music.active = true;
    applyRepeat();
  } catch (e) {
    showToast(`Riproduzione non riuscita: ${e}`);
    return;
  }
  if (o.open !== false) goto('/music');
}

export function togglePause() {
  // the queue finished (or a video stopped it): start it again where it was
  if (!music.active) return void playTracks(music.queue, music.index, { open: false });
  cmd('cycle', 'pause');
}

export function next() {
  if (music.index < music.queue.length - 1) cmd('playlist-next');
  else if (music.repeat === 'all') cmd('playlist-play-index', 0);
}

/** Back to the start of the song, or (in its first seconds) to the previous one. */
export function prev() {
  if (music.time > 3 || music.index === 0) cmd('seek', 0, 'absolute');
  else cmd('playlist-prev');
}

export function jump(i: number) {
  if (!music.active) return void playTracks(music.queue, i, { open: false });
  music.index = i;
  cmd('playlist-play-index', i);
}

export const seekBy = (s: number) => music.active && cmd('seek', s, 'relative');

export function cycleRepeat() {
  music.repeat = music.repeat === 'off' ? 'all' : music.repeat === 'all' ? 'one' : 'off';
  if (music.active) applyRepeat();
}

/** Shuffle what's left of the queue, or go back to the original order; the song playing goes on. */
export async function toggleShuffle() {
  const cur = music.queue[music.index];
  if (!cur) return;
  let queue: Item[];
  let index: number;
  if (!music.shuffle) {
    const done = music.queue.slice(0, music.index + 1);
    const used = new Set(done.map((x) => x.Id));
    queue = [...done, ...shuffled(original.filter((x) => !used.has(x.Id)))];
    index = music.index;
  } else {
    queue = original;
    index = Math.max(0, original.findIndex((x) => x.Id === cur.Id));
  }
  music.shuffle = !music.shuffle;
  music.queue = queue;
  music.index = index;
  if (music.active) await invoke('music_requeue', { ids: queue.map((x) => x.Id), index }).catch(() => {});
}

/** End playback and forget the queue (the mini player goes away). */
export async function stopMusic() {
  music.queue = [];
  music.index = 0;
  music.active = false;
  await invoke('music_stop').catch(() => {});
}

/** Call once at startup. */
export function initMusic() {
  const un = listen<State>('music-state', ({ payload: s }) => {
    music.active = s.active;
    if (!s.active) return;
    // mpv's position is the truth (a song can be in the queue twice); the id
    // guards against a moment where mpv's playlist is being rebuilt
    const was = music.index;
    if (s.item_id) {
      const q = music.queue;
      if (q[s.index]?.Id === s.item_id) music.index = s.index;
      else if (q[music.index]?.Id !== s.item_id) {
        const i = q.findIndex((x) => x.Id === s.item_id);
        if (i >= 0) music.index = i;
      }
    }
    music.time = s.time;
    if (!saver.still || music.index !== was) music.shown = s.time;
    music.duration = s.duration || (music.queue[music.index]?.RunTimeTicks ?? 0) / 1e7;
    music.paused = s.paused;
  });
  return () => void un.then((f) => f());
}

export function clock(s: number): string {
  if (!isFinite(s) || s < 0) s = 0;
  const m = Math.floor(s / 60);
  const h = Math.floor(m / 60);
  const ss = String(Math.floor(s % 60)).padStart(2, '0');
  return h ? `${h}:${String(m % 60).padStart(2, '0')}:${ss}` : `${m}:${ss}`;
}
