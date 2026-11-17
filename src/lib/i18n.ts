// The interface's language. Text is written in English where it is used:
// `t('Who is watching?')`, `t('Hello, {name}', { name })`. Other languages are
// dictionaries from the English text to theirs (`i18n/<language>.json`, the
// same files Rust and the phone page use); what a dictionary lacks stays
// English. When one English text needs two translations, a note after a bar
// tells them apart and is never shown: `t('Play|game')`.
//
// Rust decides the language (locale.rs: the system's, or settings.json) and
// puts it in the page before anything runs, so `t` works at once, even in a
// module's top-level constants. Changing it reloads the page.
import { invoke } from '@tauri-apps/api/core';
import it from '../../i18n/it.json';

type Info = { lang: string; region: string; languages: { id: string; name: string }[] };

const DICTIONARIES: Record<string, Record<string, string>> = { it };

const injected = (globalThis as { __AZOTOTV__?: Info }).__AZOTOTV__;

/** The language picked a moment ago, until the app starts again with it. */
function picked(): string | null {
  try {
    return sessionStorage.getItem('azoto.lang');
  } catch {
    return null;
  }
}

const browser = typeof navigator === 'undefined' ? 'en-US' : navigator.language;

/** "en", "it". */
export const lang: string = picked() ?? injected?.lang ?? (browser.split('-')[0] in DICTIONARIES ? browser.split('-')[0] : 'en');
/** "US", "IT": whose dates, clock and YouTube. */
export const region: string = injected?.region ?? browser.split('-')[1] ?? 'US';
/** For `toLocaleDateString` and the like: the language with the region's habits. */
export const locale = `${lang}-${region}`;
export const languages = injected?.languages ?? [
  { id: 'en', name: 'English' },
  { id: 'it', name: 'Italiano' },
];

/** Month names as the language has them, January first. */
export const MONTHS = Array.from({ length: 12 }, (_, i) => new Date(2021, i, 15).toLocaleDateString(locale, { month: 'long' }));
/** Weekday names, Sunday first. */
export const WEEKDAYS = Array.from({ length: 7 }, (_, i) => new Date(2021, 7, 1 + i).toLocaleDateString(locale, { weekday: 'long' }));

const dictionary = DICTIONARIES[lang] ?? {};

/** `en` in the interface's language, with `{name}` replaced from `values`. */
export function t(en: string, values?: Record<string, unknown>): string {
  let text = dictionary[en] || en.split('|')[0];
  if (values) for (const name in values) text = text.replaceAll(`{${name}}`, String(values[name]));
  return text;
}

/** One of two texts by a number, which they get as `{n}`: `tn(3, '{n} song', '{n} songs')`. */
export function tn(n: number, one: string, many: string, values?: Record<string, unknown>): string {
  return t(n === 1 ? one : many, { n, ...values });
}

/** Switches language: saved for good by Rust, then the page starts again. */
export async function setLanguage(id: string) {
  await invoke('set_language', { lang: id });
  try {
    sessionStorage.setItem('azoto.lang', id);
  } catch {
    // no storage: the next start has it anyway
  }
  location.reload();
}
