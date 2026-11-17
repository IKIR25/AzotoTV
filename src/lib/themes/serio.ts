// Serio (Serious): calm and flat. A living room in the evening with a lamp on,
// and a printed TV guide: warm greys instead of blue-black, solid panels with
// hairline edges (no glass, no glows, no shadows), one lamp-amber accent used
// sparingly, IBM Plex Sans in three weights, figures in even columns. The
// colour is left to what's on: posters, covers, the apps' tiles. Two versions,
// for the evening and for the day, and one that follows the sun.
// Designed with the unslop skill (Damir Sokolovsky, @heydamir, CC BY 4.0).
import '@fontsource/ibm-plex-sans/400.css';
import '@fontsource/ibm-plex-sans/500.css';
import '@fontsource/ibm-plex-sans/600.css';
import type { Theme } from './index.svelte';

// only 400-600 are loaded, so the UI's 700s come out as 600
const font = "'IBM Plex Sans', 'Noto Sans', system-ui, sans-serif";

const css = `
:root[data-theme^='serio'] body { font-variant-numeric: tabular-nums; }
:root[data-theme^='serio'] .glass { backdrop-filter: none; -webkit-backdrop-filter: none; box-shadow: none; }
:root[data-theme^='serio'] :is(h1, h2) { font-weight: 500; }
:root[data-theme^='serio'] :is(h2, h3, .k, .kind) { text-transform: none; letter-spacing: 0; }
/* a lit controller slot: an outline, not a glow */
:root[data-theme^='serio'] .slot.lit { box-shadow: 0 0 0 0.3rem var(--accent); }
/* the printed guide: a hairline under a page's title (not under a programme's) */
:root[data-theme^='serio'] :is(header, .glass) > h1:first-child { padding-bottom: 1rem; border-bottom: 1px solid rgb(var(--fg-rgb) / 0.12); }
`;

export const night: Theme = {
  id: 'serio-night',
  name: 'Serious · night',
  note: 'Flat and quiet, warm dark greys',
  css,
  tokens: {
    '--bg': '#141311',
    '--bg-rgb': '20 19 17',
    '--text': '#efe9de',
    '--text-rgb': '239 233 222',
    '--muted': 'rgba(239, 233, 222, 0.6)',
    '--fg': '#efe9de',
    '--fg-rgb': '239 233 222',
    '--panel-rgb': '28 26 24',
    '--scrim-rgb': '20 19 17',
    '--glass-bg': '#1c1a18',
    '--glass-edge': 'rgba(239, 233, 222, 0.08)',
    '--blur': '0px',
    '--focus-bg': '#efe9de',
    '--focus-text': '#1a1815',
    '--focus-text-rgb': '26 24 21',
    '--ring': '#efe9de',
    '--accent': '#d9a35b',
    '--accent-rgb': '217 163 91',
    '--accent-soft': '#d9b17a',
    '--accent-on-focus': '#8a5a1c',
    '--on-accent': '#1a1815',
    '--ok': '#4cc38a',
    '--ok-on-focus': '#16794a',
    '--danger': '#e5484d',
    '--danger-text': '#f08a8d',
    '--destroy': '#c93a3f',
    '--destroy-hi': '#e5484d',
    '--caution': '#d9a35b',
    '--caution-text': '#e8c48e',
    '--placeholder': '#262421',
    '--placeholder-grad': '#262421',
    '--avatar-grad': '#d9a35b',
    '--wall': 'var(--bg)',
    '--font': font,
  },
};

export const day: Theme = {
  id: 'serio-day',
  name: 'Serious · day',
  note: 'The same on light paper',
  css,
  tokens: {
    '--bg': '#f4f0e8',
    '--bg-rgb': '244 240 232',
    '--text': '#1f1b16',
    '--text-rgb': '31 27 22',
    '--muted': 'rgba(31, 27, 22, 0.62)',
    '--fg': '#1f1b16',
    '--fg-rgb': '31 27 22',
    '--panel-rgb': '253 251 247',
    '--scrim-rgb': '253 251 247',
    '--glass-bg': '#fdfbf7',
    '--glass-edge': 'rgba(31, 27, 22, 0.1)',
    '--blur': '0px',
    '--focus-bg': '#1f1b16',
    '--focus-text': '#f7f4ee',
    '--focus-text-rgb': '247 244 238',
    '--ring': '#1f1b16',
    '--accent': '#9a6220',
    '--accent-rgb': '154 98 32',
    '--accent-soft': '#9a6220',
    '--accent-on-focus': '#e8c48e',
    '--on-accent': '#fff',
    '--ok': '#15803d',
    '--ok-on-focus': '#6fd29a',
    '--danger': '#dc2626',
    '--danger-text': '#b91c1c',
    '--destroy': '#c62828',
    '--destroy-hi': '#dc2626',
    '--caution': '#b7791f',
    '--caution-text': '#92600f',
    '--placeholder': '#e7e1d6',
    '--placeholder-grad': '#e7e1d6',
    '--avatar-grad': '#9a6220',
    '--wall': 'var(--bg)',
    '--font': font,
  },
};

export const auto: Theme = {
  id: 'serio-auto',
  name: 'Serious · automatic',
  note: 'Day from sunrise, night from sunset',
  tokens: {},
  auto: { day: day.id, night: night.id },
};
