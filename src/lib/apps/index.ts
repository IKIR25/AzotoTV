// The apps on the AzotoTV home, in their default order. Each profile picks
// which ones it has (the Store, `installed.svelte.ts`) and can rearrange its
// tiles (hold A on one); both are kept on this device per Jellyfin user, like
// the rest of AzotoTV's per-profile data.
import { liveApp } from './live.svelte';
import { tvApp } from './tv.svelte';
import { musicaApp } from './musica.svelte';
import { giochiApp } from './giochi.svelte';
import { youtubeApp } from './youtube.svelte';
import { galleriaApp } from './galleria.svelte';
import { miiApp } from './mii.svelte';
import { storeApp } from './store.svelte';
import { installed } from './installed.svelte';
import type { App } from './types';
import { invoke } from '@tauri-apps/api/core';

export type { App, Slide } from './types';

export const APPS: App[] = [liveApp, tvApp, musicaApp, giochiApp, youtubeApp, galleriaApp, miiApp, storeApp];

const storeKey = (user: string) => `azoto.apps.${user}`;

/** The profile's tiles in its order: saved keys first (unknown or removed ones dropped), newly added apps at the end. */
export function appOrder(user: string): App[] {
  const have = installed(user);
  let saved: string[] = [];
  try {
    saved = JSON.parse(localStorage.getItem(storeKey(user)) ?? '[]');
  } catch {}
  const mine = APPS.filter((a) => have.includes(a.key));
  const byKey = new Map(mine.map((a) => [a.key, a]));
  const out = saved.flatMap((k) => byKey.get(k) ?? []);
  // added later: in the order they were added
  const rest = have.flatMap((k) => byKey.get(k) ?? []).filter((a) => !out.includes(a));
  return [...out, ...rest];
}

export function saveAppOrder(user: string, apps: App[]) {
  try {
    localStorage.setItem(storeKey(user), JSON.stringify(apps.map((a) => a.key)));
  } catch {}
  reportApps(user);
}

/** Tells the server the profile's apps in order: the iPhone app shows the same home (phone.rs `phone_apps`). */
export function reportApps(user: string) {
  if (!user) return;
  invoke('phone_apps', { user, apps: appOrder(user).map((a) => a.key) }).catch(() => {});
}

/** At startup: every profile this TV knows of (they're kept in this webview's storage). */
export function reportAllApps() {
  const users = new Set<string>();
  try {
    for (const k of Object.keys(localStorage)) {
      const m = /^azoto\.(?:myapps|apps)\.(.+)$/.exec(k);
      if (m) users.add(m[1]);
    }
  } catch {}
  users.forEach(reportApps);
}
