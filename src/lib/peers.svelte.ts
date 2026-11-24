// Linked AzotoTVs (peer.rs): their games in Giochi, downloads from them.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Game } from './games.svelte';

export interface PeerState {
  id: string;
  name: string;
  url: string;
  online: boolean;
}

export interface Found {
  url: string;
  id: string;
  name: string;
  linked: boolean;
}

/** A game on a linked AzotoTV. */
export interface Remote {
  peer: string;
  peer_name: string;
  id: string;
  name: string;
  type: Game['type'];
  platform: Game['platform'];
  steam_appid: number | null;
  description: string;
  year: string | null;
  genres: string[];
  art: string[];
  art_stamp: number;
  players: number;
  /** bytes */
  size: number;
  /** downloaded here already: the local game's id */
  here: string | null;
  /** bytes: the emulator and what it needs, for a machine without it */
  runner_size: number;
  /** what this machine lacks to play it ("Eden"), when something */
  missing: string | null;
  /** it comes along with the download (else it's to install by hand) */
  brings: boolean;
}

export interface Progress {
  peer: string;
  /** that AzotoTV's name */
  peer_name: string;
  game: string;
  name: string;
  done: number;
  total: number;
  speed: number;
  state: 'running' | 'done' | 'error' | 'cancelled';
  error: string | null;
  id: string | null;
  /** after the files: "Installing Dolphin"; with `total`, a step that tells how far it is */
  step: string | null;
  /** an update of this local game, rather than a new one */
  update: string | null;
}

/** What an update from the AzotoTV a downloaded game came from would bring. */
export interface Update {
  peer: string;
  peer_name: string;
  /** the game's id there */
  game: string;
  files: number;
  /** bytes to fetch */
  bytes: number;
  removed: number;
}

export const peers = $state({ remote: [] as Remote[], downloads: {} as Record<string, Progress> });

const k = (peer: string, game: string) => `${peer}/${game}`;
export const download = (r: { peer: string; id: string }) => peers.downloads[k(r.peer, r.id)] ?? null;

let started = false;
/** Keeps `peers.downloads` up to date (once, at start). */
export function watchDownloads() {
  if (started) return;
  started = true;
  void invoke<Progress[]>('peer_downloads').then((l) => l.forEach((p) => (peers.downloads[k(p.peer, p.game)] = p)));
  void listen<Progress>('peer-download', (e) => (peers.downloads[k(e.payload.peer, e.payload.game)] = e.payload));
}

export async function loadRemote() {
  peers.remote = await invoke<Remote[]>('peer_games');
  return peers.remote;
}

export const peerStatus = () => invoke<PeerState[]>('peer_status');
export const findPeer = () => invoke<Found | null>('peer_find');
export const linkPeer = (url: string) => invoke<PeerState>('peer_link', { url });
export const unlinkPeer = (id: string) => invoke<void>('peer_unlink', { id });
export const peerTargets = () => invoke<string[]>('peer_targets');
export const startDownload = (r: Remote, into: string) => invoke<void>('peer_download', { peer: r.peer, game: r.id, name: r.name, into });
export const cancelDownload = (r: { peer: string; id: string }) => invoke<void>('peer_cancel', { peer: r.peer, game: r.id });
/** null: up to date, or not a game from another AzotoTV; throws when that one doesn't answer */
export const updateInfo = (id: string) => invoke<Update | null>('peer_update_info', { id });
export const startUpdate = (id: string) => invoke<void>('peer_update', { id });

export const remoteArt = (r: Remote, kind: string) =>
  r.art.includes(kind) ? `peer://localhost/${r.peer}/${encodeURIComponent(r.id)}/${kind}?v=${r.art_stamp}` : null;

export function size(b: number) {
  const gb = b / 1e9;
  return gb >= 1 ? `${gb.toFixed(1).replace('.', ',')} GB` : `${Math.max(1, Math.round(b / 1e6))} MB`;
}

export function speed(b: number) {
  const mb = b / 1e6;
  return mb >= 1 ? `${mb.toFixed(1).replace('.', ',')} MB/s` : `${Math.round(b / 1000)} kB/s`;
}
