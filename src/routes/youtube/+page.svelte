<script lang="ts">
  import { t } from '$lib/i18n';
  import { tick } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api } from '$lib/jf';
  import { auth } from '$lib/session.svelte';
  import { refresh, stage } from '$lib/stage.svelte';
  import { restoreFocus } from '$lib/nav';
  import Card from '$lib/Card.svelte';
  import Row from '$lib/Row.svelte';
  import Picker from '$lib/Picker.svelte';
  import {
    account, channelTile, continueWatching, feed, loadYoutube, me, newFromSubs, recTile, resetProfile, setShortsLimit, shortsLeft,
    shortsLimit, shortsPool, shortTile, SHORTS_CHOICES, SHORTS_DEFAULT, videoTile, ytImage,
  } from '$lib/youtube.svelte';

  // The YouTube app: rows of what this profile follows and might like (and
  // why: "Perché hai guardato…"), Shorts last and only as many as are left for
  // today. A on a video opens its page. With a Google account linked, its
  // home and subscriptions come first and AzotoTV's own picks follow in one row.
  let admin = $state(false);
  let sheet = $state(false);
  // "Ricomincia da zero" asks for a second A within a few seconds (no dialog)
  let confirmReset = $state(false);
  let resetTimer: ReturnType<typeof setTimeout>;
  function reset() {
    clearTimeout(resetTimer);
    if (!confirmReset) {
      confirmReset = true;
      resetTimer = setTimeout(() => (confirmReset = false), 4000);
      return;
    }
    confirmReset = false;
    resetProfile();
    loadYoutube(auth.id, true);
  }
  $effect(() => () => clearTimeout(resetTimer));

  $effect(() => {
    const user = auth.id;
    void refresh.n;
    stage.backdrop = null;
    loadYoutube(user).then(async () => {
      await tick();
      restoreFocus(page.url.pathname, '.rows [data-focusable]');
    });
    api.me().then((u) => (admin = !!u.Policy?.IsAdministrator), () => (admin = false));
  });

  const cont = $derived(feed.user === auth.id ? continueWatching().slice(0, 20) : []);
  const left = $derived(shortsLeft());
  const linked = $derived(account.user === auth.id && !!account.info);
  const subsRow = $derived(newFromSubs());
  const shorts = $derived(shortsPool().filter((v) => !me.shorts.seen.includes(v.id)).slice(0, left));
  const firstTime = $derived(!me.history.length && !me.subs.length && !linked);
  let photoOk = $state(true);
  const limitLabel = (n: number) => (n ? t('{n} a day', { n }) : t('None'));

  async function closeSheet() {
    sheet = false;
    await tick();
    document.querySelector<HTMLElement>('[data-key="shorts-limit"]')?.focus({ preventScroll: true });
  }
</script>

<div class="scroller">
  <header>
    <h1>YouTube</h1>
    <div class="row">
      <button class="btn primary" data-focusable data-key="search" data-scroll-top onclick={() => goto('/youtube/search')}>
        <svg viewBox="0 0 24 24"><path d="M10 3a7 7 0 0 1 5.6 11.2l5.1 5.1-1.4 1.4-5.1-5.1A7 7 0 1 1 10 3zm0 2a5 5 0 1 0 0 10 5 5 0 0 0 0-10z" /></svg>
        {t('Search')}
      </button>
      <button class="btn chip acc" data-focusable data-key="account" data-scroll-top onclick={() => goto('/youtube/account')}>
        {#if linked && account.info}
          {#if account.info.photo && photoOk}<img src={ytImage(account.info.photo)} alt="" onerror={() => (photoOk = false)} />{/if}
          {account.info.name || t('Google account')}
        {:else}
          <svg viewBox="0 0 24 24"><path d="M12 12a4.5 4.5 0 1 0 0-9 4.5 4.5 0 0 0 0 9zm0 2c-4.4 0-8 2.2-8 5v2h16v-2c0-2.8-3.6-5-8-5z" /></svg>
          {t('Sign in with Google')}
        {/if}
      </button>
      {#if admin}
        <button class="btn chip" data-focusable data-key="shorts-limit" data-scroll-top onclick={() => (sheet = true)}>
          <span class="k">Shorts</span>
          {limitLabel(shortsLimit.n)}
        </button>
      {/if}
      {#if !firstTime || me.later.length || me.liked.length}
        <button class="btn chip" class:warn={confirmReset} data-focusable data-key="reset" data-scroll-top onclick={reset}>
          {confirmReset ? t('Press again: erases subscriptions, history, likes and Watch later') : t('↺ Start from scratch')}
        </button>
      {/if}
    </div>
  </header>

  <div class="rows">
    {#if firstTime}
      <p class="msg">{t('Search for a channel you like and subscribe: the recommendations learn from what you watch on this profile.')}</p>
    {/if}
    {#if cont.length}
      <Row title={t('Continue watching')}>
        {#each cont as v (v.id)}<Card wide size={22} tile={videoTile(v, 'c')} />{/each}
      </Row>
    {/if}
    {#if linked}
      {#if account.error && !account.home.length}
        <p class="msg small">{t('The Google account isn\'t answering ({error}): AzotoTV\'s recommendations meanwhile.', { error: account.error })}</p>
      {:else if account.loading && !account.home.length}
        <p class="msg">{t('Loading your account\'s home…')}</p>
      {/if}
      {#each account.home.slice(0, 1) as s, i (`a${i}`)}
        <Row title={s.title || t('Recommended for you')}>
          {#each s.videos as v (v.id)}<Card wide size={22} tile={videoTile(v, `a${i}`)} />{/each}
        </Row>
      {/each}
    {/if}
    {#if subsRow.length}
      <Row title={t('New from your subscriptions')}>
        {#each subsRow as v (v.id)}<Card wide size={22} tile={videoTile(v, 's')} />{/each}
      </Row>
    {/if}
    {#if linked}
      {#each account.home.slice(1) as s, i (`a${i + 1}`)}
        <Row title={s.title || t('More for you')}>
          {#each s.videos as v (v.id)}<Card wide size={22} tile={videoTile(v, `a${i + 1}`)} />{/each}
        </Row>
      {/each}
      {#if feed.recommended.length}
        <Row title={t('Picked by AzotoTV')}>
          {#each feed.recommended as v (v.id)}<Card wide size={22} tile={recTile(v, 'r')} />{/each}
        </Row>
      {/if}
    {:else if feed.recommended.length}
      <Row title={firstTime ? t('To discover this week') : t('Recommended for you')}>
        {#each feed.recommended as v (v.id)}<Card wide size={22} tile={recTile(v, 'r')} />{/each}
      </Row>
      {#each feed.more as m, i (`${i}:${m.title}`)}
        <Row title={m.title}>
          {#each m.videos as v (v.id)}<Card wide size={22} tile={recTile(v, `m${i}`)} />{/each}
        </Row>
      {/each}
    {:else if feed.loading}
      <p class="msg">{t('Loading the videos…')}</p>
    {:else if feed.user === auth.id && !subsRow.length}
      <p class="msg">{t('YouTube isn\'t answering. Try again in a moment.')}</p>
    {/if}
    {#if me.later.length}
      <Row title={t('Watch later')}>
        {#each me.later as v (v.id)}<Card wide size={22} tile={videoTile(v, 'l')} />{/each}
      </Row>
    {/if}
    {#if me.subs.length}
      <Row title={t('Subscriptions')}>
        {#each me.subs as c (c.id)}<Card size={11} tile={channelTile(c, 'ch')} />{/each}
      </Row>
    {/if}
    {#if shortsLimit.n > 0}
      {#if shorts.length}
        <Row title={left === 1 ? t('Shorts · 1 left for today') : t('Shorts · {left} left for today', { left })}>
          {#each shorts as v (v.id)}<Card size={12} tile={shortTile(v, 'sh')} />{/each}
        </Row>
      {:else if !left}
        <p class="msg small">{t('No Shorts left for today: they come back tomorrow.')}</p>
      {/if}
    {/if}
    <div class="end"></div>
  </div>
</div>

{#if sheet}
  <Picker
    title={t('Shorts a day per profile')}
    options={SHORTS_CHOICES.map((n) => ({ key: n, main: limitLabel(n), sub: n === SHORTS_DEFAULT ? 'predefinito' : undefined }))}
    value={shortsLimit.n}
    onpick={(n) => {
      setShortsLimit(n);
      closeSheet();
    }}
    onclose={closeSheet}
  />
{/if}

<style>
  header { padding: 3.8rem 6rem 1rem; }
  h1 { font-size: 3.6rem; font-weight: 700; letter-spacing: -0.02em; }
  .row { display: flex; align-items: center; gap: 1.6rem; margin-top: 1.2rem; }
  .row .btn.primary { font-size: 1.3rem; padding: 0.7rem 1.6rem; }
  .row svg { width: 1.6rem; height: 1.6rem; fill: currentColor; }
  .chip { font-size: 1.3rem; font-weight: 500; padding: 0.7rem 1.5rem; border-radius: 1.4rem; background: rgba(255, 255, 255, 0.1); }
  .chip .k { font-weight: 700; margin-right: 0.3rem; }
  .acc { display: flex; align-items: center; gap: 0.7rem; }
  .acc img { width: 2.2rem; height: 2.2rem; border-radius: 50%; object-fit: cover; margin-left: -0.6rem; }
  .chip.warn { background: #e0162b; color: #fff; }
  .chip.warn:focus { background: #ff4e45; color: #fff; }
  .rows { padding-top: 1.4rem; }
  .msg { margin: 0 6rem 2rem; font-size: 1.6rem; color: var(--muted); max-width: 80rem; line-height: 1.5; }
  .msg.small { font-size: 1.4rem; }
  .end { height: 8rem; }
</style>
