// Notifications (notify.rs): films / episodes that just arrived, live TV
// reminders (also the TGs, set by themselves). Rust pops each new one up over whatever is on screen (a GTK
// overlay, also above films and games); the UI keeps the list for the START
// menu's "Notifiche" and the unread dot on the avatar.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { goto } from '$app/navigation';
import { showToast } from './stage.svelte';
import { live, loadLive, watch } from './live.svelte';
import { player } from './player.svelte';
import { game } from './games.svelte';

export interface Notification {
  id: number;
  /** unix ms */
  time: number;
  kind: 'movie' | 'episode' | 'reminder' | 'onair' | 'tv' | 'test';
  kicker: string;
  title: string;
  body: string;
  /** Jellyfin image path, for `jf://localhost<image>` */
  image: string | null;
  logo: boolean;
  item_id: string | null;
  channel_id: string | null;
  /** Jellyfin users it is meant for (who requested the film) */
  users: string[];
}

export interface Reminder {
  program_id: string;
  channel_id: string;
  channel: string;
  name: string;
  episode: string | null;
  /** unix ms */
  start: number;
  image: string | null;
  user: string;
}

interface Feed {
  items: Notification[];
  unread: number;
  reminders: Reminder[];
}

export const notes = $state({ items: [] as Notification[], unread: 0, reminders: [] as Reminder[] });

/** A TG has just started: the START menu offers it first for this long. */
const ONAIR_OFFER = 10 * 60_000;
export const onair = $state({ n: null as Notification | null, until: 0 });

/** The TG the START menu offers, if one started a moment ago. */
export const onAirNow = () => (onair.n && Date.now() < onair.until ? onair.n : null);

export async function loadNotes() {
  const f = await invoke<Feed>('notify_feed').catch(() => null);
  if (!f) return;
  notes.items = f.items;
  notes.unread = f.unread;
  notes.reminders = f.reminders;
}

/** The list was opened: nothing is unread for this profile any more. */
export async function markSeen() {
  notes.unread = 0;
  await invoke('notify_seen').catch(() => {});
}

export async function clearNotes() {
  await invoke('notify_clear').catch(() => {});
  notes.items = [];
  notes.unread = 0;
}

/** Whether a notification opens something (a film / series page, a channel, pairing the TV). */
export const opens = (n: Notification) => !!(n.item_id || n.channel_id) || n.kind === 'tv';

/** A on a notification: its film / series page, the channel, or pairing the TV. */
export async function openNote(n: Notification) {
  if (onair.n?.id === n.id) onair.n = null;
  if (n.kind === 'tv') return goto('/tvpair');
  if (n.item_id) return goto(`/item/${n.item_id}`);
  if (n.channel_id) {
    await loadLive().catch(() => {});
    const ch = live.channels.find((c) => c.Id === n.channel_id);
    if (ch) return watch(ch);
    showToast('Canale non disponibile');
  }
}

/** Live TV: "Ricordamelo" on `r.program_id`, or forget it. Returns whether it is set. */
export async function toggleReminder(r: Omit<Reminder, 'user'>) {
  const set = await invoke<boolean>('reminder_toggle', { reminder: { ...r, user: '' } });
  await loadNotes();
  showToast(set ? `Ti avviso un minuto prima di «${r.name}»` : 'Promemoria tolto');
  return set;
}

export const hasReminder = (programId: string) => notes.reminders.some((r) => r.program_id === programId);

/** "5 min fa", "ieri alle 21:30". */
export function ago(t: number, now = Date.now()) {
  const m = Math.round((now - t) / 60_000);
  if (m < 1) return 'adesso';
  if (m < 60) return `${m} min fa`;
  const d = new Date(t);
  const hm = d.toLocaleTimeString('it-IT', { hour: '2-digit', minute: '2-digit' });
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((day(new Date(now)) - day(d)) / 86_400_000);
  if (days === 0) return `oggi alle ${hm}`;
  if (days === 1) return `ieri alle ${hm}`;
  return `${d.toLocaleDateString('it-IT', { day: 'numeric', month: 'short' })} alle ${hm}`;
}

/** Call once at startup. */
export function initNotify() {
  void loadNotes();
  const un = listen<{ n: Notification; overlay: boolean; toast: boolean }>('notify', ({ payload }) => {
    void loadNotes();
    if (payload.n.kind === 'onair' && payload.toast) Object.assign(onair, { n: payload.n, until: Date.now() + ONAIR_OFFER });
    // no overlay on this desktop (X11, no layer-shell): say it in the UI, when it's on screen
    if (payload.toast && !payload.overlay && !player.active && !game.active)
      showToast(`${payload.n.kicker}: ${payload.n.title}`, 5000);
  });
  return () => void un.then((f) => f());
}

