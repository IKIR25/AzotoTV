// Giochi: the shelf starts with the games this profile played last (A plays
// them straight away), then the rest in a random order, shuffled once per profile.
import { artUrl, byRecent, heroUrl, lastPlayed, launch, library, loadGames, playtime, type Game } from '../games.svelte';
import type { App, Slide } from './types';

const MAX_RECENT = 3;

/** shuffled game ids, kept per profile so a trip into an app and back shows the same order */
const shuffled = $state({ user: '', ids: [] as string[] });

function meta(g: Game): string[] {
  const m: string[] = [];
  if (g.year) m.push(g.year);
  if (g.genres.length) m.push(g.genres.slice(0, 2).join(', '));
  return m;
}

function slide(g: Game, recent: boolean): Slide {
  const p = g.played;
  const world = g.worlds[0] ?? null;
  return {
    key: `${recent ? 'r' : 'g'}:${g.id}`,
    title: g.name,
    art: artUrl(g, 'logo'),
    kicker: recent && p ? { text: `Continua a giocare · ${lastPlayed(p.last)}` } : undefined,
    meta: recent && p ? [`Giocato ${playtime(p.seconds)}`, ...meta(g)] : meta(g),
    overview: g.description,
    backdrop: heroUrl(g),
    // games with chapters carry on with the one played last, Minecraft in the last world
    action:
      p?.chapter != null && g.chapters[p.chapter]
        ? `Gioca · ${g.chapters[p.chapter].name}`
        : world
          ? `Gioca · ${world.name}`
          : 'Gioca',
    open: () => launch(g.id, p?.chapter ?? null, world),
  };
}

export const giochiApp: App = {
  key: 'giochi',
  name: 'Giochi',
  description: 'Giochi per PC, telefono, Switch, 3DS, Wii, N64, NES e sala giochi, con i salvataggi del tuo profilo.',
  route: '/giochi',
  color: 'linear-gradient(135deg, #b4ec51 0%, #3fcf6e 50%, #0f9b6c 100%)',
  icon: `<path d="M15 15h18a9 9 0 0 1 8.7 11.3l-2 7.7a5 5 0 0 1-8.6 2.1L27.5 32h-7l-3.6 4.1a5 5 0 0 1-8.6-2.1l-2-7.7A9 9 0 0 1 15 15z" />
    <path d="M14 21v6M11 24h6" />
    <circle class="fill" cx="31" cy="22" r="1.8" /><circle class="fill" cx="35" cy="26" r="1.8" />`,
  slides: () => {
    const recent = byRecent(library.games.filter((g) => g.played)).slice(0, MAX_RECENT);
    const rest = shuffled.ids.flatMap((id) => library.games.find((g) => g.id === id && !recent.includes(g)) ?? []);
    return [...recent.map((g) => slide(g, true)), ...rest.map((g) => slide(g, false))];
  },
  async load(user) {
    const games = await loadGames(user);
    if (shuffled.user !== user || shuffled.ids.length !== games.length) {
      const ids = games.map((g) => g.id);
      for (let i = ids.length - 1; i > 0; i--) {
        const j = Math.floor(Math.random() * (i + 1));
        [ids[i], ids[j]] = [ids[j], ids[i]];
      }
      shuffled.user = user;
      shuffled.ids = ids;
    }
  },
};
