// Profiles and the active session. Tokens live in Rust (`profiles.json`); JS
// only learns who is active. Every profile that signed in on this device stays
// signed in, so switching is instant (see jellyfin.rs).
import { goto } from '$app/navigation';
import { invoke } from '@tauri-apps/api/core';
import { api, setUser } from './jf';

interface Session {
  base: string;
  user_id: string;
  user_name: string;
}

export interface Profile {
  id: string;
  name: string;
  image_tag: string | null;
  has_password: boolean;
  signed_in: boolean;
  /** AzotoTV asks its PIN before opening it */
  pin: boolean;
}

export const auth = $state<{
  state: 'loading' | 'out' | 'in';
  id: string;
  name: string;
  error: string;
  /** on the picker after boot / standby: B doesn't go back into the last profile (its PIN) */
  locked: boolean;
}>({
  state: 'loading',
  id: '',
  name: '',
  error: '',
  locked: false,
});

/** AzotoTV's own settings of the active profile (profile.rs). */
export const prefs = $state({ pin: false, tg_alerts: true, auto_reminders: false });

export async function loadPrefs() {
  Object.assign(prefs, await invoke<typeof prefs>('profile_prefs').catch(() => ({ pin: false, tg_alerts: true, auto_reminders: false })));
}

/** A 4-digit PIN for the active profile, or none (null). */
export async function setPin(pin: string | null) {
  await invoke('profile_set_pin', { pin });
  prefs.pin = pin !== null;
}

export async function setTgAlerts(on: boolean) {
  await invoke('profile_set_tg_alerts', { on });
  prefs.tg_alerts = on;
}

/** Whether `pin` opens profile `id`; throws "Troppi tentativi…" after a few wrong ones. */
export const checkPin = (id: string, pin: string) => invoke<boolean>('profile_check_pin', { userId: id, pin });

function adopt(s: Session) {
  setUser(s.user_id);
  auth.id = s.user_id;
  auth.name = s.user_name;
  auth.state = 'in';
  auth.error = '';
  auth.locked = false;
  void loadPrefs();
}

/** Activate a session whose token may be stale: false (and forgotten) if the server refuses it. */
async function adoptChecked(s: Session) {
  setUser(s.user_id);
  try {
    await api.views();
  } catch (e) {
    // only a refused token is forgotten; an unreachable server is not the token's fault
    if (String(e).includes('unauthorized')) {
      await invoke('jf_sign_out', { userId: s.user_id });
      return false;
    }
  }
  adopt(s);
  return true;
}

/** Restore the active profile, else dev auto-login from env, else signed out. */
export async function initAuth() {
  const s = await invoke<Session | null>('jf_session').catch(() => null);
  // it starts on the picker: the last profile isn't open to B
  if (s && (await adoptChecked(s))) return void (auth.locked = true);
  try {
    const c = await invoke<[string, string, string] | null>('dev_credentials');
    if (c) return adopt(await invoke<Session>('jf_login', { base: c[0], user: c[1], pass: c[2] }));
  } catch (e) {
    auth.error = String(e);
  }
  auth.state = 'out';
}

export async function login(base: string, user: string, pass: string) {
  auth.error = '';
  try {
    adopt(await invoke<Session>('jf_login', { base, user, pass }));
    return true;
  } catch (e) {
    auth.error = String(e);
    return false;
  }
}

/** Switch to a profile that is already signed in on this device. */
export async function switchTo(id: string) {
  try {
    return await adoptChecked(await invoke<Session>('jf_switch', { userId: id }));
  } catch {
    return false;
  }
}

/**
 * Open a profile the way the picker does: straight in when it's signed in on
 * this device or has no password (true), else its password page (false). Its
 * PIN page comes first, if it has one (`unlocked`: it was just typed); a typed
 * password needs no PIN.
 */
export async function enterProfile(p: Profile, unlocked = false) {
  if (p.pin && !unlocked && (p.signed_in || !p.has_password)) {
    await goto(`/profiles/${p.id}/pin`);
    return false;
  }
  if (p.signed_in && (await switchTo(p.id))) {
    await goto('/', { replaceState: true });
    return true;
  }
  if (!p.has_password) {
    const base = await server();
    if (base && (await login(base, p.name, ''))) {
      await goto('/', { replaceState: true });
      return true;
    }
  }
  await goto(`/profiles/${p.id}`);
  return false;
}

/** Forget a profile on this device (revokes its token). */
export async function signOut(id: string) {
  await invoke('jf_sign_out', { userId: id });
  if (auth.id === id) {
    auth.state = 'out';
    auth.id = auth.name = '';
  }
}

/** `offline`: Jellyfin didn't answer, the list is the one saved last time. */
export const profiles = () => invoke<{ profiles: Profile[]; offline: boolean }>('jf_profiles');
export const server = () => invoke<string | null>('jf_server');
export const setServer = (base: string) => invoke<string>('jf_set_server', { base });

/** Profile picture (public, also works while nobody is signed in). */
export const userImage = (id: string, tag: string | null | undefined, px = 320) =>
  tag ? `jf://localhost/UserImage?userId=${id}&maxWidth=${px}&v=${tag}` : null;
