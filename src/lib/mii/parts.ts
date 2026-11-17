// What the Mii editor lets you change: the Switch's Mii parts, as paths into
// miijs's JSON look with the ranges that survive its encoding (measured: each
// value encoded to StoreData and back), grouped in tabs like the consoles'
// Mii makers.
import { COMMON, FAVORITE, SKIN } from './colors';
import { t } from '$lib/i18n';

/** A choice from a grid of pictures (the Mii with each option), laid out
 * like the Switch's Mii maker: `grid` = its rows of values. */
export type PartField = { kind: 'parts'; path: string; label: string; grid: number[][]; zoom?: boolean };
/** A colour: the Switch's 100, the skin tones or the favourite colours. */
export type ColorField = { kind: 'color'; path: string; label: string; palette: 'common' | 'skin' | 'favorite'; grid: number[][] };
/** A number moved with left / right. */
export type SliderField = { kind: 'slider'; path: string; label: string; min: number; max: number; low?: string; high?: string };
/** Two options side by side. */
export type ToggleField = { kind: 'toggle'; path: string; label: string; options: [string, string]; values?: [unknown, unknown] };
export type Field = PartField | ColorField | SliderField | ToggleField;

export type Tab = { key: string; name: string; fields: Field[] };

// ---- where the Switch's Mii maker puts each option: rows of values, from
// miijs's lookup tables (its "how to make this Mii" instructions). Its colour
// grid says 59, 59 / 62, 62 / 75, 75 where 58, 61, 76 are missing: fixed here.
const rows = (count: number, cols: number) =>
  Array.from({ length: Math.ceil(count / cols) }, (_, r) => Array.from({ length: Math.min(cols, count - r * cols) }, (_, c) => r * cols + c));

export const GRIDS = {
  hair: [
    [33, 47, 40, 37, 32, 107], [48, 51, 55, 70, 44, 66], [52, 50, 38, 49, 43, 31], [56, 68, 62, 115, 76, 119],
    [64, 81, 116, 121, 22, 58], [60, 87, 125, 117, 73, 75], [42, 89, 57, 54, 80, 34], [23, 86, 88, 118, 39, 36],
    [45, 67, 59, 65, 41, 30], [12, 16, 10, 82, 128, 129], [14, 95, 105, 100, 6, 20], [93, 102, 27, 4, 17, 110],
    [123, 8, 106, 72, 3, 21], [0, 98, 63, 90, 11, 120], [5, 74, 108, 94, 124, 25], [99, 69, 35, 13, 122, 113],
    [53, 24, 85, 83, 71, 131], [96, 101, 29, 7, 15, 112], [79, 1, 109, 127, 91, 26], [61, 103, 2, 77, 18, 92],
    [84, 9, 19, 130, 97, 104], [46, 78, 28, 114, 126, 111],
  ],
  eyes: [
    [2, 4, 0, 8, 39, 17], [1, 26, 16, 15, 27, 20], [33, 11, 19, 32, 9, 12], [23, 34, 21, 25, 40, 35],
    [5, 41, 13, 36, 37, 6], [24, 30, 31, 18, 28, 46], [7, 44, 38, 42, 45, 29], [3, 43, 22, 10, 14, 47],
    [48, 49, 50, 53, 59, 56], [54, 58, 57, 55, 51, 52],
  ],
  eyebrows: [[6, 0, 12, 1, 9, 19], [7, 21, 8, 17, 5, 4], [11, 10, 2, 3, 14, 20], [15, 13, 22, 18, 16, 23]],
  noses: [[1, 10, 2, 3, 6, 0], [5, 4, 8, 9, 7, 11], [13, 14, 12, 17, 16, 15]],
  mouths: [
    [23, 1, 18, 21, 22, 5], [0, 8, 10, 16, 6, 13], [7, 9, 2, 11, 3, 4],
    [15, 17, 20, 19, 14, 12], [27, 30, 24, 25, 29, 28], [26, 35, 31, 34, 33, 32],
  ],
  glasses: [[0, 4, 9, 2, 10], [3, 12, 5, 1, 11], [13, 8, 14, 7, 6], [18, 17, 19, 16, 15]],
  /** face shape, wrinkles, makeup: in number order, 4 a row */
  face: rows(12, 4),
  /** beard and moustache: 3 a row */
  beard: rows(6, 3),
  colors: [
    [2, 24, 10, 23, 15, 20, 21, 25, 26, 27], [28, 29, 30, 31, 32, 33, 34, 35, 36, 37],
    [38, 39, 40, 41, 42, 43, 44, 45, 46, 47], [48, 16, 49, 12, 50, 51, 52, 53, 54, 55],
    [56, 57, 58, 59, 13, 60, 61, 62, 63, 64], [65, 66, 67, 68, 69, 70, 71, 72, 73, 74],
    [5, 11, 75, 76, 77, 78, 79, 80, 81, 82], [14, 83, 6, 17, 7, 84, 85, 86, 87, 88],
    [1, 3, 89, 19, 90, 91, 22, 92, 93, 94], [8, 0, 95, 9, 18, 4, 96, 97, 98, 99],
  ],
  skin: [[0, 7, 1, 4, 5], [6, 3, 2, 8, 9]],
  favorite: rows(12, 12),
};

const color = (path: string, label: string, palette: ColorField['palette'] = 'common'): ColorField => ({
  kind: 'color',
  path,
  label,
  palette,
  grid: palette === 'common' ? GRIDS.colors : GRIDS[palette],
});

export const PALETTES = {
  common: COMMON,
  skin: SKIN,
  favorite: FAVORITE.map((f) => f.hex),
};

const size = (path: string, max: number): SliderField => ({ kind: 'slider', path, label: t('Size'), min: 0, max, low: t('Small'), high: t('Large') });
const squash = (path: string): SliderField => ({ kind: 'slider', path, label: t('Proportions'), min: 0, max: 6, low: t('Squashed'), high: t('Stretched') });
const rotation = (path: string, max: number): SliderField => ({ kind: 'slider', path, label: t('Rotation'), min: 0, max });
const apart = (path: string): SliderField => ({ kind: 'slider', path, label: t('Spacing'), min: 0, max: 12, low: t('Close|near'), high: t('Far apart') });
const yPos = (path: string, min: number, max: number): SliderField => ({ kind: 'slider', path, label: t('Position'), min, max, low: t('Up'), high: t('Down') });

export const TABS: Tab[] = [
  {
    key: 'profile',
    name: t('Profile'),
    fields: [
      { kind: 'toggle', path: 'general.gender', label: t('Gender'), options: [t('Male'), t('Female')], values: [0, 1] },
      color('general.favoriteColor', t('Favorite color'), 'favorite'),
      { kind: 'slider', path: 'general.height', label: t('Height'), min: 0, max: 127, low: t('Short'), high: t('Tall') },
      { kind: 'slider', path: 'general.weight', label: t('Build'), min: 0, max: 127, low: t('Thin'), high: t('Heavy') },
    ],
  },
  {
    key: 'face',
    name: t('Face'),
    fields: [
      { kind: 'parts', path: 'face.type', label: t('Shape'), grid: GRIDS.face },
      color('face.color', t('Skin tone'), 'skin'),
      { kind: 'parts', path: 'face.feature', label: t('Wrinkles'), grid: GRIDS.face, zoom: true },
      { kind: 'parts', path: 'face.makeup', label: t('Make-up'), grid: GRIDS.face, zoom: true },
    ],
  },
  {
    key: 'hair',
    name: t('Hair'),
    fields: [
      { kind: 'parts', path: 'hair.type', label: t('Hairstyle'), grid: GRIDS.hair },
      color('hair.color', t('Color')),
      { kind: 'toggle', path: 'hair.flipped', label: t('Parting'), options: [t('Normal'), t('Flipped')], values: [false, true] },
    ],
  },
  {
    key: 'eyebrows',
    name: t('Eyebrows'),
    fields: [
      { kind: 'parts', path: 'eyebrows.type', label: t('Shape'), grid: GRIDS.eyebrows, zoom: true },
      color('eyebrows.color', t('Color')),
      size('eyebrows.size', 8),
      squash('eyebrows.squash'),
      rotation('eyebrows.rotation', 11),
      apart('eyebrows.distanceApart'),
      yPos('eyebrows.yPosition', 0, 15), // miijs counts from the Switch's 3
    ],
  },
  {
    key: 'eyes',
    name: t('Eyes'),
    fields: [
      { kind: 'parts', path: 'eyes.type', label: t('Shape'), grid: GRIDS.eyes, zoom: true },
      color('eyes.color', t('Color')),
      size('eyes.size', 7),
      squash('eyes.squash'),
      rotation('eyes.rotation', 7),
      apart('eyes.distanceApart'),
      yPos('eyes.yPosition', 0, 18),
    ],
  },
  {
    key: 'nose',
    name: t('Nose'),
    fields: [
      { kind: 'parts', path: 'nose.type', label: t('Shape'), grid: GRIDS.noses, zoom: true },
      size('nose.size', 8),
      yPos('nose.yPosition', 0, 18),
    ],
  },
  {
    key: 'mouth',
    name: t('Mouth'),
    fields: [
      { kind: 'parts', path: 'mouth.type', label: t('Shape'), grid: GRIDS.mouths, zoom: true },
      color('mouth.color', t('Color')),
      size('mouth.size', 8),
      squash('mouth.squash'),
      yPos('mouth.yPosition', 0, 18),
    ],
  },
  {
    key: 'beard',
    name: t('Beard'),
    fields: [
      { kind: 'parts', path: 'beard.type', label: t('Beard'), grid: GRIDS.beard, zoom: true },
      { kind: 'parts', path: 'beard.mustache.type', label: t('Mustache'), grid: GRIDS.beard, zoom: true },
      color('beard.color', t('Color')),
      size('beard.mustache.size', 8),
      yPos('beard.mustache.yPosition', 0, 16),
    ],
  },
  {
    key: 'glasses',
    name: t('Glasses'),
    fields: [
      { kind: 'parts', path: 'glasses.type', label: t('Frames'), grid: GRIDS.glasses, zoom: true },
      color('glasses.color', t('Color')),
      size('glasses.size', 7),
      yPos('glasses.yPosition', 0, 20),
    ],
  },
  {
    key: 'mole',
    name: t('Mole'),
    fields: [
      { kind: 'toggle', path: 'mole.on', label: t('Mole'), options: ['No', t('Yes')], values: [false, true] },
      size('mole.size', 8),
      { kind: 'slider', path: 'mole.xPosition', label: t('Horizontal'), min: 0, max: 16, low: t('Left'), high: t('Right') },
      { kind: 'slider', path: 'mole.yPosition', label: t('Vertical'), min: 0, max: 30, low: t('Up'), high: t('Down') },
    ],
  },
];

export function getAt(o: any, path: string): any {
  return path.split('.').reduce((x, k) => x?.[k], o);
}

export function setAt(o: any, path: string, v: unknown) {
  const keys = path.split('.');
  const last = keys.pop()!;
  const parent = keys.reduce((x, k) => (x[k] ??= {}), o);
  parent[last] = v;
}
