// Themes: how AzotoTV looks, chosen per profile in START › Theme. A theme is a
// module in this folder: a name and a note in English (i18n/it.json has the
// Italian), the tokens it changes from Azoto's (azoto.ts lists them all) and,
// when colours aren't enough, some CSS of its own (surfaces, type). A theme
// can also be a switch between two others by the sun (`auto`). The choice is
// kept on this TV per profile, like the wallpaper.
import { untrack } from 'svelte';
import { auth } from '../session.svelte';
import azoto, { tokens as base } from './azoto';
import * as serio from './serio';
import { isDay } from './sun';

export interface Theme {
  id: string;
  /** English; t() translates it */
  name: string;
  /** one line under the name in the picker; English, t() translates it */
  note: string;
  /** the tokens this theme changes, over Azoto's */
  tokens: Record<string, string>;
  /** extra CSS, applied only while the theme is on (scope it under :root[data-theme='<id>']) */
  css?: string;
  /** not a look of its own: `day` from sunrise, `night` from sunset (theme ids) */
  auto?: { day: string; night: string };
}

export const themes: Theme[] = [azoto, serio.night, serio.day, serio.auto];

const find = (id: string) => themes.find((t) => t.id === id) ?? azoto;

/** The look a theme stands for now: itself, or for an automatic one, day's or night's. */
export const resolve = (t: Theme): Theme => (t.auto ? find(isDay() ? t.auto.day : t.auto.night) : t);

/** Every token of a theme, Azoto's filled in. */
export const allTokens = (t: Theme) => ({ ...base, ...resolve(t).tokens });

/** The tokens as a style attribute: a preview card wears its theme this way. */
export const tokenStyle = (t: Theme) =>
  Object.entries(allTokens(t))
    .map(([k, v]) => `${k}: ${v}`)
    .join('; ');

const key = (user: string) => `azoto.theme.${user}`;

/** `pick`: the theme the profile chose; `id`: the look on screen (they differ for an automatic one). */
export const current = $state({ pick: 'azoto', id: 'azoto' });

function saved(user: string): string {
  try {
    return localStorage.getItem(key(user)) ?? 'azoto';
  } catch {
    return 'azoto';
  }
}

let sunTimer: ReturnType<typeof setInterval> | undefined;

function apply(chosen: Theme) {
  const t = resolve(chosen);
  current.pick = chosen.id;
  // an automatic theme looks at the sun once a minute; nothing is redrawn until it turns
  clearInterval(sunTimer);
  if (chosen.auto) sunTimer = setInterval(() => resolve(chosen).id !== current.id && apply(chosen), 60_000);
  if (current.id === t.id && document.getElementById('azoto-theme')) return;
  let el = document.getElementById('azoto-theme') as HTMLStyleElement | null;
  if (!el) {
    el = document.createElement('style');
    el.id = 'azoto-theme';
    document.head.append(el);
  }
  // :root:root beats app.css's :root whatever order the sheets load in
  el.textContent = `:root:root { ${tokenStyle(t)} }\n${t.css ?? ''}`;
  document.documentElement.dataset.theme = t.id;
  current.id = t.id;
}

/** Puts on the theme of whoever is signed in (Azoto on the profile screen). */
export function applyProfileTheme() {
  const id = auth.state === 'in' ? saved(auth.id) : 'azoto';
  untrack(() => apply(find(id)));
}

/** This profile's theme from now on; it shows at once. */
export function setTheme(id: string) {
  try {
    localStorage.setItem(key(auth.id), id);
  } catch {}
  apply(find(id));
}
