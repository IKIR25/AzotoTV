// Phones (see src-tauri/src/phone.rs): what they ask the TV to do, and what
// they're told about it (the keyboard is open, the screens are off, whose
// profile is on).
//
// A phone acts only while one of its profiles is on, or on the profile picker,
// where it may move around but only open its own profiles. Anything else puts
// the question "switch to one of this phone's profiles?" on screen (`ask`,
// PhoneAsk.svelte), which the phone itself can answer; a request that brought
// it up (a video, photos) goes ahead once the switch is done.
import { goto } from '$app/navigation';
import { page } from '$app/state';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { closeKeyboard, osk, setText } from './osk.svelte';
import { poke, saver, sleepNow } from './idle.svelte';
import { inject } from './pad';
import { play, player } from './player.svelte';
import { live, loadLive, watch as watchChannel } from './live.svelte';
import { api } from './jf';
import { stopMusic } from './music.svelte';
import { showToast } from './stage.svelte';
import { auth, enterProfile, profiles } from './session.svelte';
import { useProfile, watch, yt, type Video } from './youtube.svelte';
import { gallery, loadGallery, type Asset } from './gallery.svelte';
import { findGame, game, loadGames, platform } from './games.svelte';
import { reportAllApps } from './apps';

export interface Account {
  id: string;
  name: string;
}

/** The phone behind a request, and the profiles it's paired with. */
export interface Who {
  id: string;
  users: Account[];
}

/** Photos a phone asked to show right now: the layout opens them full screen. */
export const sent = $state<{ assets: Asset[] }>({ assets: [] });

/** "This phone is paired with ...: switch?" on screen; `then` runs after the switch. */
/** A page the phone can be the touch screen of, like a Switch game's (its
 * name; '' = none). Phones see it as the game on the TV with a screen. */
export const touchScreen = $state({ name: '' });
export const ask = $state<{ who: Who | null; then: (() => void) | null }>({ who: null, then: null });

interface Send {
  kind: 'youtube' | 'url';
  id?: string;
  url?: string;
  host?: string;
  from: string;
  phone: Who;
}

/** "Guarda sulla TV" from the iPhone app: a film / episode from where the phone was, or a channel. */
interface Watch {
  user: Account;
  item?: string;
  ticks?: number;
  audio?: number | null;
  sub?: number | null;
  channel?: string;
  from: string;
  phone: Who;
}

interface Show {
  album: string | null;
  assets: Asset[];
  from: string;
  phone: Who;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** The profile picker (also before anyone signs in): every paired phone may move around it. */
const onPicker = () => page.url.pathname === '/profiles' || (auth.state !== 'in' && !page.url.pathname.startsWith('/profiles/') && page.url.pathname !== '/login');

/** The profile a phone needs right now: the one on, or the one whose password page is open; '' otherwise. */
function tvUser(): string {
  const path = page.url.pathname;
  if (path.startsWith('/profiles/')) return path.split('/')[2] ?? '';
  if (path === '/profiles' || path === '/login' || auth.state !== 'in') return '';
  return auth.id;
}

const mine = (who: Who) => who.users.some((u) => u.id === tvUser());

export const names = (users: Account[]) =>
  users.length < 2 ? (users[0]?.name ?? '') : `${users.slice(0, -1).map((u) => u.name).join(', ')} e ${users.at(-1)!.name}`;

function openAsk(who: Who, then: (() => void) | null = null) {
  poke();
  ask.who = who;
  ask.then = then;
}

/** The answer: switch to `u` (then do what the phone asked), or stay (null). */
export async function answer(u: Account | null) {
  const then = ask.then;
  ask.who = null;
  ask.then = null;
  if (!u) return;
  const p = (await profiles().catch(() => ({ profiles: [] }))).profiles.find((x) => x.id === u.id);
  if (!p) return showToast(`Il profilo ${u.name} non c'è più su questa TV`);
  sent.assets = [];
  await stopPlayer();
  // straight in, or its password page (the phone can type it): then only once in
  if (await enterProfile(p)) then?.();
}

/** A press on a phone (only while no video plays: mpv's are checked in Rust). */
function onKey(a: string, who: Who) {
  if (ask.who?.id === who.id) return inject(a); // answering the question
  if (onPicker()) {
    // around the picker, yes; into a profile that isn't the phone's, no
    if (a === 'a' || a === 'a-hold') {
      const el = document.activeElement as HTMLElement | null;
      const id = el?.matches('.prof') ? el.dataset.key : undefined;
      if (!id || !who.users.some((u) => u.id === id)) return openAsk(who);
    }
    return inject(a);
  }
  if (mine(who)) return inject(a);
  // someone else's password page: back to the picker is fine
  if (a === 'b' && page.url.pathname.startsWith('/profiles/')) return inject(a);
  openAsk(who);
}

/** Whatever plays in the video player stops (a phone sent something else). */
async function stopPlayer() {
  if (!player.active) return;
  await invoke('player_stop').catch(() => {});
  for (let i = 0; i < 40 && player.active; i++) await sleep(100);
}

/** "Manda alla TV": YouTube in the YouTube app (resume point, history), anything else through yt-dlp. */
async function onSend(s: Send) {
  poke();
  sent.assets = [];
  await stopPlayer();
  if (s.kind === 'youtube' && s.id) {
    if (auth.state === 'in') useProfile(auth.id);
    let v: Video;
    try {
      v = (await yt.video(s.id)).video;
    } catch {
      v = { id: s.id, title: 'Video di YouTube', channel: '', channel_id: '', length: 0, views: '', published: '', ts: 0, short: false, live: false };
    }
    showToast(`${s.from || 'Un telefono'} ha mandato «${v.title}»`);
    await watch(v);
    return;
  }
  player.active = true;
  try {
    await invoke('phone_play', {
      url: s.url,
      title: 'Video dal telefono',
      subtitle: [s.host, s.from && `mandato da ${s.from}`].filter(Boolean).join(' · '),
    });
  } catch (e) {
    player.active = false;
    showToast(`Riproduzione non riuscita: ${e}`);
  }
}

async function onWatch(w: Watch) {
  poke();
  sent.assets = [];
  await stopPlayer();
  if (w.channel) {
    await loadLive().catch(() => {});
    const ch = live.channels.find((c) => c.Id === w.channel);
    if (!ch) return showToast('Canale non disponibile');
    showToast(`${w.from || 'Un telefono'}: ${ch.Name} sulla TV`);
    return watchChannel(ch);
  }
  if (!w.item) return;
  try {
    const it = await api.item(w.item);
    showToast(`${w.from || 'Un telefono'}: «${it.Type === 'Episode' ? (it.SeriesName ?? it.Name) : it.Name}» sulla TV`);
    const tracks = w.audio != null || w.sub != null ? { audio: w.audio ?? null, sub: w.sub ?? -1 } : undefined;
    await play(it, { startTicks: w.ticks ?? undefined, fromStart: !w.ticks, tracks });
  } catch (e) {
    showToast(`Riproduzione non riuscita: ${e}`);
  }
}

/** The phone's profile on the TV first: straight in when nobody is watching (the
 *  phone opened it with its PIN), otherwise the usual question. */
async function forPhone(user: Account, phone: Who, then: () => void) {
  if (tvUser() === user.id) return then();
  if (onPicker() || saver.dark) {
    poke();
    const p = (await profiles().catch(() => ({ profiles: [] }))).profiles.find((x) => x.id === user.id);
    if (!p) return showToast(`Il profilo ${user.name} non c'è più su questa TV`);
    sent.assets = [];
    await stopPlayer();
    if (await enterProfile(p, true)) then();
    return;
  }
  openAsk({ id: phone.id, users: [user] }, then);
}

/** "Mostra sulla TV": the photos just sent (full screen), or a whole album's slideshow. */
async function onShow(s: Show) {
  poke();
  await stopPlayer();
  if (s.assets.length) {
    sent.assets = s.assets;
    return;
  }
  if (!s.album) return;
  sent.assets = [];
  await goto(`/galleria/album/${s.album}?show=1`);
}

/** The power button: like START › Spegni (back to "who's watching", screens and TV off), or back on. */
async function onPower(who: Who) {
  if (saver.dark) return void poke(); // waking it up: any paired phone
  if (!mine(who) && !onPicker()) return openAsk(who);
  sent.assets = [];
  await stopPlayer();
  await stopMusic();
  if (auth.state === 'in') await goto('/profiles');
  auth.locked = true;
  sleepNow();
}

/** Call once at startup. */
export function initPhone() {
  const uns = [
    listen<{ a: string; phone: Who }>('phone-key', (e) => onKey(e.payload.a, e.payload.phone)),
    listen<{ value: string; enter: boolean; phone: Who }>('phone-text', (e) => {
      if (!osk.target || !mine(e.payload.phone)) return;
      poke();
      setText(e.payload.value);
      if (e.payload.enter) void closeKeyboard(true);
    }),
    listen<{ phone: Who }>('phone-power', (e) => void onPower(e.payload.phone)),
    listen<Send>('phone-send', (e) => {
      const s = e.payload;
      if (mine(s.phone)) void onSend(s);
      else openAsk(s.phone, () => void onSend(s));
    }),
    listen<Watch>('phone-watch', (e) => void forPhone(e.payload.user, e.payload.phone, () => void onWatch(e.payload))),
    listen<Show>('phone-show', (e) => {
      const s = e.payload;
      if (mine(s.phone)) void onShow(s);
      else openAsk(s.phone, () => void onShow(s));
    }),
    listen<{ album: string; n: number; from: string }>('phone-added', (e) => {
      const { album, n, from } = e.payload;
      showToast(`${from || 'Un telefono'} ha aggiunto ${n} foto a «${album}»`, 4000);
      if (gallery.loaded) void loadGallery(true).catch(() => {});
    }),
    // over a video, when the overlay can't show (phone.rs `tell`)
    listen<Who>('phone-denied', (e) => showToast(`Questo telefono è collegato a ${names(e.payload.users)}`)),
  ];
  // the iPhone app's homes
  reportAllApps();
  // what phones are told: the keyboard is open (never what a password field
  // holds), the screens are off, whose profile is on
  const stop = $effect.root(() => {
    $effect(() => {
      const el = osk.target;
      const secret = el?.type === 'password';
      const text = el
        ? { value: secret ? '' : osk.value, label: el.placeholder || el.getAttribute('aria-label') || (secret ? 'Password' : ''), secret }
        : null;
      invoke('phone_state', { state: { text } }).catch(() => {});
    });
    $effect(() => {
      invoke('phone_state', { state: { dark: saver.dark } }).catch(() => {});
    });
    // what plays (the app's "Continua qui")
    $effect(() => {
      const now = player.now ? { ...player.now } : null;
      invoke('phone_state', { state: { playing: now } }).catch(() => {});
    });
    // a game for more players: every paired phone may join it as a controller (any profile)
    $effect(() => {
      const g = game.active ? findGame(game.id) : null;
      // started without the Giochi page ever opening (e.g. from a pin): the library isn't loaded yet
      if (game.active && !g && auth.state === 'in') void loadGames(auth.id).catch(() => {});
      // screen: a Switch game, the phone can be its touch screen (phonescreen.rs);
      // console: a 3DS game, the phone shows its bottom screen with the 3DS's controls
      const screen = !!g && (platform(g) === 'Switch' || platform(g) === '3DS');
      const console = g && platform(g) === '3DS' ? '3ds' : null;
      // or a page made for touch (the Mii editor), shown like a game's screen
      const page = touchScreen.name ? { name: touchScreen.name, players: 1, screen: true } : null;
      invoke('phone_state', {
        state: { game: game.active ? { name: game.name || g?.name || '', players: g?.players ?? 1, screen, console } : page }
      }).catch(() => {});
    });
    $effect(() => {
      const user = tvUser();
      invoke('phone_state', { state: { user, user_name: user && user === auth.id ? auth.name : '', picker: onPicker() } }).catch(() => {});
    });
  });
  return () => {
    stop();
    uns.forEach((u) => void u.then((f) => f()));
  };
}
