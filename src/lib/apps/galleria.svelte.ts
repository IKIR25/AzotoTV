// Galleria: the shelf shows "Accadde oggi" first (photos taken on this date in
// earlier years), then the albums shared with the TV, most recently added to first.
import { goto } from '$app/navigation';
import { albumCount, counts, gallery, loadGallery, longDate, memoryRoute, pic, span, yearsAgo } from '../gallery.svelte';
import type { App, Slide } from './types';

export const galleriaApp: App = {
  key: 'galleria',
  name: 'Galleria',
  description: 'Le foto e i video di famiglia: gli album condivisi con la TV, «Accadde oggi» e le presentazioni.',
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
        kicker: { text: `Accadde oggi · ${yearsAgo(m.year)}` },
        meta: [counts(m.assets)],
        backdrop: pic(first.id, 'preview'),
        action: 'Guarda',
        open: () => goto(memoryRoute(m)),
      };
    });
    const albums = gallery.albums.slice(0, 8).map(
      (a): Slide => ({
        key: `a:${a.id}`,
        title: a.name,
        kicker: { text: 'Album' },
        meta: [albumCount(a), span(a), a.owner ? `Condiviso da ${a.owner}` : ''].filter(Boolean),
        overview: a.description || undefined,
        backdrop: a.cover ? pic(a.cover, 'preview') : null,
        action: 'Apri',
        open: () => goto(`/galleria/album/${a.id}`),
      }),
    );
    if (memories.length || albums.length) return [...memories, ...albums];
    return [
      {
        key: 'empty',
        title: 'Galleria',
        kicker: { text: 'Foto di famiglia' },
        meta: [],
        overview: gallery.enabled
          ? "Dall'app Immich (telefono o computer) condividi un album con l'utente «TV»: comparirà qui."
          : 'La Galleria non è ancora collegata al server delle foto.',
        backdrop: null,
        action: 'Apri',
        open: () => goto('/galleria'),
      },
    ];
  },
  load: () => loadGallery(),
};
