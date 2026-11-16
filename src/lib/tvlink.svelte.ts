// The TV's pairing with AzotoTV (tv.rs): when the TV stops taking AzotoTV's
// commands (Spegni, turning it on), START shows "Collega la TV" (/tvpair).
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export const tvLink = $state({ unpaired: false });

/** Call once at startup. */
export function initTvLink() {
  invoke<boolean>('tv_unpaired')
    .then((u) => (tvLink.unpaired = u))
    .catch(() => {});
  const un = listen<boolean>('tv-unpaired', (e) => (tvLink.unpaired = e.payload));
  return () => void un.then((f) => f());
}
