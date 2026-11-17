// Which apps a profile has on its home: each profile picks its own in the
// Store ("installing" only shows a tile; every app is built into AzotoTV).
// Kept on this device per Jellyfin user, like the tile order.
import { APPS, reportApps } from './index';
import { api } from '../jf';
import { auth } from '../session.svelte';

/**
 * What every profile starts with (plus the Store, which is always there).
 * Music joins them when the server has a music library: see `read`.
 */
const FIRST = ['tv', 'live', 'giochi'];
const WITH_MUSIC = ['tv', 'live', 'music', 'giochi'];
const storeKey = (user: string) => `azoto.myapps.${user}`;

/** Profiles whose music library was already looked for in this run. */
const askedMusic = new Set<string>();

/**
 * At startup: forget the first Store's choices (old profiles kept every app
 * there; now every profile starts fresh with the FIRST apps + Store).
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

function saved(user: string): string[] | null {
  try {
    const v = JSON.parse(localStorage.getItem(storeKey(user)) ?? 'null');
    if (Array.isArray(v)) return v.filter((k) => typeof k === 'string');
  } catch {}
  return null;
}

function read(user: string): string[] {
  const v = saved(user);
  if (v) return v;
  // A profile that never chose its apps: once the server says it has a music
  // library (asked as the signed-in profile, which can see it), Music is among
  // them too, saved, so from then on the choice is the profile's own.
  if (user === auth.id && !askedMusic.has(user)) {
    askedMusic.add(user);
    api
      .musicView()
      .then((m) => {
        if (m && !saved(user)) write(user, WITH_MUSIC);
      })
      .catch(() => {});
  }
  return FIRST;
}

function write(user: string, keys: string[]) {
  cache = { user, keys };
  try {
    localStorage.setItem(storeKey(user), JSON.stringify(keys));
  } catch {}
  version.n++;
  reportApps(user);
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
  write(user, keys);
}

/** Changes whenever a profile installs or removes something (for effects to depend on). */
export const installedVersion = () => version.n;
