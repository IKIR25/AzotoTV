// The Store: always on the home. Its shelf offers the apps this profile
// doesn't have yet; A on a slide adds that one straight away.
import { goto } from '$app/navigation';
import { auth } from '../session.svelte';
import { showToast } from '../stage.svelte';
import { APPS } from './index';
import { installed, setInstalled } from './installed.svelte';
import type { App, Slide } from './types';
import { t } from '$lib/i18n';

export function install(a: App) {
  setInstalled(auth.id, a.key, true);
  showToast(t('{name} added to Home', { name: a.name }));
}

export function uninstall(a: App) {
  setInstalled(auth.id, a.key, false);
  showToast(t('{name} removed from Home', { name: a.name }));
}

export const storeApp: App = {
  key: 'store',
  name: 'Store',
  description: t('Add apps to your Home or remove them.'),
  fixed: true,
  route: '/store',
  color: 'linear-gradient(135deg, #5ee7df 0%, #3a8dff 55%, #6a4cff 100%)',
  icon: `<path d="M11 17h26l-2.2 21.2A3 3 0 0 1 31.8 41H16.2a3 3 0 0 1-3-2.8z" />
    <path d="M18 21v-6a6 6 0 0 1 12 0v6" />`,
  slides: () => {
    const have = installed(auth.id);
    const missing = APPS.filter((a) => !have.includes(a.key));
    if (!missing.length) {
      return [
        {
          key: 'all',
          title: t('You already have every app'),
          kicker: { text: 'Store' },
          meta: [],
          overview: t('In the Store you can also remove from Home the ones you don\'t use.'),
          backdrop: null,
          action: t('Open the Store'),
          open: () => goto('/store'),
        },
      ];
    }
    return missing.map(
      (a): Slide => ({
        key: a.key,
        title: a.name,
        kicker: { text: t('Store · to add') },
        meta: [],
        overview: a.description,
        backdrop: null,
        action: t('Add to Home'),
        open: () => install(a),
      }),
    );
  },
};
