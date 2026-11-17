
// Azoto: the original look, and the full list of tokens every other theme
// starts from (a theme only lists what it changes). Pages use them in their
// styles; channel tokens (*-rgb) are "r g b" so a page can add its own alpha:
// rgb(var(--fg-rgb) / 0.1).
import type { Theme } from './index.svelte';

export const tokens: Record<string, string> = {
  '--bg': '#07080f',
  '--bg-rgb': '7 8 15',
  '--text': '#f4f5fa',
  '--text-rgb': '244 245 250',
  '--muted': 'rgba(244, 245, 250, 0.62)',
  // foreground ink: fills, lines and icons drawn on top of the background
  '--fg': '#fff',
  '--fg-rgb': '255 255 255',
  '--shadow-rgb': '0 0 0',
  // side sheets and dialogs
  '--panel-rgb': '16 18 32',
  // dark labels over pictures
  '--scrim-rgb': '10 12 24',
  '--glass-bg': 'rgba(28, 32, 52, 0.38)',
  '--glass-edge': 'rgba(255, 255, 255, 0.16)',
  '--blur': '2.4rem',
  // the focused thing: light pill with dark text, ring around pictures
  '--focus-bg': '#fff',
  '--focus-text': '#10121c',
  '--focus-text-rgb': '16 18 28',
  '--ring': '#fff',
  '--accent': '#7c8cff',
  '--accent-rgb': '124 140 255',
  '--accent-soft': '#9aa6ff',
  '--accent-on-focus': '#4a55c8',
  '--on-accent': '#fff',
  // the red of the Acer Azoto the TV box is (logo)
  '--azoto': '#e8202e',
  '--ok': '#6fe3a1',
  '--ok-on-focus': '#128a4a',
  '--danger': '#ff5d6c',
  '--danger-text': '#ff8a8a',
  '--destroy': '#e0162b',
  '--destroy-hi': '#ff4e45',
  '--caution': '#e0a040',
  '--caution-text': '#f0c070',
  // empty posters and covers
  '--placeholder': '#1d2240',
  '--placeholder-grad': 'linear-gradient(160deg, #2a3157, #151833)',
  '--avatar-grad': 'linear-gradient(135deg, #ff4fa3, #7c8cff)',
  // behind everything when the profile has no wallpaper
  '--wall': 'radial-gradient(35rem circle at 55rem 100%, #19d3b0, transparent), radial-gradient(35rem circle at calc(100% - 15rem) 45rem, #ff4fa3, transparent), radial-gradient(35rem circle at 20rem 15rem, #5b6cff, transparent), var(--bg)',
  // the interface typeface
  '--font': "'Inter', 'Noto Sans', system-ui, sans-serif",
};

export default {
  id: 'azoto',
  name: 'Azoto',
  note: 'Glass, colour and light: the original look',
  tokens: {},
} satisfies Theme;
