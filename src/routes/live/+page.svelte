<script lang="ts">
  import { t } from '$lib/i18n';
  import { onMount, tick } from 'svelte';
  import { page } from '$app/state';
  import { img, type Item } from '$lib/jf';
  import { stage } from '$lib/stage.svelte';
  import { restoreFocus } from '$lib/nav';
  import { hhmm, live, loadLive, ms, nowNext, onAir, watch } from '$lib/live.svelte';
  import { hasReminder, toggleReminder } from '$lib/notify.svelte';
  import { itemPin, pinOnHold } from '$lib/pins.svelte';
  import { key } from '$lib/input.svelte';

  // Live TV: channels on the left (what's on now, how far in), the focused
  // channel on the right (programme on air, description, what follows).
  // A watches the channel; in the player ▲ ▼ change channel. ▶ goes to what
  // follows, where A sets a reminder (a notification a minute before it starts).

  let sel = $state<Item | null>(null);
  let failed = $state('');
  let ready = $state(false);

  const cur = $derived(sel ?? live.channels[0] ?? null);
  const info = $derived(cur ? nowNext(cur) : { now: undefined, next: [] as Item[] });
  const logo = (ch: Item, rem: number) =>
    ch.ImageTags?.Primary ? img(ch.Id, 'Primary', rem, ch.ImageTags.Primary) : null;
  const picture = (p: Item | undefined, rem: number) =>
    p?.ImageTags?.Primary ? img(p.Id, 'Primary', rem, p.ImageTags.Primary) : null;
  const left = (p: Item) => Math.max(0, Math.round((ms(p.EndDate) - live.clock) / 60000));

  // ▶ from a channel goes to what follows on it, ◀ comes back: the list sits
  // lower on screen, too far down for the spatial nav to call it "the same row".
  function jump(node: HTMLElement, [dir, target]: ['left' | 'right', () => HTMLElement | null]) {
    const h = (e: Event) => {
      const t = (e as CustomEvent<string>).detail === dir ? target() : null;
      if (!t) return;
      e.preventDefault();
      t.focus({ preventScroll: true });
    };
    node.addEventListener('navmove', h);
    return { destroy: () => node.removeEventListener('navmove', h) };
  }
  const firstProg = () => document.querySelector<HTMLElement>('.prog');
  const curChannel = () => (cur ? document.querySelector<HTMLElement>(`.ch[data-key="${CSS.escape(cur.Id)}"]`) : null);

  function remind(ch: Item, p: Item) {
    void toggleReminder({
      program_id: p.Id,
      channel_id: ch.Id,
      channel: ch.Name ?? '',
      name: p.Name ?? '',
      episode: p.EpisodeTitle ?? null,
      start: ms(p.StartDate),
      image: ch.ImageTags?.Primary ? img(ch.Id, 'Primary', 8, ch.ImageTags.Primary).replace('jf://localhost', '') : null,
    });
  }

  $effect(() => {
    stage.backdrop = picture(info.now, 40) ?? (cur ? logo(cur, 40) : null);
    stage.ambient = true;
  });

  onMount(async () => {
    try {
      await loadLive();
    } catch (e) {
      failed = String(e);
    }
    ready = true;
    await tick();
    restoreFocus(page.url.pathname, '.ch');
  });
</script>

<header class="head">{t('Live TV')} <span class="dot">●</span></header>

{#if failed}
  <p class="msg">{t('Couldn\'t load the channels: {failed}', { failed })}</p>
{:else if ready && !live.channels.length}
  <p class="msg">{t('No channels available.')}</p>
{:else}
  <div class="live">
    <section class="list">
      <div class="scroller">
        {#each live.channels as ch (ch.Id)}
          {@const n = nowNext(ch)}
          <button
            class="ch"
            data-focusable
            data-key={ch.Id}
            use:jump={['right', firstProg]}
            use:pinOnHold={itemPin(ch)}
            onfocus={() => (sel = ch)}
            onclick={() => watch(ch)}
          >
            <span class="num">{ch.ChannelNumber}</span>
            <span class="logo">{#if logo(ch, 8)}<img src={logo(ch, 8)} alt="" loading="lazy" />{/if}</span>
            <span class="txt">
              <b>{n.now?.Name ?? ch.Name}</b>
              <small>{ch.Name}{n.now ? t(' · until {time}', { time: hhmm(n.now.EndDate) }) : ''}</small>
              {#if n.now}<i class="bar"><em style="transform:scaleX({onAir(n.now)})"></em></i>{/if}
            </span>
          </button>
        {/each}
      </div>
    </section>

    {#if cur}
      <section class="detail glass">
        <div class="pic">
          {#if picture(info.now, 30)}
            <img src={picture(info.now, 30)} alt="" />
          {:else if logo(cur, 16)}
            <img class="lg" src={logo(cur, 16)} alt="" />
          {/if}
        </div>
        <p class="chan">{cur.ChannelNumber} · {cur.Name} <span class="dot">{t('● Live')}</span></p>
        {#if info.now}
          <h1>{info.now.Name}</h1>
          {#if info.now.EpisodeTitle}<p class="ep">{info.now.EpisodeTitle}</p>{/if}
          <p class="slot">
            {t('{hhmm} – {hhmm2} · {left} min left', { hhmm: hhmm(info.now.StartDate), hhmm2: hhmm(info.now.EndDate), left: left(info.now) })}
          </p>
          <i class="bar big"><em style="transform:scaleX({onAir(info.now)})"></em></i>
          {#if info.now.Overview}<p class="ov">{info.now.Overview}</p>{/if}
        {:else}
          <h1>{cur.Name}</h1>
          <p class="slot">{t('No guide for this channel')}</p>
        {/if}
        {#if info.next.length}
          <h2>{t('Up next')}</h2>
          <ul class="next">
            {#each info.next.slice(0, 5) as p (p.Id)}
              <li>
                <button class="prog" data-focusable use:jump={['left', curChannel]} onclick={() => remind(cur, p)}>
                  <time>{hhmm(p.StartDate)}</time><span>{p.Name}</span>
                  {#if hasReminder(p.Id)}
                    <svg class="bell" viewBox="0 0 24 24" aria-label={t('reminder')}
                      ><path d="M6 16V11a6 6 0 1 1 12 0v5l1.5 2h-15z" /><path d="M10 20.5a2 2 0 0 0 4 0" /></svg
                    >
                  {/if}
                  <em class="act">{key('A')} {hasReminder(p.Id) ? t('remove reminder') : t('remind me')}</em>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
        <p class="hint">{t('{a} watch · ▶ reminder · in the player ▲ ▼ change channel', { a: key('A') })}</p>
      </section>
    {/if}
  </div>
{/if}

<style>
  /* Live TV is its own app: no TV top bar, a title level with the clock corner */
  .head {
    position: absolute; top: 3rem; left: 6rem; height: 5.6rem; display: flex; align-items: center; gap: 1rem;
    font-size: 2.6rem; font-weight: 700; letter-spacing: -0.01em;
  }
  .head .dot { font-size: 1.4rem; }
  .live { position: absolute; inset: 0; display: flex; gap: 3rem; padding: 10.5rem 6rem 3rem; }
  .list { position: relative; flex: 0 0 50rem; }
  .list .scroller { padding: 0.6rem 0.6rem 2rem; }
  .ch {
    width: 100%; display: flex; align-items: center; gap: 1.4rem; padding: 0.8rem 1.2rem;
    border-radius: 1.4rem; text-align: left;
  }
  .ch:focus { background: rgb(var(--fg-rgb) / 0.92); color: var(--focus-text); }
  .ch:focus small, .ch:focus .num { color: rgb(var(--focus-text-rgb) / 0.65); }
  .num { flex: 0 0 3.2rem; text-align: right; font-size: 1.4rem; font-weight: 700; color: var(--muted); font-variant-numeric: tabular-nums; }
  .logo {
    flex: 0 0 8rem; height: 4.5rem; border-radius: 0.8rem; display: grid; place-items: center;
    background: rgb(var(--fg-rgb) / 0.08); overflow: hidden;
  }
  .logo img { max-width: 85%; max-height: 80%; object-fit: contain; }
  .txt { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 0.2rem; }
  .txt b { font-size: 1.4rem; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .txt small { font-size: 1.1rem; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .bar { display: block; height: 0.35rem; border-radius: 0.2rem; background: rgb(var(--fg-rgb) / 0.2); overflow: hidden; margin-top: 0.3rem; }
  .ch:focus .bar { background: rgb(var(--focus-text-rgb) / 0.18); }
  .bar em { display: block; height: 100%; background: var(--accent); transform-origin: left; }
  .bar.big { height: 0.5rem; margin: 1rem 0 0; }

  .detail { flex: 1; min-width: 0; border-radius: 2.2rem; padding: 2.4rem 2.8rem; display: flex; flex-direction: column; overflow: hidden; }
  /* a long overview or schedule gets cut at the bottom; nothing above it shrinks (the title used to vanish) */
  .detail > * { flex-shrink: 0; }
  .pic { height: 22rem; display: flex; align-items: center; margin-bottom: 1.6rem; flex-shrink: 0; }
  .pic img { max-width: 100%; max-height: 22rem; border-radius: 1.2rem; box-shadow: 0 1.6rem 3rem rgb(var(--shadow-rgb) / 0.45); }
  .pic img.lg { max-height: 10rem; box-shadow: none; }
  .chan { font-size: 1.3rem; color: var(--muted); display: flex; gap: 1.2rem; align-items: center; }
  .dot { color: var(--danger); font-weight: 600; }
  h1 {
    font-size: 2.8rem; font-weight: 700; line-height: 1.15; margin-top: 0.4rem;
    display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
  }
  .ep { font-size: 1.5rem; color: var(--muted); margin-top: 0.2rem; }
  .slot { font-size: 1.3rem; color: var(--muted); margin-top: 0.8rem; }
  .ov {
    font-size: 1.3rem; line-height: 1.5; color: rgb(var(--text-rgb) / 0.82); margin-top: 1.2rem;
    display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden;
  }
  h2 { font-size: 1.4rem; font-weight: 600; margin-top: 1.8rem; color: var(--muted); text-transform: uppercase; letter-spacing: 0.12em; }
  .next { list-style: none; padding: 0; margin: 0.4rem -1rem 0; display: flex; flex-direction: column; gap: 0.1rem; }
  .prog {
    width: 100%; display: flex; align-items: center; gap: 1.4rem; padding: 0.45rem 1rem; border-radius: 0.9rem;
    font-size: 1.35rem; white-space: nowrap; text-align: left;
  }
  .next time { color: var(--muted); font-variant-numeric: tabular-nums; flex: none; }
  .next span { overflow: hidden; text-overflow: ellipsis; }
  .bell { flex: none; width: 1.6rem; height: 1.6rem; fill: none; stroke: var(--accent-soft); stroke-width: 2; stroke-linecap: round; stroke-linejoin: round; }
  .act { display: none; margin-left: auto; padding-left: 1rem; font-style: normal; font-size: 1.1rem; color: rgb(var(--focus-text-rgb) / 0.6); flex: none; }
  .prog:focus { background: rgb(var(--fg-rgb) / 0.92); color: var(--focus-text); }
  .prog:focus time { color: rgb(var(--focus-text-rgb) / 0.65); }
  .prog:focus .bell { stroke: var(--accent-on-focus); }
  .prog:focus .act { display: block; }
  .hint { margin-top: auto; padding-top: 1.2rem; font-size: 1.1rem; color: var(--muted); }
  .msg { position: absolute; top: 12rem; left: 6rem; font-size: 1.8rem; color: var(--muted); }
</style>
