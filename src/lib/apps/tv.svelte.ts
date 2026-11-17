// TV (the Jellyfin client): the shelf starts with what you're in the middle of
// (A resumes it straight away), then a few random films and series (A opens them).
import { goto } from '$app/navigation';
import { api, backdropUrl, episodeLabel, img, logoUrl, progress, runtime, type Item } from '../jf';
import { play } from '../player.svelte';
import type { App, Slide } from './types';
import { t } from '$lib/i18n';

const shelf = $state({
  user: '',
  /** part-watched videos, then next episodes of series in progress */
  going: [] as { it: Item; resume: boolean }[],
  /** random picks, kept for the profile so a trip into an app and back shows the same */
  films: [] as Item[],
});

const MAX_GOING = 8;

function filmMeta(it: Item): string[] {
  const meta: string[] = [];
  if (it.Type === 'Series') meta.push(t('TV series'));
  if (it.ProductionYear) meta.push(String(it.ProductionYear));
  if (it.Type === 'Movie' && runtime(it)) meta.push(runtime(it));
  if (it.Genres?.length) meta.push(it.Genres.slice(0, 2).join(', '));
  if (it.CommunityRating) meta.push(`★ ${it.CommunityRating.toFixed(1)}`);
  return meta;
}

function filmSlide(it: Item): Slide {
  return {
    key: it.Id,
    title: it.Name,
    art: logoUrl(it, 36),
    meta: filmMeta(it),
    pill: it.OfficialRating,
    overview: it.Overview,
    backdrop: backdropUrl(it),
    action: t('Open'),
    open: () => goto(`/item/${it.Id}`),
  };
}

function goingSlide({ it, resume }: { it: Item; resume: boolean }): Slide {
  const ep = it.Type === 'Episode';
  const pct = resume ? progress(it) / 100 : 0;
  const meta = ep ? [`${episodeLabel(it)} · ${it.Name}`] : filmMeta(it).slice(0, 2);
  if (pct && it.RunTimeTicks) {
    const left = Math.max(1, Math.round(((1 - pct) * it.RunTimeTicks) / 600_000_000));
    meta.push(t('{left} min left', { left }));
  }
  return {
    key: `go:${it.Id}`,
    title: ep ? (it.SeriesName ?? it.Name) : it.Name,
    art:
      ep && it.ParentLogoItemId && it.ParentLogoImageTag
        ? img(it.ParentLogoItemId, 'Logo', 36, it.ParentLogoImageTag)
        : logoUrl(it, 36),
    kicker: { text: resume ? t('Continue watching') : t('Next episode') },
    meta,
    progress: pct || undefined,
    overview: it.Overview,
    backdrop: backdropUrl(it),
    action: resume ? t('Resume') : t('Watch'),
    open: () => play(it),
  };
}

export const tvApp: App = {
  key: 'tv',
  name: 'TV',
  description: t('The library\'s films and TV series, with Continue watching.'),
  route: '/tv',
  color: 'linear-gradient(135deg, #5b6cff 0%, #9b5cff 55%, #ff4fa3 100%)',
  icon: `<rect x="5" y="9" width="38" height="25" rx="4.5" />
    <path d="M17 40h14" />
    <path class="fill" d="M20.5 16.5v10l8.5-5z" />`,
  slides: () => [...shelf.going.map(goingSlide), ...shelf.films.map(filmSlide)],
  async load(user) {
    const [resume, next, films] = await Promise.all([
      api.resume(),
      api.nextUp(),
      shelf.user === user && shelf.films.length ? shelf.films : api.random(12).then((r) => r.Items),
    ]);
    // a series being resumed doesn't also show up as "next episode"
    const seen = new Set(resume.Items.map((x) => x.SeriesId ?? x.Id));
    const going = [
      ...resume.Items.map((it) => ({ it, resume: true })),
      ...next.Items.filter((x) => !seen.has(x.SeriesId ?? x.Id)).map((it) => ({ it, resume: false })),
    ].slice(0, MAX_GOING);
    shelf.user = user;
    shelf.going = going;
    shelf.films = films;
  },
};
