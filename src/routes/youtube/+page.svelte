<script lang="ts">
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
  const limitLabel = (n: number) => (n ? `${n} al giorno` : 'Nessuno');

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
        Cerca
      </button>
      <button class="btn chip acc" data-focusable data-key="account" data-scroll-top onclick={() => goto('/youtube/account')}>
        {#if linked && account.info}
          {#if account.info.photo && photoOk}<img src={ytImage(account.info.photo)} alt="" onerror={() => (photoOk = false)} />{/if}
          {account.info.name || 'Account Google'}
        {:else}
          <svg viewBox="0 0 24 24"><path d="M12 12a4.5 4.5 0 1 0 0-9 4.5 4.5 0 0 0 0 9zm0 2c-4.4 0-8 2.2-8 5v2h16v-2c0-2.8-3.6-5-8-5z" /></svg>
          Accedi con Google
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
          {confirmReset ? 'Premi di nuovo: cancella iscrizioni, cronologia, Mi piace e Più tardi' : '↺ Ricomincia da zero'}
        </button>
      {/if}
    </div>
  </header>

  <div class="rows">
    {#if firstTime}
      <p class="msg">Cerca un canale che ti piace e iscriviti: i consigli imparano da quello che guardi su questo profilo.</p>
    {/if}
    {#if cont.length}
      <Row title="Continua a guardare">
        {#each cont as v (v.id)}<Card wide size={22} tile={videoTile(v, 'c')} />{/each}
      </Row>
    {/if}
    {#if linked}
      {#if account.error && !account.home.length}
        <p class="msg small">L'account Google non risponde ({account.error}): intanto i consigli di AzotoTV.</p>
      {:else if account.loading && !account.home.length}
        <p class="msg">Carico la home del tuo account…</p>
      {/if}
      {#each account.home.slice(0, 1) as s, i (`a${i}`)}
        <Row title={s.title || 'Consigliati per te'}>
          {#each s.videos as v (v.id)}<Card wide size={22} tile={videoTile(v, `a${i}`)} />{/each}
        </Row>
      {/each}
    {/if}
    {#if subsRow.length}
      <Row title="Nuovi dalle iscrizioni">
        {#each subsRow as v (v.id)}<Card wide size={22} tile={videoTile(v, 's')} />{/each}
      </Row>
    {/if}
    {#if linked}
      {#each account.home.slice(1) as s, i (`a${i + 1}`)}
        <Row title={s.title || 'Altri per te'}>
          {#each s.videos as v (v.id)}<Card wide size={22} tile={videoTile(v, `a${i + 1}`)} />{/each}
        </Row>
      {/each}
      {#if feed.recommended.length}
        <Row title="Scelti da AzotoTV">
          {#each feed.recommended as v (v.id)}<Card wide size={22} tile={recTile(v, 'r')} />{/each}
        </Row>
      {/if}
    {:else if feed.recommended.length}
      <Row title={firstTime ? 'Da scoprire questa settimana' : 'Consigliati per te'}>
        {#each feed.recommended as v (v.id)}<Card wide size={22} tile={recTile(v, 'r')} />{/each}
      </Row>
      {#each feed.more as m, i (`${i}:${m.title}`)}
        <Row title={m.title}>
          {#each m.videos as v (v.id)}<Card wide size={22} tile={recTile(v, `m${i}`)} />{/each}
        </Row>
      {/each}
    {:else if feed.loading}
      <p class="msg">Carico i video…</p>
    {:else if feed.user === auth.id && !subsRow.length}
      <p class="msg">YouTube non risponde. Riprova tra poco.</p>
    {/if}
    {#if me.later.length}
      <Row title="Guarda più tardi">
        {#each me.later as v (v.id)}<Card wide size={22} tile={videoTile(v, 'l')} />{/each}
      </Row>
    {/if}
    {#if me.subs.length}
      <Row title="Iscrizioni">
        {#each me.subs as c (c.id)}<Card size={11} tile={channelTile(c, 'ch')} />{/each}
      </Row>
    {/if}
    {#if shortsLimit.n > 0}
      {#if shorts.length}
        <Row title={left === 1 ? 'Shorts · ne resta 1 per oggi' : `Shorts · ne restano ${left} per oggi`}>
          {#each shorts as v (v.id)}<Card size={12} tile={shortTile(v, 'sh')} />{/each}
        </Row>
      {:else if !left}
        <p class="msg small">Shorts finiti per oggi: tornano domani.</p>
      {/if}
    {/if}
    <div class="end"></div>
  </div>
</div>

{#if sheet}
  <Picker
    title="Shorts al giorno per profilo"
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
