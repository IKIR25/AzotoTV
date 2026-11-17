// The 12 favourite colours (shirt colour), as the Wii and the Switch show them.
import { t } from '$lib/i18n';
export const FAVORITE = [
  { name: t('Red'), hex: '#d21e14' },
  { name: t('Orange'), hex: '#ff6e19' },
  { name: t('Yellow'), hex: '#ffd820' },
  { name: t('Light green'), hex: '#78d220' },
  { name: t('Green'), hex: '#00782d' },
  { name: t('Blue'), hex: '#0a48b4' },
  { name: t('Light blue'), hex: '#3caade' },
  { name: t('Pink'), hex: '#f55a7d' },
  { name: t('Purple'), hex: '#7328ad' },
  { name: t('Brown'), hex: '#483818' },
  { name: t('White'), hex: '#e0e0e0' },
  { name: t('Black'), hex: '#181814' },
];

export const favorite = (i: number) => FAVORITE[i] ?? FAVORITE[0];

// The Switch's colours (nn::mii CommonColorTable, sRGB, from the FFL port's
// FFLiColor.cpp): hair, eyebrows, beard, eyes, mouth and glasses pick from
// these 100. The Wii's own colours are among them (WII_COLORS).
export const COMMON = [
  '#2d2828', '#402010', '#5c180a', '#7c3a14', '#787880', '#4e3e10', '#885818', '#d0a04a', '#000000', '#6c7070',
  '#663c2c', '#605e30', '#4654a8', '#387058', '#603810', '#a81008', '#203068', '#a86000', '#787068', '#d85208',
  '#f00c08', '#f54848', '#f09a74', '#8c5040', '#842626', '#ff7366', '#ffa6a6', '#ffc0ba', '#732e3b', '#991f3d',
  '#8a173e', '#b53e42', '#c71e56', '#b05381', '#c7546e', '#fa7597', '#fcacc9', '#ffc9d8', '#311c40', '#37283d',
  '#4c184d', '#6f42b3', '#855cb8', '#c083cc', '#a893c9', '#c5ace6', '#eebefa', '#d2c5ed', '#191f40', '#123f66',
  '#2a82d4', '#57b4f2', '#7ac5de', '#89a6fa', '#84bdfa', '#a1e3ff', '#0b2e36', '#013d3b', '#0d4f59', '#236663',
  '#307e8c', '#4faeb0', '#7ac49e', '#7fd4c0', '#87e5b6', '#0a4a35', '#437a00', '#027562', '#369970', '#4bad1a',
  '#92bf0a', '#63c788', '#9ee042', '#96de7e', '#bbf2aa', '#99932b', '#a69563', '#ccc039', '#ccb987', '#d9cc82',
  '#d5d96f', '#d5e683', '#d8fa9d', '#7d4500', '#e6bb7a', '#fee24a', '#fade82', '#f7ea9c', '#faf89b', '#a64d1e',
  '#ff960d', '#d19b69', '#ffb266', '#ffc28c', '#e5cfb1', '#414141', '#9b9b9b', '#bebebe', '#dcd7cd', '#ffffff',
];

/** Skin (faceline) colours: the Switch has 10, the Wii the first 6. */
export const SKIN = ['#ffd3ad', '#ffb66b', '#de7942', '#ffaa8c', '#ad5129', '#632c18', '#ffbea5', '#ffc58f', '#8c3c23', '#3c2d23'];

/** The common colours each part had on the Wii (what they become there). */
export const WII_COLORS = {
  hair: [0, 1, 2, 3, 4, 5, 6, 7],
  eyes: [8, 9, 10, 11, 12, 13],
  mouth: [19, 20, 21, 22, 23],
  glasses: [8, 14, 15, 16, 17, 18],
};
