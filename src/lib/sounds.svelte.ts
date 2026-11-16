// Interface sounds (played by Rust, see src-tauri/src/sound.rs): a tick when
// the focus moves, a blip on A, another on B. On or off for the whole TV (START
// menu), not per profile: everyone in the room hears them.
import { invoke } from '@tauri-apps/api/core';

export type UiSound = 'move' | 'select' | 'back';

const KEY = 'azoto.sounds';

function saved(): boolean {
  try {
    return localStorage.getItem(KEY) !== '0';
  } catch {
    return true;
  }
}

export const sounds = $state({ on: saved() });

export function setSounds(on: boolean) {
  sounds.on = on;
  try {
    localStorage.setItem(KEY, on ? '1' : '0');
  } catch {}
}

export function uiSound(s: UiSound) {
  if (sounds.on) invoke('ui_sound', { name: s }).catch(() => {});
}
