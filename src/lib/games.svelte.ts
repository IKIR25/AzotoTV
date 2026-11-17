// Games: the library lives on disk (Rust, `games.rs`); launching hands the
// screen and the pads to the game until it exits (`game-ended`).
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { goto } from '$app/navigation';
import { player } from './player.svelte';
import { refresh, showToast } from './stage.svelte';
import { locale, t } from '$lib/i18n';

export type Art = 'cover' | 'hero' | 'logo' | 'header';

export interface Chapter {
  name: string;
  subtitle: string | null;
  description: string | null;
  /** `chapterArt` has a picture */
  art: boolean;
}

/** A mod the current profile can switch on (applied when the game starts). */
export interface Mod {
  id: string;
  name: string;
  subtitle: string | null;
  description: string | null;
  /** `modArt` has a picture */
  art: boolean;
  /** on for this profile */
  on: boolean;
}

/** A Minecraft world of the current profile. */
export interface World {
  /** its folder: what `launch` takes */
  id: string;
  name: string;
  mode: 'survival' | 'creative' | 'adventure' | 'spectator';
  hardcore: boolean;
  /** last played, unix seconds */
  last: number;
  /** the world's own 64x64 picture (a data: URL) */
  icon: string | null;
}

export interface Game {
  id: string;
  name: string;
  type: 'native' | 'proton' | 'switch' | 'wii' | '3ds' | 'minecraft';
  /** "mobile" / "n64" / "nes" / "arcade": a port of a phone or console game, or an
   * arcade machine (MAME); otherwise `type` decides the platform */
  platform: 'mobile' | 'switch' | 'n64' | 'nes' | 'arcade' | null;
  steam_appid: number | null;
  description: string;
  genres: string[];
  developers: string[];
  year: string | null;
  /** trailer names; play one with `playTrailer` */
  trailers: string[];
  art: Art[];
  /** "xbox": the game sees an Xbox pad; "keys": keyboard/mouse driven by the pad */
  pad: 'xbox' | 'keys';
  /** a stick moves a mouse pointer (the cursor sensitivity applies) */
  cursor: boolean;
  /** this profile's history: unix seconds, total seconds played, the chapter played last */
  played: { last: number; seconds: number; chapter: number | null } | null;
  /** listed like a series' episodes (empty: the game is one piece) */
  chapters: Chapter[];
  /** Minecraft: this profile's worlds, most recently played first */
  worlds: World[];
  /** how many can play together on the TV; above 1, phones can join as controllers */
  players: number;
  /** games with Steam achievements (run with the Steam API emulator): [earned, total] for this profile */
  achievements: [number, number] | null;
  /** mods this profile can switch on and off (listed like the chapters) */
  mods: Mod[];
}

/** A Steam achievement, as this profile has it. */
export interface Achievement {
  name: string;
  description: string;
  /** secret: name and description stay hidden until earned */
  hidden: boolean;
  /** when it was earned, unix seconds */
  earned: number | null;
  /** index for `achievementIcon` */
  icon: number;
}

interface Ended {
  id: string;
  seconds: number;
  /** "exit" the game closed, "quit" Home held, "error" */
  reason: string;
  error: string | null;
}

/** `starting` while the game is prepared (first run of a Windows game makes its
 *  Wine prefix); `loading` until its window is on screen (`game-shown`). */
export const game = $state({ active: false, starting: false, loading: false, id: '', name: '' });

/** The library for the current profile (reloaded by `loadGames`). */
export const library = $state({ user: '', games: [] as Game[] });

export async function loadGames(user: string) {
  const games = await invoke<Game[]>('game_list');
  library.user = user;
  library.games = games;
  return games;
}

/** The console a game belongs to: the Giochi page has a tab for each, in this order. */
export const PLATFORMS = ['PC', 'Mobile', 'Switch', '3DS', 'Wii', 'N64', 'NES', 'Arcade'] as const;
export type Platform = (typeof PLATFORMS)[number];
const PORTS = { mobile: 'Mobile', switch: 'Switch', n64: 'N64', nes: 'NES', arcade: 'Arcade' } as const;
export const platform = (g: Game): Platform =>
  g.platform
    ? PORTS[g.platform]
    : g.type === 'switch'
      ? 'Switch'
      : g.type === '3ds'
        ? '3DS'
        : g.type === 'wii'
          ? 'Wii'
          : 'PC';

/** The Giochi page's open tab (kept while visiting a game's page and back). */
export const giochiTab = $state({ platform: 'PC' as Platform });

export const findGame = (id: string) => library.games.find((g) => g.id === id) ?? null;

/** Chapter `i` (from 0)'s picture. */
export const chapterArt = (g: Game, i: number) => (g.chapters[i]?.art ? `game://localhost/${g.id}/chapter-${i + 1}` : null);

/** A mod's picture. */
export const modArt = (g: Game, m: Mod) => (m.art ? `game://localhost/${g.id}/mod-${m.id}` : null);

/** Switch a mod on or off for the current profile (it applies at the next launch). */
export async function setMod(g: Game, m: Mod, on: boolean) {
  g.mods = await invoke<Mod[]>('game_set_mod', { id: g.id, modId: m.id, on });
}

export const achievementIcon = (g: Game, a: Achievement) => `game://localhost/${g.id}/ach-${a.icon}`;

/** Earned ones first (newest first), then the rest in the game's order. */
export const getAchievements = (id: string) => invoke<Achievement[]>('game_achievements', { id });

export const artUrl = (g: Game, kind: Art) => (g.art.includes(kind) ? `game://localhost/${g.id}/${kind}` : null);

/** Best wide picture for a backdrop: the hero art, else the store header. */
export const heroUrl = (g: Game) => artUrl(g, 'hero') ?? artUrl(g, 'header');

/** Most recently played first, then the never-played ones by name. */
export const byRecent = (gs: Game[]) =>
  [...gs].sort((a, b) => (b.played?.last ?? 0) - (a.played?.last ?? 0) || a.name.localeCompare(b.name));

export function playtime(s: number): string {
  const m = Math.round(s / 60);
  if (m < 60) return `${Math.max(1, m)} min`;
  const h = Math.floor(m / 60);
  return m % 60 ? `${h} h ${m % 60} min` : `${h} h`;
}

export function lastPlayed(unix: number): string {
  const d = new Date(unix * 1000);
  const days = Math.floor((Date.now() - d.getTime()) / 86_400_000);
  if (days < 1 && new Date().getDate() === d.getDate()) return 'oggi';
  if (days < 2) return 'ieri';
  if (days < 7) return t('{days} days ago', { days });
  return d.toLocaleDateString(locale, { day: 'numeric', month: 'long' });
}

const MODES: Record<World['mode'], string> = {
  survival: t('Survival'),
  creative: t('Creative'),
  adventure: t('Adventure'),
  spectator: t('Spectator'),
};
export const worldMode = (w: World) => (w.hardcore ? 'Hardcore' : MODES[w.mode]);

/** A game for more players waiting on the Controller screen (/controllers) to start. */
export const lobby = $state({ pending: null as { id: string; chapter: number | null; world: World | null } | null });

/** `chapter`: for games with chapters (default the first). `world`: Minecraft
 *  goes straight into it (none: the title screen, where new worlds are made).
 *  A game for more players first shows who is connected (the Controller
 *  screen), whose Gioca starts it with `ready`. */
export async function launch(id: string, chapter: number | null = null, world: World | null = null, ready = false) {
  if (game.active || player.active) return;
  const g = findGame(id);
  if (!ready && (g?.players ?? 1) > 1) {
    lobby.pending = { id, chapter, world };
    await goto('/controllers');
    return;
  }
  game.active = true;
  game.starting = true;
  game.loading = true;
  game.id = id;
  game.name = [g?.name, chapter !== null ? g?.chapters[chapter]?.name : null, world?.name].filter(Boolean).join(' · ');
  try {
    await invoke('game_launch', { id, chapter, world: world?.id ?? null });
  } catch (e) {
    game.active = false;
    game.loading = false;
    game.id = '';
    showToast(t('Couldn\'t start: {e}', { e }), 4000);
  } finally {
    game.starting = false;
  }
}

export async function playTrailer(g: Game, index = 0) {
  if (game.active || player.active) return;
  player.active = true;
  try {
    await invoke('game_trailer', { id: g.id, index, title: g.name });
  } catch (e) {
    player.active = false;
    showToast(t('Trailer not available: {e}', { e }));
  }
}

export interface Settings {
  cursor_sensitivity: number;
}
export const getSettings = () => invoke<Settings>('game_settings');
export const saveSettings = (settings: Settings) => invoke<Settings>('game_save_settings', { settings });

/** Call once at startup. */
export function initGames() {
  const shown = listen('game-shown', () => (game.loading = false));
  // a game the iPhone app started in its phone mode (phone.rs `/api/app/play`)
  const started = listen<{ id: string; name: string }>('game-started', (e) => {
    game.active = true;
    game.loading = true;
    game.id = e.payload.id;
    game.name = e.payload.name;
  });
  const un = listen<Ended>('game-ended', (e) => {
    game.active = false;
    game.loading = false;
    game.id = '';
    refresh.n++; // play history changed: shelves and pages reload
    if (e.payload.reason === 'error') showToast(t('The game closed with an error: {error}', { error: e.payload.error }));
  });
  return () => {
    void un.then((f) => f());
    void shown.then((f) => f());
    void started.then((f) => f());
  };
}
