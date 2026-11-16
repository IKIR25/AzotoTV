// Live TV: channels + guide from Jellyfin (an M3U tuner fed by iptv-org), and
// watching a channel in the video player. While a channel plays, ▲ ▼ in the
// player ask for the next / previous one (`player-zap`), and the overlay's
// info follows the guide when a programme ends. A TG that starts while
// something else plays is offered in the player: A there sends `player-goto`.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { api, type Item } from './jf';
import { player } from './player.svelte';
import { stopMusic } from './music.svelte';
import { showToast } from './stage.svelte';
import { saver } from './idle.svelte';

export const live = $state({
  channels: [] as Item[],
  /** channel id -> its programmes from now on, in time order */
  guide: {} as Record<string, Item[]>,
  /** bumped every 30 s so "now" / progress bars move */
  clock: Date.now(),
});

/** Jellyfin dates carry 7 fractional digits, which not every Date parser takes. */
export const ms = (d?: string) => (d ? Date.parse(d.replace(/\.\d+Z$/, 'Z')) : NaN);

let loadedAt = 0;

/** Channels + the next 12 hours of guide (cached for 10 minutes). */
export async function loadLive(force = false) {
  if (!force && live.channels.length && Date.now() - loadedAt < 600_000) return;
  const channels = await api.channels();
  const g: Record<string, Item[]> = {};
  if (channels.length) {
    const progs = await api.guide(channels.map((c) => c.Id)).catch(() => ({ Items: [] as Item[] }));
    for (const p of progs.Items) (g[p.ChannelId ?? ''] ??= []).push(p);
  }
  live.channels = channels;
  live.guide = g;
  loadedAt = Date.now();
}

/** On air on `ch` right now, and what follows. */
export function nowNext(ch: Item, t = live.clock): { now?: Item; next: Item[] } {
  const list = (live.guide[ch.Id] ?? []).filter((p) => ms(p.EndDate) > t);
  const now = list[0] && ms(list[0].StartDate) <= t ? list[0] : undefined;
  return { now, next: now ? list.slice(1) : list };
}

export const hhmm = (d?: string) =>
  new Date(ms(d)).toLocaleTimeString('it-IT', { hour: '2-digit', minute: '2-digit' });

/** How far into the programme we are, 0..1. */
export function onAir(p: Item, t = live.clock) {
  const s = ms(p.StartDate);
  const e = ms(p.EndDate);
  return e > s ? Math.max(0, Math.min(1, (t - s) / (e - s))) : 0;
}

// ---- watching ----------------------------------------------------------------

let watching: Item | null = null;

/** What the player's overlay shows for `ch` (programme on air, slot, next). */
function osd(ch: Item) {
  const { now, next } = nowNext(ch, Date.now());
  const sub = [`${ch.ChannelNumber ?? ''} · ${ch.Name}`, now?.EpisodeTitle].filter(Boolean).join(' · ');
  return {
    title: now?.Name ?? ch.Name,
    subtitle: now ? sub : `${ch.ChannelNumber ?? ''} · In diretta`,
    extra: {
      live: true,
      start: now ? Math.floor(ms(now.StartDate) / 1000) : null,
      stop: now ? Math.floor(ms(now.EndDate) / 1000) : null,
      upnext: next[0] ? `${hhmm(next[0].StartDate)} · ${next[0].Name}` : '',
    },
  };
}

export async function watch(ch: Item) {
  if (player.active) return;
  player.active = true;
  await stopMusic();
  watching = ch;
  player.now = { id: ch.Id, name: ch.Name, live: true };
  const o = osd(ch);
  try {
    await invoke('player_play', {
      args: {
        item_id: ch.Id,
        media_type: 'Video',
        start_ticks: 0,
        title: o.title,
        subtitle: o.subtitle,
        live: true,
        extra: o.extra,
      },
    });
  } catch (e) {
    player.active = false;
    player.now = null;
    watching = null;
    showToast(`Canale non disponibile: ${e}`);
  }
}

/** A channel is playing: switch it to `ch`. */
async function switchTo(ch: Item) {
  watching = ch;
  player.now = { id: ch.Id, name: ch.Name, live: true };
  const o = osd(ch);
  await invoke('player_channel', { channelId: ch.Id, title: o.title, subtitle: o.subtitle, extra: o.extra }).catch(
    () => {},
  );
}

/** Call once at startup: channel up/down from the player, guide-following info. */
export function initLive() {
  const zap = listen<number>('player-zap', async ({ payload: dir }) => {
    if (!watching || !live.channels.length) return;
    const list = live.channels;
    const i = list.findIndex((c) => c.Id === watching!.Id);
    await switchTo(list[(i + dir + list.length) % list.length]);
  });
  const ended = listen('player-ended', () => (watching = null));
  // "Guarda il Tg1" in the player: switch channel, or leave the film / video for it
  const goto = listen<string>('player-goto', async ({ payload: id }) => {
    await loadLive().catch(() => {});
    const ch = live.channels.find((c) => c.Id === id);
    if (!ch) return showToast('Canale non disponibile');
    if (watching) return void (watching.Id !== ch.Id && (await switchTo(ch)));
    await invoke('player_stop').catch(() => {});
    for (let i = 0; i < 50 && player.active; i++) await new Promise((r) => setTimeout(r, 100));
    await watch(ch);
  });
  // every 30 s: move the clock; while a channel plays, refresh its overlay
  // info when the programme on air changes
  let shown = '';
  const timer = setInterval(() => {
    // progress bars stay put once idle: every redraw wakes the GPU (idle.svelte.ts)
    if (!saver.still) live.clock = Date.now();
    if (live.channels.length) void loadLive().catch(() => {}); // no-op until 10 min old
    if (!watching) return (shown = '');
    const o = osd(watching);
    const key = `${watching.Id}|${o.title}|${o.extra.start}`;
    if (key !== shown && shown) {
      invoke('player_cmd', { args: ['set_property', 'user-data/azoto/info', JSON.stringify({ ...o.extra, title: o.title, subtitle: o.subtitle })] }).catch(() => {});
    }
    shown = key;
  }, 30_000);
  return () => {
    clearInterval(timer);
    void zap.then((f) => f());
    void ended.then((f) => f());
    void goto.then((f) => f());
  };
}
