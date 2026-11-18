<script lang="ts">
  import { t } from '$lib/i18n';
  import { key } from './input.svelte';
  import { onMount } from 'svelte';
  import { modal } from './stage.svelte';
  import { auth } from './session.svelte';
  import { answer, ask, names } from './phone.svelte';

  // A phone asked for something while the TV is on a profile it isn't paired
  // with (phone.svelte.ts): switch to one of its profiles, or stay. The phone's
  // own arrows and OK answer it, as do the controller and the TV remote.
  const who = ask.who!;
  const close = () => void answer(null);
  let panel: HTMLElement;

  onMount(() => {
    const before = document.activeElement;
    modal.close = close;
    panel.querySelector<HTMLElement>('[data-focusable]')?.focus({ preventScroll: true });
    return () => {
      if (modal.close === close) modal.close = null;
      if (before instanceof HTMLElement && before.isConnected) before.focus({ preventScroll: true });
    };
  });

  const onProfile = auth.state === 'in' && !location.pathname.startsWith('/profiles') && location.pathname !== '/login';
</script>

<div class="dim">
  <section class="panel glass" data-nav-trap bind:this={panel}>
    <span class="ico" aria-hidden="true">
      <svg viewBox="0 0 24 24"><rect x="7" y="2.5" width="10" height="19" rx="2.5" /><path d="M11 18.5h2" /></svg>
    </span>
    <h1>{t('This phone is connected to {names}', { names: names(who.users) })}</h1>
    <p>
      {#if onProfile}{t('{name}\'s profile is on the TV.', { name: auth.name })}{/if}
      {who.users.length === 1 ? t('Switch to {name}?', { name: who.users[0].name }) : t('Switch to one of these profiles?')}
    </p>
    <div class="row">
      {#each who.users as u (u.id)}
        <button class="btn primary" data-focusable onclick={() => answer(u)}>{t('Switch to {name}', { name: u.name })}</button>
      {/each}
      <button class="btn" data-focusable onclick={close}>No</button>
    </div>
    <div class="hint"><kbd data-btn="A">{key('A')}</kbd> {t('select ·')} <kbd data-btn="B">{key('B')}</kbd> {t('close')}</div>
  </section>
</div>

<style>
  .dim { position: absolute; z-index: 40; inset: 0; display: grid; place-items: center; background: rgb(var(--bg-rgb) / 0.6); }
  .panel {
    display: grid; justify-items: center; gap: 1.6rem; width: 72rem; max-width: calc(100% - 8rem);
    padding: 4rem; border-radius: 2.8rem; background: rgb(var(--panel-rgb) / 0.95); text-align: center;
  }
  .ico { width: 5rem; height: 5rem; color: var(--accent); }
  svg { width: 100%; height: 100%; fill: none; stroke: currentColor; stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; }
  h1 { font-size: 2.8rem; margin: 0; }
  p { font-size: 1.7rem; color: var(--muted); margin: 0; }
  .row { display: flex; flex-wrap: wrap; justify-content: center; gap: 1.2rem; margin-top: 1rem; }
  .hint { font-size: 1.15rem; color: var(--muted); }
  kbd {
    font: inherit; font-size: 0.95rem; font-weight: 700; padding: 0.05rem 0.45rem;
    border: 1px solid var(--glass-edge); border-radius: 0.4rem; color: var(--text);
  }
</style>
