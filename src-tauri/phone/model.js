// The app's state shared by every screen: the profile's apps and games, the tabs
// and their page stacks.

import { Stack, emit } from './ui.js';
import { api, account, login, store } from './api.js';

/** An app on the home, as the TV calls it (src/lib/apps), with the phone's own Richieste. */
export const APPS = {
  live: { title: 'Live TV', icon: 'live', colors: ['#E8202E', '#7A1020'] },
  tv: { title: 'TV', icon: 'film', colors: ['#5B6CFF', '#2A2F7A'] },
  music: { title: 'Music', icon: 'music', colors: ['#FF4FA3', '#7A1F55'] },
  youtube: { title: 'YouTube', icon: 'youtube', colors: ['#FF3B30', '#6E1612'] },
  giochi: { title: 'Games', icon: 'pad', colors: ['#19D3B0', '#0B5E52'] },
  galleria: { title: 'Gallery', icon: 'gallery', colors: ['#FFB547', '#7A4A0B'] },
  richieste: { title: 'Requests', icon: 'requests', colors: ['#9B6BFF', '#3E2580'] },
};

export const model = {
  /** the active profile's apps, in its TV order (only those the phone opens) */
  apps: [],
  /** the Mobile games */
  games: [],
  tab: 'home',
  /** tab -> Stack */
  stacks: {},
};

export function setStacks(stacks) {
  model.stacks = stacks;
}

/** Opens a page on the current tab. */
export function push(page) {
  model.stacks[model.tab].push(page);
}

/** The profile's apps and the phone's other profiles (they may have been unpaired meanwhile). */
export async function loadHome() {
  const a = account();
  if (!a) throw new Error('signed out');
  const home = await api('/api/app/home', { query: { user: a.id } });
  if (JSON.stringify(home.users) !== JSON.stringify(login.users)) store({ ...login, users: home.users });
  const list = home.apps.filter((x) => x in APPS && x !== 'richieste');
  // Richieste: when the profile has a Seerr user
  try {
    const s = await api('/api/app/seerr/status', { query: { user: a.id } });
    if (s.enabled && s.user) list.push('richieste');
  } catch {}
  model.apps = list;
  try {
    model.games = await api('/api/app/games');
  } catch {}
  emit('home', model);
}
