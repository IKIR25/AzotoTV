// Which apps a profile has on its home: each profile picks its own in the
// Store ("installing" only shows a tile; every app is built into AzotoTV).
// Kept on this device per Jellyfin user, like the tile order.
import { APPS, reportApps } from './index';

/** what every profile starts with (plus the Store, which is always there) */
const FIRST = ['live'];
const storeKey = (user: string) => `azoto.myapps.${user}`;

/**
 * At startup: forget the first Store's choices (old profiles kept every app
 * there; now every profile starts fresh with Diretta TV + Store).
 */
export function initInstalled() {
  try {
    for (const k of Object.keys(localStorage)) {
      if (k.startsWith('azoto.installed.') || k === 'azoto.beforeStore') localStorage.removeItem(k);
    }
  } catch {}
}

// `version` makes the readers reactive; the cache itself is a plain variable
// (reading never writes state, which Svelte forbids inside templates).
const version = $state({ n: 0 });
let cache = { user: '', keys: [] as string[] };

function read(user: string): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(storeKey(user)) ?? 'null');
    if (Array.isArray(v)) return v.filter((k) => typeof k === 'string');
  } catch {}
  return FIRST;
}

/** The profile's apps (keys), the fixed ones included. */
export function installed(user: string): string[] {
  void version.n;
  if (cache.user !== user) cache = { user, keys: read(user) };
  const fixed = APPS.filter((a) => a.fixed && !cache.keys.includes(a.key)).map((a) => a.key);
  return [...cache.keys, ...fixed];
}

export const isInstalled = (user: string, key: string) => installed(user).includes(key);

/** Put an app on the profile's home, or take it off (its data and pins stay). */
export function setInstalled(user: string, key: string, on: boolean) {
  const keys = installed(user).filter((k) => k !== key);
  if (on) keys.push(key);
  cache = { user, keys };
  try {
    localStorage.setItem(storeKey(user), JSON.stringify(keys));
  } catch {}
  version.n++;
  reportApps(user);
}

/** Changes whenever a profile installs or removes something (for effects to depend on). */
export const installedVersion = () => version.n;
