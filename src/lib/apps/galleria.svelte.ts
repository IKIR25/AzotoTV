// Galleria: the shelf shows "Accadde oggi" first (photos taken on this date in
// earlier years), then the albums shared with the TV, most recently added to first.
import { goto } from '$app/navigation';
import { albumCount, counts, gallery, loadGallery, longDate, memoryRoute, pic, span, yearsAgo } from '../gallery.svelte';
import type { App, Slide } from './types';
import { t } from '$lib/i18n';

export const galleriaApp: App = {
  key: 'galleria',
  name: t('Gallery'),
  description: t('The family\'s photos and videos: the albums shared with the TV, “On this day” and slideshows.'),
  route: '/galleria',
  color: 'linear-gradient(135deg, #ffd25e 0%, #ff8a5c 50%, #e8457c 100%)',
  // a picture frame: sun and hills
  icon: `<rect x="7" y="10" width="34" height="28" rx="5" />
    <circle class="fill" cx="17.5" cy="19.5" r="3" />
    <path d="M8.5 33.5l10-9.5 7 6.5 5-4.5 9.5 8.5" />`,
  slides: () => {
    const memories = gallery.memories.slice(0, 4).map((m): Slide => {
      const first = m.assets.find((a) => !a.video) ?? m.assets[0];
      return {
        key: `m:${m.year}`,
        title: longDate(first.taken),
        kicker: { text: t('On this day · {yearsAgo}', { yearsAgo: yearsAgo(m.year) }) },
        meta: [counts(m.assets)],
        backdrop: pic(first.id, 'preview'),
        action: t('Watch'),
        open: () => goto(memoryRoute(m)),
      };
    });
    const albums = gallery.albums.slice(0, 8).map(
      (a): Slide => ({
        key: `a:${a.id}`,
        title: a.name,
        kicker: { text: 'Album' },
        meta: [albumCount(a), span(a), a.owner ? t('Shared by {owner}', { owner: a.owner }) : ''].filter(Boolean),
        overview: a.description || undefined,
        backdrop: a.cover ? pic(a.cover, 'preview') : null,
        action: t('Open'),
        open: () => goto(`/galleria/album/${a.id}`),
      }),
    );
    if (memories.length || albums.length) return [...memories, ...albums];
    return [
      {
        key: 'empty',
        title: t('Gallery'),
        kicker: { text: t('Family photos') },
        meta: [],
        overview: gallery.enabled
          ? t('From the Immich app (phone or computer), share an album with the “TV” user: it will show up here.')
          : t('Gallery isn\'t connected to the photo server yet.'),
        backdrop: null,
        action: t('Open'),
        open: () => goto('/galleria'),
      },
    ];
  },
  load: () => loadGallery(),
};
