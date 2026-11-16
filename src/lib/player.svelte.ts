// Playback orchestration. The heavy lifting (mpv, progress reporting) lives in
// Rust (`player.rs`); this decides *what* to play and reacts when it ends.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { api, episodeLabel, invalidate, type Item } from './jf';
import { refresh, showToast } from './stage.svelte';
import { auth } from './session.svelte';
import { initialChoice, toPlayer, type Choice } from './tracks';

/** `now`: what plays, for the phones' "Continua qui" (a film / episode, or a live channel). */
export const player = $state({ active: false, now: null as null | { id: string; name: string; live: boolean } });
let current: Item | null = null;

interface Ended {
  item_id: string;
  /** "eof" watched to the end, "quit" stopped by the user, "error" */
  reason: string;
  position_ticks: number;
}

/** Series -> next episode, Season -> first unwatched episode, else the item itself. */
async function resolve(it: Item): Promise<Item | null> {
  if (it.Type === 'Series') {
    const next = (await api.nextUpFor(it.Id)).Items[0];
    return next ?? (await api.firstEpisode(it.Id)).Items[0] ?? null;
  }
  if (it.Type === 'Season' && it.SeriesId) {
    const eps = (await api.episodes(it.SeriesId, it.Id)).Items;
    return eps.find((e) => !e.UserData?.Played) ?? eps[0] ?? null;
  }
  return it;
}

export async function play(
  it: Item,
  opts: { fromStart?: boolean; startTicks?: number; tracks?: Choice } = {},
) {
  if (player.active) return;
  player.active = true;
  try {
    const resolved = await resolve(it);
    if (!resolved) {
      player.active = false;
      return showToast('Niente da riprodurre');
    }
    // stream list (only single-item fetches have it) for the audio/subtitle choice
    const target = resolved.MediaStreams ? resolved : await api.item(resolved.Id).catch(() => resolved);
    const tracks = target.MediaStreams
      ? toPlayer(target, opts.tracks ?? initialChoice(auth.id, target))
      : {};
    const d = target.UserData;
    const resume =
      opts.startTicks ?? (!opts.fromStart && d && !d.Played ? (d.PlaybackPositionTicks ?? 0) : 0);
    current = target;
    player.now = { id: target.Id, name: target.Type === 'Episode' ? `${target.SeriesName ?? ''} · ${episodeLabel(target)}` : target.Name, live: false };

    const isEpisode = target.Type === 'Episode';
    let nextTitle: string | null = null;
    if (isEpisode && target.SeriesId) {
      try {
        const all = (await api.seriesEpisodes(target.SeriesId)).Items;
        const i = all.findIndex((e) => e.Id === target.Id);
        nextTitle = all[i + 1]?.Name ?? null;
      } catch {}
    }
    await invoke('player_play', {
      args: {
        item_id: target.Id,
        media_type: target.MediaType === 'Audio' ? 'Audio' : 'Video',
        start_ticks: resume,
        title: isEpisode ? (target.SeriesName ?? target.Name) : target.Name,
        subtitle: isEpisode
          ? `${episodeLabel(target)} · ${target.Name}`
          : [target.ProductionYear, target.AlbumArtist].filter(Boolean).join(' · '),
        next_title: nextTitle,
        ...tracks,
      },
    });
  } catch (e) {
    player.active = false;
    player.now = null;
    current = null;
    showToast(`Riproduzione non riuscita: ${e}`);
  }
}

/** Call once at startup: refresh the UI and chain episodes when mpv exits. */
export function initPlayer() {
  const un = listen<Ended>('player-ended', async (e) => {
    player.active = false;
    player.now = null;
    const finished = current;
    current = null;
    invalidate();
    refresh.n++;
    if (e.payload.reason === 'error') return showToast('Errore di riproduzione');
    if (e.payload.reason === 'eof' && finished?.Type === 'Episode' && finished.SeriesId) {
      const next = (await api.nextUpFor(finished.SeriesId)).Items.find((x) => x.Id !== finished.Id);
      if (next) {
        showToast(`Prossimo episodio: ${next.Name}`);
        await play(next);
      }
    }
  });
  return () => void un.then((f) => f());
}
