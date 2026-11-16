// The Mii app's music: a Mii maker theme (bgm.rs plays it, looped), chosen
// per profile. Silent while other music or a video plays, and with the
// screens off.
import { invoke } from '@tauri-apps/api/core';

export const TRACKS = [
  { key: 'wiiu', name: 'Wii U' },
  { key: '3ds', name: '3DS' },
  { key: 'wii', name: 'Wii' },
  { key: 'off', name: 'Spenta' },
] as const;
export type Track = (typeof TRACKS)[number]['key'];

const store = (user: string) => `azoto.mii.music.${user}`;

export const miiMusic = $state<{ track: Track }>({ track: 'wiiu' });

export function loadTrack(user: string) {
  try {
    const t = localStorage.getItem(store(user));
    miiMusic.track = TRACKS.some((x) => x.key === t) ? (t as Track) : 'wiiu';
  } catch {
    miiMusic.track = 'wiiu';
  }
}

/** Wii U -> 3DS -> Wii -> off -> Wii U */
export function nextTrack(user: string) {
  const i = TRACKS.findIndex((x) => x.key === miiMusic.track);
  miiMusic.track = TRACKS[(i + 1) % TRACKS.length].key;
  try {
    localStorage.setItem(store(user), miiMusic.track);
  } catch {}
}

export const trackName = () => TRACKS.find((x) => x.key === miiMusic.track)?.name ?? '';

/** On (that track) or off; repeating the same call does nothing. */
let playing: string | null = null;
export function setMusic(track: Track | null) {
  const want = track === 'off' ? null : track;
  if (want === playing) return;
  playing = want;
  if (want) invoke('bgm_play', { track: want }).catch((e) => console.warn('[mii] music', e));
  else invoke('bgm_stop').catch(() => {});
}
