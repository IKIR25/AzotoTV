// Typed Seerr client. Calls go through Rust (`seerr_request`), which holds the
// API key, acts as the Seerr user linked to the Jellyfin user, and only allows
// catalogue + request endpoints. Artwork comes from TMDB via `tmdb://`.
import { invoke } from '@tauri-apps/api/core';
import { t } from '$lib/i18n';

export interface SeerrStatus {
  enabled: boolean;
  /** linked Seerr user, null if this Jellyfin user has no Seerr account */
  user: string | null;
  can_request: boolean;
}

/** Seerr MediaStatus */
export const MS = { UNKNOWN: 1, PENDING: 2, PROCESSING: 3, PARTIAL: 4, AVAILABLE: 5 } as const;

export interface MediaInfo {
  status: number;
  jellyfinMediaId?: string | null;
  seasons?: { seasonNumber: number; status: number }[];
}

export interface SItem {
  id: number;
  mediaType: 'movie' | 'tv' | 'person';
  title?: string;
  name?: string;
  releaseDate?: string;
  firstAirDate?: string;
  posterPath?: string | null;
  backdropPath?: string | null;
  overview?: string;
  voteAverage?: number;
  mediaInfo?: MediaInfo | null;
}

export interface SDetails extends SItem {
  tagline?: string;
  runtime?: number;
  genres?: { name: string }[];
  seasons?: { seasonNumber: number; name: string; episodeCount: number }[];
}

export interface SPage {
  page: number;
  totalPages: number;
  results: SItem[];
}

type Q = Record<string, string | number | undefined>;
const req = <T>(method: string, path: string, query: Q = {}, body: unknown = null) =>
  invoke<T>('seerr_request', {
    method,
    path,
    query: Object.entries(query)
      .filter(([, v]) => v !== undefined)
      .map(([k, v]) => [k, String(v)]),
    body,
  });

const LANG = { language: 'it' };

export const seerr = {
  status: () => invoke<SeerrStatus>('seerr_status'),
  search: (query: string, page = 1) => req<SPage>('GET', '/search', { query, page, ...LANG }),
  details: async (type: 'movie' | 'tv', id: number): Promise<SDetails> => ({
    ...(await req<SDetails>('GET', `/${type}/${id}`, LANG)),
    mediaType: type, // the details endpoints don't include it
  }),
  trending: (page = 1) => req<SPage>('GET', '/discover/trending', { page, ...LANG }),
  /** Request a movie, or the given seasons of a series. */
  request: (type: 'movie' | 'tv', id: number, seasons?: number[]) =>
    req('POST', '/request', {}, { mediaType: type, mediaId: id, ...(type === 'tv' ? { seasons } : {}) }),
};

export const tmdbImg = (path: string | null | undefined, size = 'w342') =>
  path ? `tmdb://localhost/${size}${path}` : null;

export const sTitle = (it: SItem) => it.title ?? it.name ?? '';
export const sYear = (it: SItem) => (it.releaseDate ?? it.firstAirDate ?? '').slice(0, 4);

/** Short Italian status label, or '' when nothing is requested/available yet. */
export function statusLabel(mi: MediaInfo | null | undefined): string {
  switch (mi?.status) {
    case MS.AVAILABLE:
      return t('Available');
    case MS.PARTIAL:
      return t('Partly available');
    case MS.PROCESSING:
      return t('On its way');
    case MS.PENDING:
      return t('Requested');
    default:
      return '';
  }
}

/** Where a request is on its way to the library (downloads.rs). */
export interface DlProgress {
  stage: 'none' | 'unreleased' | 'search' | 'wait' | 'cloud' | 'download' | 'unpack' | 'import' | 'error';
  /** 0..1, request to library */
  overall: number;
  /** bytes/s */
  speed: number | null;
  /** seconds */
  eta: number | null;
  note: string | null;
  /** series: [in the library, asked for] */
  episodes: [number, number] | null;
}

/** null on a machine without the *arr config (the dev PC) or when Radarr/Sonarr don't know it yet. */
export const dlProgress = (media: 'movie' | 'tv', tmdb: number) =>
  invoke<DlProgress | null>('dl_progress', { media, tmdb });

/** Is something on its way (worth asking how far it is)? */
export const inFlight = (mi: MediaInfo | null | undefined) =>
  mi?.status === MS.PROCESSING || mi?.status === MS.PARTIAL || mi?.status === MS.PENDING;
