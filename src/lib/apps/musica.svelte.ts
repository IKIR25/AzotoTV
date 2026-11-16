// Musica: the shelf shows the song playing (A opens the player), the whole
// library shuffled, the profile's playlists, the albums and the songs added
// lately (A plays them).
import { goto } from '$app/navigation';
import { api, img, type Item } from '../jf';
import { music, playTracks } from '../music.svelte';
import type { App, Slide } from './types';

const shelf = $state({
  view: null as Item | null,
  count: 0,
  playlists: [] as Item[],
  albums: [] as Item[],
  songs: [] as Item[],
});

const cover = (it: Item | undefined, rem: number) =>
  it?.ImageTags?.Primary ? img(it.Id, 'Primary', rem, it.ImageTags.Primary) : null;
const artist = (it: Item) => it.AlbumArtist ?? it.Artists?.[0] ?? '';

function nowPlaying(): Slide | null {
  const it = music.queue[music.index];
  if (!it) return null;
  const left = music.queue.length - music.index - 1;
  return {
    key: 'now',
    title: it.Name,
    cover: cover(it, 14),
    kicker: { text: music.active && !music.paused ? 'In riproduzione' : 'In pausa' },
    meta: [artist(it), it.Album ?? '', left > 0 ? `poi altri ${left} brani` : ''].filter(Boolean),
    progress: music.duration ? Math.min(1, music.shown / music.duration) : undefined,
    backdrop: cover(it, 40),
    ambient: true,
    action: 'Apri',
    open: () => goto('/music'),
  };
}

function listSlide(it: Item, kind: 'playlist' | 'album'): Slide {
  const meta =
    kind === 'playlist'
      ? [it.ChildCount ? `${it.ChildCount} brani` : 'Playlist']
      : [artist(it), it.ProductionYear ? String(it.ProductionYear) : ''].filter(Boolean);
  return {
    key: `${kind}:${it.Id}`,
    title: it.Name,
    cover: cover(it, 14),
    kicker: { text: kind === 'playlist' ? 'Le tue playlist' : 'Aggiunti di recente' },
    meta,
    backdrop: cover(it, 40),
    ambient: true,
    action: 'Riproduci',
    open: async () => {
      const songs =
        kind === 'playlist'
          ? (await api.playlistItems(it.Id)).Items
          : (await api.tracks(it.Id, 'ParentIndexNumber,IndexNumber,SortName')).Items;
      await playTracks(songs.filter((x) => x.Type === 'Audio'));
    },
  };
}

function shuffleSlide(view: Item): Slide {
  const pic = shelf.songs.find((s) => s.ImageTags?.Primary);
  return {
    key: 'shuffle',
    title: 'Tutta la musica in casuale',
    kicker: { text: view.Name },
    meta: shelf.count ? [`${shelf.count} brani`] : [],
    backdrop: cover(pic, 40),
    ambient: true,
    action: 'Riproduci',
    open: async () => playTracks((await api.tracks(view.Id)).Items, 0, { shuffle: true }),
  };
}

function songSlide(it: Item, i: number): Slide {
  return {
    key: `song:${it.Id}`,
    title: it.Name,
    cover: cover(it, 14),
    kicker: { text: 'Brani aggiunti di recente' },
    meta: [artist(it)].filter(Boolean),
    backdrop: cover(it, 40),
    ambient: true,
    action: 'Riproduci',
    // the recent songs play on from this one
    open: () => playTracks(shelf.songs, i),
  };
}

export const musicaApp: App = {
  key: 'music',
  name: 'Musica',
  description: 'La musica della libreria: playlist, album e brani.',
  route: '/musica',
  color: 'linear-gradient(135deg, #19d3b0 0%, #1fa2ff 60%, #5b6cff 100%)',
  icon: `<path d="M19 34V12l18-4v22" />
    <circle class="fill" cx="14.5" cy="34.5" r="4.5" />
    <circle class="fill" cx="32.5" cy="30.5" r="4.5" />`,
  slides: () => {
    const now = nowPlaying();
    return [
      ...(now ? [now] : []),
      ...(shelf.view && shelf.count ? [shuffleSlide(shelf.view)] : []),
      ...shelf.playlists.map((p) => listSlide(p, 'playlist')),
      ...shelf.albums.map((a) => listSlide(a, 'album')),
      ...shelf.songs.map(songSlide),
    ];
  },
  async load() {
    const view = await api.musicView();
    if (!view) {
      Object.assign(shelf, { view: null, count: 0, playlists: [], albums: [], songs: [] });
      return;
    }
    const [playlists, albums, recent] = await Promise.all([
      api.playlists(),
      api.latest(view.Id),
      api.items({ parentId: view.Id, types: 'Audio', sortBy: 'DateCreated', sortOrder: 'Descending', limit: 10 }),
    ]);
    shelf.view = view;
    shelf.count = recent.TotalRecordCount;
    shelf.playlists = playlists;
    shelf.albums = albums.filter((x) => x.Type === 'MusicAlbum').slice(0, 10);
    shelf.songs = recent.Items;
  },
};
