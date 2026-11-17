// YouTube: the shelf carries on with the videos left halfway, then what's new
// from the subscriptions, then recommendations (the Google account's, when the
// profile has one linked); A plays them straight away. Never Shorts.
import { account, continueWatching, feed, loadYoutube, newFromSubs, resumeAt, shortViews, thumb, watch, when, type Video } from '../youtube.svelte';
import type { App, Slide } from './types';
import { t } from '$lib/i18n';

function slide(v: Video, kind: 'c' | 's' | 'r'): Slide {
  const pos = resumeAt(v.id);
  return {
    key: `${kind}:${v.id}`,
    title: v.title,
    kicker: { text: kind === 'c' ? t('Continue watching') : kind === 's' ? t('New · {channel}', { channel: v.channel }) : t('Recommended · {channel}', { channel: v.channel }) },
    meta: [shortViews(v.views), when(v)].filter(Boolean),
    progress: kind === 'c' && v.length ? pos / v.length : undefined,
    backdrop: thumb(v.id, 'big'),
    action: kind === 'c' ? t('Resume') : t('Watch'),
    open: () => watch(v),
  };
}

export const youtubeApp: App = {
  key: 'youtube',
  name: 'YouTube',
  description: t('YouTube videos and channels, with no account or with yours. Shorts with a daily limit.'),
  route: '/youtube',
  color: 'linear-gradient(135deg, #ff4e45 0%, #e0162b 55%, #a3001b 100%)',
  // a filled rounded screen with the play mark cut out (the TV tile's is an outline)
  icon: `<path class="fill" fill-rule="evenodd" d="M14 11h20a8 8 0 0 1 8 8v10a8 8 0 0 1-8 8H14a8 8 0 0 1-8-8V19a8 8 0 0 1 8-8zM20.5 17.5v13l11-6.5z" />`,
  slides: () => {
    const cont = continueWatching().slice(0, 2);
    const seen = new Set(cont.map((v) => v.id));
    const subs = newFromSubs().filter((v) => !seen.has(v.id)).slice(0, 4);
    subs.forEach((v) => seen.add(v.id));
    const picks = account.info && account.home.length ? account.home[0].videos : feed.recommended;
    const rec = picks.filter((v) => !seen.has(v.id)).slice(0, 6);
    return [...cont.map((v) => slide(v, 'c')), ...subs.map((v) => slide(v, 's')), ...rec.map((v) => slide(v, 'r'))];
  },
  load: (user) => loadYoutube(user),
};
