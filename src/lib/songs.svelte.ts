// Musica's YouTube side: songs from YouTube Music to play straight away
// (streamed by the music player through yt-dlp) or to download into the
// library, alone or straight into a playlist. Downloads happen on the server
// (`songs.rs`), one at a time; everyone then has the song in Jellyfin.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { api, img, type Item } from './jf';
import { playTracks } from './music.svelte';
import { showToast } from './stage.svelte';
import { ytImage } from './youtube.svelte';

export interface Song {
  id: string;
  title: string;
  artist: string;
  album: string;
  /** seconds */
  length: number;
  cover: string | null;
}

type JobState = 'queued' | 'downloading' | 'adding' | 'done' | 'error';
interface Job {
  song: Song;
  user: string;
  playlists: [string, string][];
  state: JobState;
  error: string;
}
interface Status {
  enabled: boolean;
  jobs: Job[];
  /** YouTube id -> Jellyfin item id */
  library: Record<string, string>;
}

export const songs = $state<Status>({ enabled: false, jobs: [], library: {} });

/** Where a song stands: in the library, being downloaded, failed, or only on YouTube. */
export function songState(id: string): 'library' | 'queued' | 'downloading' | 'adding' | 'error' | null {
  if (songs.library[id]) return 'library';
  const j = songs.jobs.find((x) => x.song.id === id && x.state !== 'done');
  return j ? (j.state as Exclude<JobState, 'done'>) : null;
}

export const STATE_LABEL: Record<string, string> = {
  library: '✓ Nella libreria',
  queued: 'In coda',
  downloading: 'Scarico…',
  adding: 'Quasi pronto…',
  error: 'Non riuscito',
};

export const search = (query: string) => invoke<Song[]>('ytm_search', { query });

export async function loadSongs(fresh = false) {
  const s = await invoke<Status>('songs_status', { fresh }).catch(() => null);
  if (s) Object.assign(songs, s);
}

/** Call once at startup: follow the downloads, and say when one is ready (or failed). */
export function initSongs() {
  loadSongs();
  const un = listen<Status>('songs-changed', ({ payload: s }) => {
    for (const j of s.jobs) {
      const was = songs.jobs.find((x) => x.song.id === j.song.id)?.state;
      if (was && was !== j.state && j.state === 'done') showToast(`Scaricato: ${j.song.title}`);
      if (was && was !== j.state && j.state === 'error') showToast(`Non sono riuscito a scaricare ${j.song.title}`, 4000);
    }
    Object.assign(songs, s);
  });
  return () => void un.then((f) => f());
}

/** A YouTube song as a queue entry for the music player. */
export function songItem(s: Song): Item {
  return {
    Id: `yt:${s.id}`,
    Name: s.title,
    Type: 'Audio',
    MediaType: 'Audio',
    AlbumArtist: s.artist,
    Album: s.album,
    RunTimeTicks: s.length * 1e7,
    ytCover: s.cover,
  };
}

/** The cover of a queue entry, Jellyfin's or a YouTube song's. */
export function songCover(it: Item | null | undefined, rem: number): string | null {
  if (!it) return null;
  if (it.Id.startsWith('yt:')) return ytImage(it.ytCover);
  return it.ImageTags?.Primary ? img(it.Id, 'Primary', rem, it.ImageTags.Primary) : null;
}

/** Play a found song: the library's copy if there is one, else straight from YouTube. */
export async function playSong(s: Song) {
  const lib = songs.library[s.id];
  const it = lib ? await api.item(lib).catch(() => null) : null;
  await playTracks([it ?? songItem(s)], 0);
}

/** Download into the library (and into `playlists`, the current profile's, once it's there). */
export async function download(s: Song, playlists: string[] = []) {
  try {
    await invoke('songs_download', { song: s, playlists });
    if (songs.library[s.id]) showToast(playlists.length ? 'Aggiunto alla playlist' : 'È già nella libreria');
    else showToast(playlists.length ? `${s.title}: lo scarico e lo aggiungo alla playlist` : `${s.title}: lo scarico`);
  } catch (e) {
    showToast(String(e), 4000);
  }
}

/** Put a song (library item or YouTube song) into one of this profile's playlists. */
export async function addToPlaylist(playlist: string, what: Item | Song) {
  if ('title' in what) {
    // a YouTube song: in the library first (downloaded if needed), then in the playlist
    const lib = songs.library[what.id];
    if (lib) return addToPlaylist(playlist, { Id: lib } as Item);
    if (!songs.enabled) return showToast('Scaricare è possibile solo sul server', 4000);
    return download(what, [playlist]);
  }
  if (what.Id.startsWith('yt:')) {
    const s = { id: what.Id.slice(3), title: what.Name, artist: what.AlbumArtist ?? '', album: what.Album ?? '', length: (what.RunTimeTicks ?? 0) / 1e7, cover: what.ytCover ?? null };
    return addToPlaylist(playlist, s);
  }
  try {
    await api.addToPlaylist(playlist, [what.Id]);
    showToast('Aggiunto alla playlist');
  } catch (e) {
    showToast(`Non riuscito: ${e}`, 4000);
  }
}
