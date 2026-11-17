// Mii: the profile's Miis, made once and used on every console. The shelf
// shows them one by one, drawn whole.
import { goto } from '$app/navigation';
import { displayName, kinOf, lib, loadMiis, mainOf } from '../mii/library.svelte';
import { face } from '../mii/pics.svelte';
import type { App, Slide } from './types';
import { t } from '$lib/i18n';

export const miiApp: App = {
  key: 'mii',
  name: 'Mii',
  description: t('Your Miis: make them once and find them on Wii and Switch, with real names and family ties.'),
  route: '/mii',
  color: 'linear-gradient(135deg, #8fdcec 0%, #3aa6c9 55%, #2a7fb0 100%)',
  // a Mii's head and shoulders
  icon: `<circle cx="24" cy="18" r="8.5" />
    <path d="M10.5 40c1.5-7.5 7-11.5 13.5-11.5S36 32.5 37.5 40" />`,
  slides: () => {
    if (!lib.people.length)
      return [
        {
          key: 'empty',
          title: 'Mii',
          kicker: { text: t('Your Miis') },
          meta: [],
          overview: t('Add the Miis you already have on Switch and Wii, then find them in every game.'),
          backdrop: null,
          action: t('Open'),
          open: () => goto('/mii'),
        },
      ];
    return lib.people.slice(0, 12).map(
      (p): Slide => {
        const kin = kinOf(p).filter((k) => !k.inferred).slice(0, 3);
        const full = [p.names.first, p.names.surname].filter(Boolean).join(' ');
        const m = mainOf(p);
        return {
          key: p.id,
          title: displayName(p),
          kicker: { text: p.names.nickname ? t('Mii · {nickname}', { nickname: p.names.nickname }) : 'Mii' },
          cover: m ? face(m.nx) : null,
          meta: [full !== displayName(p) && full, ...kin.map((k) => `${k.label}: ${displayName(k.person)}`)].filter(Boolean) as string[],
          backdrop: null,
          action: t('Open'),
          open: () => goto(`/mii/${p.id}`),
        };
      },
    );
  },
  load: (user) => loadMiis(user),
};
