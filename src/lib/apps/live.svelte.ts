// Diretta TV: the shelf shows what's on now, channel by channel; A starts the channel.
import { img, type Item } from '../jf';
import { hhmm, live, loadLive, nowNext, onAir, watch } from '../live.svelte';
import type { App, Slide } from './types';
import { t } from '$lib/i18n';

function channelSlide(ch: Item, at: number): Slide {
  const { now, next } = nowNext(ch, at);
  const logo = (rem: number) => (ch.ImageTags?.Primary ? img(ch.Id, 'Primary', rem, ch.ImageTags.Primary) : null);
  const pic = now?.ImageTags?.Primary ? img(now.Id, 'Primary', 120, now.ImageTags.Primary) : null;
  const meta = now ? [`${hhmm(now.StartDate)} – ${hhmm(now.EndDate)}`] : [t('No guide available')];
  if (next[0]) meta.push(t('Then {a} · {b}', { a: hhmm(next[0].StartDate), b: next[0].Name }));
  return {
    key: ch.Id,
    title: now?.Name ?? ch.Name,
    kicker: { icon: logo(6), text: `${ch.ChannelNumber ?? ''} · ${ch.Name}` },
    live: true,
    meta,
    overview: now?.Overview,
    progress: now ? onAir(now, at) : undefined,
    backdrop: pic ?? logo(40),
    ambient: !pic,
    action: t('Watch'),
    open: () => watch(ch),
  };
}

export const liveApp: App = {
  key: 'live',
  name: t('Live TV'),
  description: t('Live TV channels, with the programme guide.'),
  route: '/live',
  color: 'linear-gradient(135deg, #ff5d6c 0%, #ff7a45 55%, #ffb03a 100%)',
  icon: `<circle class="fill" cx="24" cy="24" r="4" />
    <path d="M16.6 16.6a10.5 10.5 0 0 0 0 14.8M31.4 16.6a10.5 10.5 0 0 1 0 14.8" />
    <path d="M10.6 10.6a19 19 0 0 0 0 26.8M37.4 10.6a19 19 0 0 1 0 26.8" />`,
  // channels that have something on air (all of them if there's no guide)
  slides: () => {
    const at = live.clock;
    const on = live.channels.filter((ch) => nowNext(ch, at).now);
    return (on.length ? on : live.channels).map((ch) => channelSlide(ch, at));
  },
  load: () => loadLive(),
};
