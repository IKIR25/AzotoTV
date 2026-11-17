// Audio / subtitle choice for playback: Italian labels, defaults, a per-series
// memory (by language, so it carries over to the next episodes) and the
// mapping from Jellyfin stream indexes to mpv track ids.
import type { Item, MediaStream } from './jf';
import { t } from '$lib/i18n';

/** Jellyfin stream indexes; sub -1 = off. */
export interface Choice {
  audio: number | null;
  sub: number;
}

const LANGS: Record<string, string> = {
  ita: t('Italian'), it: t('Italian'), eng: t('English'), en: t('English'), spa: t('Spanish'), es: t('Spanish'),
  fra: t('French'), fre: t('French'), fr: t('French'), deu: t('German'), ger: t('German'), de: t('German'),
  jpn: t('Japanese'), ja: t('Japanese'), por: t('Portuguese'), pt: t('Portuguese'), rus: t('Russian'), ru: t('Russian'),
  zho: t('Chinese'), chi: t('Chinese'), zh: t('Chinese'), kor: t('Korean'), ko: t('Korean'), nld: t('Dutch'), dut: t('Dutch'),
  pol: t('Polish'), tur: t('Turkish'), ara: t('Arabic'), hin: 'Hindi', swe: t('Swedish'), nor: t('Norwegian'), nob: t('Norwegian'),
  dan: t('Danish'), fin: t('Finnish'), ell: t('Greek'), gre: t('Greek'), ces: t('Czech'), cze: t('Czech'), hun: t('Hungarian'),
  ron: t('Romanian'), rum: t('Romanian'), ukr: t('Ukrainian'), heb: t('Hebrew'), tha: t('Thai'), vie: t('Vietnamese'),
  ind: t('Indonesian'), bul: t('Bulgarian'), hrv: t('Croatian'), srp: t('Serbian'), slv: t('Slovenian'), slk: t('Slovak'),
  est: t('Estonian'), lav: t('Latvian'), lit: t('Lithuanian'), isl: t('Icelandic'), cat: t('Catalan'), msa: t('Malay'),
};
const lang = (code?: string) =>
  !code || code === 'und' ? t('Unknown') : (LANGS[code.toLowerCase()] ?? code.toUpperCase());

const CODECS: Record<string, string> = {
  eac3: 'Dolby Digital+', ac3: 'Dolby Digital', truehd: 'Dolby TrueHD', dts: 'DTS', aac: 'AAC',
  flac: 'FLAC', opus: 'Opus', mp3: 'MP3', vorbis: 'Vorbis', pcm_s16le: 'PCM', pcm_s24le: 'PCM',
};

export const audioStreams = (it: Item) => (it.MediaStreams ?? []).filter((s) => s.Type === 'Audio');
export const subStreams = (it: Item) => (it.MediaStreams ?? []).filter((s) => s.Type === 'Subtitle');

/** Extra words from the stream title, if they say more than the language ("Commento", "European"…). */
function extra(s: MediaStream) {
  const t = s.Title?.trim();
  if (!t || /^(forced|sdh|default|predefinito)$/i.test(t)) return '';
  if (t.toLowerCase() === lang(s.Language).toLowerCase() || t.toLowerCase() === s.Language) return '';
  return t;
}

export function audioLabel(s: MediaStream) {
  const codec = CODECS[s.Codec?.toLowerCase() ?? ''] ?? s.Codec?.toUpperCase() ?? '';
  const atmos = /atmos/i.test(s.DisplayTitle ?? '') ? ' Atmos' : '';
  const ch = s.ChannelLayout ?? (s.Channels ? `${s.Channels}ch` : '');
  return {
    main: lang(s.Language) + (extra(s) ? ` · ${extra(s)}` : ''),
    sub: [codec + atmos, ch].filter(Boolean).join(' '),
  };
}

export function subLabel(s: MediaStream) {
  const tags = [
    s.IsForced ? 'forzati' : '',
    s.IsHearingImpaired || /sdh/i.test(s.Title ?? '') ? t('for the hard of hearing') : '',
  ].filter(Boolean);
  return {
    main: lang(s.Language) + (tags.length ? ` (${tags.join(', ')})` : ''),
    sub: [extra(s), s.IsExternal ? t('external file') : '', (s.Codec ?? '').toUpperCase()].filter(Boolean).join(' · '),
  };
}

// ---- memory per series (by language) ------------------------------------

interface Pref {
  a?: string;
  s: string | null; // null = off
  forced?: boolean;
}
const key = (user: string, series: string) => `azoto.tracks.${user}.${series}`;

export function remember(user: string, it: Item, c: Choice) {
  if (it.Type !== 'Episode' || !it.SeriesId) return;
  const ms = it.MediaStreams ?? [];
  const a = ms.find((s) => s.Index === c.audio);
  const s = ms.find((x) => x.Index === c.sub);
  const p: Pref = { a: a?.Language, s: s ? (s.Language ?? 'und') : null, forced: s?.IsForced };
  try {
    localStorage.setItem(key(user, it.SeriesId), JSON.stringify(p));
  } catch {}
}

function recall(user: string, it: Item): Pref | null {
  if (it.Type !== 'Episode' || !it.SeriesId) return null;
  try {
    return JSON.parse(localStorage.getItem(key(user, it.SeriesId)) ?? 'null');
  } catch {
    return null;
  }
}

/** Starting choice: this series' remembered languages, else Jellyfin's per-user defaults. */
export function initialChoice(user: string, it: Item): Choice {
  const src = it.MediaSources?.[0];
  const auds = audioStreams(it);
  const subs = subStreams(it);
  const c: Choice = {
    audio: src?.DefaultAudioStreamIndex ?? auds[0]?.Index ?? null,
    sub: src?.DefaultSubtitleStreamIndex ?? -1,
  };
  const p = recall(user, it);
  if (p) {
    const a = auds.find((s) => s.Language === p.a);
    if (a) c.audio = a.Index;
    if (p.s === null) c.sub = -1;
    else {
      const same = subs.filter((s) => (s.Language ?? 'und') === p.s);
      const s = same.find((x) => !!x.IsForced === !!p.forced) ?? same[0];
      if (s) c.sub = s.Index;
    }
  }
  return c;
}

/** What the Rust player needs: mpv track ids for the file's own tracks + external subtitle files. */
export function toPlayer(it: Item, c: Choice) {
  const auds = audioStreams(it).filter((s) => !s.IsExternal);
  const embedded = subStreams(it).filter((s) => !s.IsExternal);
  const ai = auds.findIndex((s) => s.Index === c.audio);
  const si = embedded.findIndex((s) => s.Index === c.sub);
  return {
    aid: ai >= 0 ? ai + 1 : null,
    sid: c.sub === -1 ? 0 : si >= 0 ? si + 1 : null,
    ext_subs: subStreams(it)
      .filter((s) => s.IsExternal)
      .map((s) => ({
        index: s.Index,
        codec: s.Codec ?? 'srt',
        title: subLabel(s).main,
        lang: s.Language ?? '',
        select: s.Index === c.sub,
      })),
    audio_index: c.audio,
    subtitle_index: c.sub,
  };
}
