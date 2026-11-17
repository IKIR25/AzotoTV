// Typed Jellyfin client. All traffic goes through Rust (`jf_request`), images
// through the `jf://` protocol; the access token never reaches JS.
import { invoke } from '@tauri-apps/api/core';

export interface UserData {
  PlaybackPositionTicks?: number;
  PlayedPercentage?: number;
  Played?: boolean;
  IsFavorite?: boolean;
  UnplayedItemCount?: number;
}
export interface Person {
  Id: string;
  Name: string;
  Role?: string;
  Type: string;
  PrimaryImageTag?: string;
}
export interface MediaStream {
  Index: number;
  Type: string; // Video | Audio | Subtitle | Attachment
  Language?: string;
  Codec?: string;
  Title?: string;
  DisplayTitle?: string;
  ChannelLayout?: string;
  Channels?: number;
  IsExternal?: boolean;
  IsForced?: boolean;
  IsDefault?: boolean;
  IsHearingImpaired?: boolean;
}
export interface MediaSource {
  Id: string;
  DefaultAudioStreamIndex?: number;
  DefaultSubtitleStreamIndex?: number;
}
export interface Item {
  Id: string;
  Name: string;
  Type: string; // Movie | Series | Season | Episode | BoxSet | Audio | Folder ...
  CollectionType?: string;
  MediaType?: string; // Video | Audio
  ProductionYear?: number;
  Overview?: string;
  Genres?: string[];
  CommunityRating?: number;
  OfficialRating?: string;
  RunTimeTicks?: number;
  ChildCount?: number;
  ImageTags?: Record<string, string>;
  BackdropImageTags?: string[];
  ParentBackdropItemId?: string;
  ParentBackdropImageTags?: string[];
  SeriesId?: string;
  SeriesName?: string;
  SeriesPrimaryImageTag?: string;
  /** songs: their album, whose cover stands in when the song has none */
  AlbumId?: string;
  AlbumPrimaryImageTag?: string;
  /** an episode's series logo (when Logo is in EnableImageTypes) */
  ParentLogoItemId?: string;
  ParentLogoImageTag?: string;
  SeasonId?: string;
  ParentIndexNumber?: number;
  IndexNumber?: number;
  AlbumArtist?: string;
  Album?: string;
  Artists?: string[];
  People?: Person[];
  UserData?: UserData;
  /** live TV: channel number; programmes: their slot and channel */
  ChannelNumber?: string;
  ChannelId?: string;
  StartDate?: string;
  EndDate?: string;
  EpisodeTitle?: string;
  CurrentProgram?: Item;
  /** a song's entry in a playlist (to take it out again) */
  PlaylistItemId?: string;
  /** AzotoTV's own: a YouTube Music song (`Id` = "yt:<video id>"), its album art */
  ytCover?: string | null;
  /** only on single-item fetches (`api.item`) */
  MediaStreams?: MediaStream[];
  MediaSources?: MediaSource[];
}
export interface Page {
  Items: Item[];
  TotalRecordCount: number;
}

let userId = '';
export const setUser = (id: string) => {
  userId = id;
  cache.clear();
};

type Q = Record<string, string | number | boolean | undefined>;
const toQuery = (o: Q): [string, string][] =>
  Object.entries(o)
    .filter(([, v]) => v !== undefined)
    .map(([k, v]) => [k, String(v)]);

// Tiny GET cache so going "back" is instant; cleared after anything that
// changes server state (playback, watched, favourite).
const cache = new Map<string, { t: number; v: unknown }>();
export const invalidate = () => cache.clear();

/** Called when the server refuses the active token (password changed, revoked…); set by the layout. */
export const onUnauthorized: { fn: (() => void) | null } = { fn: null };

async function req<T>(method: string, path: string, query: Q = {}, body: unknown = null): Promise<T> {
  try {
    return await invoke<T>('jf_request', { method, path, query: toQuery(query), body });
  } catch (e) {
    if (String(e).includes('unauthorized')) onUnauthorized.fn?.();
    throw e;
  }
}

async function get<T>(path: string, query: Q = {}, ttl = 20_000): Promise<T> {
  const key = path + JSON.stringify(query);
  const hit = cache.get(key);
  if (hit && Date.now() - hit.t < ttl) return hit.v as T;
  const v = await req<T>('GET', path, query);
  cache.set(key, { t: Date.now(), v });
  return v;
}

const FIELDS = 'Overview,Genres,CommunityRating,OfficialRating,ProductionYear,RunTimeTicks,ChildCount,AlbumArtist,Album';
const IMG = 'Primary,Backdrop,Thumb';
/** + title art, for the home's top shelf */
const IMG_SHELF = 'Primary,Backdrop,Thumb,Logo';

const VIEW_ORDER = ['movies', 'tvshows', 'music', 'boxsets'];
const viewRank = (v: Item) => {
  const i = VIEW_ORDER.indexOf(v.CollectionType ?? '');
  return i < 0 ? VIEW_ORDER.length : i;
};
/** Views the TV app doesn't show: Musica and Live TV are apps of their own (playlists live in Musica). */
const HIDDEN_VIEWS = ['playlists', 'livetv', 'music'];

const allViews = () =>
  get<{ Items: Item[] }>('/UserViews', { userId }).then((r) => [...r.Items].sort((a, b) => viewRank(a) - viewRank(b)));

export const api = {
  /** The signed-in user (Policy.IsAdministrator: may change settings that bind every profile). */
  me: () => get<{ Policy?: { IsAdministrator?: boolean } }>('/Users/Me', {}, 300_000),
  /** Libraries of the TV app, in our order (Film, Serie, Collezioni, rest). */
  views: () => allViews().then((Items) => ({ Items: Items.filter((v) => !HIDDEN_VIEWS.includes(v.CollectionType ?? '')) })),
  /** Any library by id (the Musica app's one isn't among `views`). */
  view: (id: string) => allViews().then((v) => v.find((x) => x.Id === id) ?? null),
  /** The music library (the Musica app). */
  musicView: () => allViews().then((v) => v.find((x) => x.CollectionType === 'music') ?? null),
  resume: () =>
    get<Page>('/UserItems/Resume', { userId, Limit: 20, MediaTypes: 'Video', Fields: FIELDS, EnableImageTypes: IMG_SHELF }, 0),
  nextUp: () => get<Page>('/Shows/NextUp', { userId, Limit: 20, Fields: FIELDS, EnableImageTypes: IMG_SHELF }, 0),
  latest: (parentId: string) =>
    get<Item[]>('/Items/Latest', { userId, ParentId: parentId, Limit: 20, Fields: FIELDS, EnableImageTypes: IMG }),
  items: (o: {
    parentId?: string;
    types?: string;
    sortBy?: string;
    sortOrder?: string;
    start?: number;
    limit?: number;
    search?: string;
  }) =>
    get<Page>('/Items', {
      userId,
      Recursive: true,
      ParentId: o.parentId,
      IncludeItemTypes: o.types,
      SortBy: o.sortBy,
      SortOrder: o.sortOrder,
      StartIndex: o.start ?? 0,
      Limit: o.limit ?? 60,
      SearchTerm: o.search,
      Fields: FIELDS,
      EnableImageTypes: IMG,
    }),
  /** Live TV channels, by channel number. */
  channels: () =>
    get<Page>('/LiveTv/Channels', { userId, EnableImageTypes: 'Primary', Limit: 500 }, 60_000).then((p) =>
      [...p.Items].sort((a, b) => Number(a.ChannelNumber ?? 9999) - Number(b.ChannelNumber ?? 9999)),
    ),
  /** Programmes of `channelIds` overlapping the next `hours`. */
  guide: (channelIds: string[], hours = 12) => {
    const now = new Date();
    return get<Page>(
      '/LiveTv/Programs',
      {
        userId,
        ChannelIds: channelIds.join(','),
        MinEndDate: now.toISOString(),
        MaxStartDate: new Date(now.getTime() + hours * 3600_000).toISOString(),
        SortBy: 'StartDate',
        Fields: 'Overview',
        EnableImageTypes: 'Primary',
        Limit: 2000,
      },
      0,
    );
  },
  /** Films, series and collections marked ♥ (Home's "Preferiti" row). */
  favorites: () =>
    get<Page>('/Items', {
      userId,
      Recursive: true,
      Filters: 'IsFavorite',
      IncludeItemTypes: 'Movie,Series,BoxSet',
      SortBy: 'SortName',
      Limit: 40,
      Fields: FIELDS,
      EnableImageTypes: IMG,
    }),
  /** Random films and series that have a backdrop (the Home top shelf). */
  random: (limit = 12) =>
    get<Page>(
      '/Items',
      {
        userId,
        Recursive: true,
        IncludeItemTypes: 'Movie,Series',
        ImageTypes: 'Backdrop',
        SortBy: 'Random',
        Limit: limit,
        Fields: FIELDS,
        EnableImageTypes: 'Primary,Backdrop,Logo',
      },
      0,
    ),
  /** Every song under `parentId` (whole queues are built from it). */
  tracks: (parentId: string, sortBy = 'SortName', sortOrder = 'Ascending') =>
    get<Page>('/Items', {
      userId,
      Recursive: true,
      ParentId: parentId,
      IncludeItemTypes: 'Audio',
      SortBy: sortBy,
      SortOrder: sortOrder,
      Limit: 5000,
      Fields: FIELDS,
      EnableImageTypes: 'Primary',
    }),
  /** The user's music playlists. */
  playlists: () =>
    get<Page>('/Items', {
      userId,
      Recursive: true,
      IncludeItemTypes: 'Playlist',
      SortBy: 'SortName',
      Fields: FIELDS,
      EnableImageTypes: 'Primary',
    }).then((p) => p.Items.filter((x) => x.MediaType === 'Audio')),
  playlistItems: (id: string) =>
    get<Page>(`/Playlists/${id}/Items`, { userId, Fields: FIELDS, EnableImageTypes: 'Primary' }),
  /** A new music playlist of this profile only (private: other profiles don't see it), with `ids` in it; its id. */
  async createPlaylist(name: string, ids: string[] = []) {
    const r = await req<{ Id: string }>('POST', '/Playlists', {}, { Name: name, Ids: ids, UserId: userId, MediaType: 'Audio', IsPublic: false, Users: [] });
    invalidate();
    return r.Id;
  },
  async addToPlaylist(id: string, ids: string[]) {
    await req('POST', `/Playlists/${id}/Items`, { ids: ids.join(','), userId });
    invalidate();
  },
  /** `entryIds`: the songs' `PlaylistItemId`s (a song can be in a playlist twice). */
  async removeFromPlaylist(id: string, entryIds: string[]) {
    await req('DELETE', `/Playlists/${id}/Items`, { entryIds: entryIds.join(',') });
    invalidate();
  },
  async renamePlaylist(id: string, name: string) {
    await req('POST', `/Playlists/${id}`, {}, { Name: name });
    invalidate();
  },
  async deletePlaylist(id: string) {
    await req('DELETE', `/Items/${id}`);
    invalidate();
  },
  /** Songs of the library matching `q` (Musica's search). */
  searchSongs: (q: string) =>
    get<Page>('/Items', { userId, Recursive: true, IncludeItemTypes: 'Audio', SearchTerm: q, Limit: 30, Fields: FIELDS, EnableImageTypes: 'Primary' }),
  nextUpFor: (seriesId: string) =>
    get<Page>('/Shows/NextUp', { userId, seriesId, Limit: 2, Fields: FIELDS, EnableImageTypes: IMG }, 0),
  seriesEpisodes: (seriesId: string) =>
    get<Page>(`/Shows/${seriesId}/Episodes`, { userId, Fields: 'RunTimeTicks' }),
  firstEpisode: (seriesId: string) =>
    get<Page>(`/Shows/${seriesId}/Episodes`, { userId, Limit: 1, Fields: FIELDS }),
  item: (id: string) => get<Item>(`/Items/${id}`, { userId }),
  /** Several items at once, with their title art (the home's pinned tiles). */
  byIds: (ids: string[]) =>
    get<Page>('/Items', { userId, Ids: ids.join(','), Fields: FIELDS, EnableImageTypes: IMG_SHELF }, 0),
  seasons: (seriesId: string) => get<Page>(`/Shows/${seriesId}/Seasons`, { userId, Fields: FIELDS }),
  episodes: (seriesId: string, seasonId: string) =>
    get<Page>(`/Shows/${seriesId}/Episodes`, { userId, seasonId, Fields: FIELDS }),
  /** The film / series with this TMDB id if it just arrived (among the 50 newest), else null.
   *  (Jellyfin ignores `AnyProviderIdEquals`, so the match is done here.) */
  byTmdb: (tmdb: number, type: 'Movie' | 'Series') =>
    get<Page>(
      '/Items',
      { userId, Recursive: true, IncludeItemTypes: type, SortBy: 'DateCreated', SortOrder: 'Descending', Limit: 50, Fields: 'ProviderIds' },
      0,
    ).then((r) => r.Items.find((it) => (it as { ProviderIds?: Record<string, string> }).ProviderIds?.Tmdb === String(tmdb)) ?? null),
  similar: (id: string) => get<Page>(`/Items/${id}/Similar`, { userId, Limit: 14, Fields: FIELDS }),
  children: (parentId: string) =>
    get<Page>('/Items', { userId, ParentId: parentId, Fields: FIELDS, EnableImageTypes: IMG, SortBy: 'ProductionYear,SortName' }),
  async setPlayed(id: string, played: boolean) {
    await req(played ? 'POST' : 'DELETE', `/UserPlayedItems/${id}`, { userId });
    invalidate();
  },
  async setFavorite(id: string, fav: boolean) {
    await req(fav ? 'POST' : 'DELETE', `/UserFavoriteItems/${id}`, { userId });
    invalidate();
  },
};

// ---- images ------------------------------------------------------------

const remPx = () => innerWidth / 120;
/** CSS rem width -> device pixels, rounded up to 64 so URLs (and cache hits) stay stable. */
const sized = (rem: number) => Math.max(64, Math.ceil((rem * remPx() * (devicePixelRatio || 1)) / 64) * 64);

type ImgType = 'Primary' | 'Backdrop' | 'Thumb' | 'Logo';
export function img(id: string, type: ImgType, rem: number, tag?: string): string {
  // NOTE: deliberately `v=` and not Jellyfin's usual `tag=`. With `tag` present a
  // server plugin burns language flags / rating badges into Primary posters;
  // `v` is ignored by the server but still busts our own disk cache.
  const width = type === 'Backdrop' ? Math.min(sized(rem), 1920) : sized(rem);
  return `jf://localhost/Items/${id}/Images/${type}?maxWidth=${width}&quality=85${tag ? `&v=${tag}` : ''}`;
}

/** Best image for a card. `wide` = 16:9 slot, otherwise poster/square. */
export function cardImage(it: Item, wide: boolean, rem: number): string | null {
  const t = it.ImageTags ?? {};
  if (wide) {
    if (it.Type === 'Episode' && t.Primary) return img(it.Id, 'Primary', rem, t.Primary);
    if (t.Thumb) return img(it.Id, 'Thumb', rem, t.Thumb);
    if (it.BackdropImageTags?.[0]) return img(it.Id, 'Backdrop', rem, it.BackdropImageTags[0]);
    if (it.ParentBackdropItemId && it.ParentBackdropImageTags?.[0])
      return img(it.ParentBackdropItemId, 'Backdrop', rem, it.ParentBackdropImageTags[0]);
    if (t.Primary) return img(it.Id, 'Primary', rem, t.Primary);
    return null;
  }
  if (t.Primary) return img(it.Id, 'Primary', rem, t.Primary);
  if (it.Type === 'Episode' && it.SeriesId && it.SeriesPrimaryImageTag)
    return img(it.SeriesId, 'Primary', rem, it.SeriesPrimaryImageTag);
  if (it.AlbumId && it.AlbumPrimaryImageTag) return img(it.AlbumId, 'Primary', rem, it.AlbumPrimaryImageTag);
  return null;
}

/** Full-bleed backdrop for the stage behind the UI. */
export function backdropUrl(it: Item | null, rem = 120): string | null {
  if (!it) return null;
  if (it.BackdropImageTags?.[0]) return img(it.Id, 'Backdrop', rem, it.BackdropImageTags[0]);
  if (it.ParentBackdropItemId && it.ParentBackdropImageTags?.[0])
    return img(it.ParentBackdropItemId, 'Backdrop', rem, it.ParentBackdropImageTags[0]);
  if (it.ImageTags?.Thumb) return img(it.Id, 'Thumb', rem, it.ImageTags.Thumb);
  return null;
}

export const logoUrl = (it: Item, rem = 40) =>
  it.ImageTags?.Logo ? img(it.Id, 'Logo', rem, it.ImageTags.Logo) : null;

// ---- formatting --------------------------------------------------------

export function runtime(it: Item): string {
  if (!it.RunTimeTicks) return '';
  const m = Math.round(it.RunTimeTicks / 600_000_000);
  return m >= 60 ? `${Math.floor(m / 60)} h ${m % 60} min` : `${m} min`;
}

export function progress(it: Item): number {
  const d = it.UserData;
  if (!d || d.Played) return 0;
  if (d.PlayedPercentage) return Math.min(100, d.PlayedPercentage);
  if (d.PlaybackPositionTicks && it.RunTimeTicks) return (d.PlaybackPositionTicks / it.RunTimeTicks) * 100;
  return 0;
}

export function episodeLabel(it: Item): string {
  if (it.Type !== 'Episode') return '';
  const s = it.ParentIndexNumber != null ? `S${it.ParentIndexNumber}` : '';
  const e = it.IndexNumber != null ? `E${it.IndexNumber}` : '';
  return `${s}${e}`;
}
