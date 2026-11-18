<script lang="ts">
  import { locale, t } from '$lib/i18n';
  import { key } from './input.svelte';
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { control, modal, showToast } from './stage.svelte';
  import { auth } from './session.svelte';

  // "Collega un telefono" (START menu): a QR code and a 4-digit code that pair
  // a phone with this profile (src-tauri/src/phone.rs; the same phone can be
  // paired with more profiles), and the phones paired with it (A on one,
  // twice: unpaired from this profile). A new code shows after each pairing.
  const close = () => (control.phones = false);

  interface Pairing {
    code: string;
    url: string;
    address: string;
    qr: string;
  }
  interface Paired {
    id: string;
    /** what the phone is ("iPhone", "Galaxy S23 di Luca") */
    label: string;
    /** its other profiles */
    others: string[];
    paired: number;
  }

  let pairing = $state<Pairing | null>(null);
  let phones = $state<Paired[]>([]);
  let armed = $state('');
  let armTimer: ReturnType<typeof setTimeout>;
  let done: HTMLElement;

  const user = auth.id;
  const refresh = async () => (phones = await invoke<Paired[]>('phone_list', { user }).catch(() => []));
  const newCode = async () => (pairing = await invoke<Pairing>('phone_pairing', { user, name: auth.name }).catch(() => null));

  function forget(p: Paired) {
    if (armed !== p.id) {
      armed = p.id;
      clearTimeout(armTimer);
      armTimer = setTimeout(() => (armed = ''), 3500);
      return;
    }
    armed = '';
    void invoke('phone_forget', { id: p.id, user }).then(refresh);
    showToast(t('{label} disconnected from {name}', { label: p.label, name: auth.name }));
    done.focus({ preventScroll: true });
  }

  const when = (s: number) => new Date(s * 1000).toLocaleDateString(locale, { day: 'numeric', month: 'long' });

  onMount(() => {
    const before = document.activeElement;
    modal.close = close;
    void refresh();
    void newCode();
    done.focus({ preventScroll: true });
    // a phone typed the code: the list updates, and a new code for the next one
    const un = listen('phone-paired', () => {
      void refresh();
      showToast(t('Phone connected ✓'));
      void newCode();
    });
    return () => {
      clearTimeout(armTimer);
      void un.then((f) => f());
      void invoke('phone_pairing_stop').catch(() => {});
      if (modal.close === close) modal.close = null;
      if (before instanceof HTMLElement && before.isConnected) before.focus({ preventScroll: true });
    };
  });
</script>

<div class="dim">
  <section class="panel glass" data-nav-trap>
    <div class="qr" aria-label={t('QR code')}>
      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
      {#if pairing}{@html pairing.qr}{/if}
    </div>
    <div class="side">
      <h1>{t('Connect a phone to {name}', { name: auth.name })}</h1>
      <p class="lead">{t('The phone will control the TV while {name}\'s profile is on it.', { name: auth.name })}</p>
      <ol>
        <li>{t('Point the phone\'s camera at the QR code, on the home Wi-Fi.')}</li>
        <li>{t('Or open')} <b>{pairing?.address ?? '…'}</b> {t('and type the code:')}</li>
      </ol>
      <div class="code">
        {#each (pairing?.code ?? '····').split('') as d, i (i)}<span>{d}</span>{/each}
      </div>
      <p class="small">{t('Phone already connected to another profile? In the app, tap your name › Add a profile.')}</p>
      <p class="small">{t('On iPhone: Share › Add to Home Screen, and it becomes an app.')}</p>

      {#if phones.length}
        <h2>{t('Phones connected to {name}', { name: auth.name })}</h2>
        <div class="list">
          {#each phones as p (p.id)}
            <button class="phone" class:armed={armed === p.id} data-focusable onblur={() => armed === p.id && (armed = '')} onclick={() => forget(p)}>
              <span>{armed === p.id ? t('Press again to disconnect it from {name}', { name: auth.name }) : p.label}</span>
              <small>{armed === p.id ? '' : p.others.length ? `anche ${p.others.join(', ')}` : `dal ${when(p.paired)}`}</small>
              {#if armed !== p.id}<em class="x">{t('Unlink')}</em>{/if}
            </button>
          {/each}
        </div>
      {/if}
      <button class="btn primary" data-focusable bind:this={done} onclick={close}>{t('Done')}</button>
      <div class="hint">{#if phones.length}<kbd data-btn="A">{key('A')}</kbd> {t('on a phone: disconnects it from {name} ·', { name: auth.name })} {/if}<kbd data-btn="B">{key('B')}</kbd> {t('close')}</div>
    </div>
  </section>
</div>

<style>
  .dim { position: absolute; z-index: 22; inset: 0; display: grid; place-items: center; background: rgb(var(--bg-rgb) / 0.6); }
  .panel {
    display: flex; gap: 4rem; width: 110rem; max-width: calc(100% - 8rem); padding: 4rem;
    border-radius: 2.8rem; background: rgb(var(--panel-rgb) / 0.95);
  }
  .qr { flex: none; width: 36rem; height: 36rem; border-radius: 2rem; background: #fff; padding: 1.2rem; }
  .qr :global(svg) { width: 100%; height: 100%; display: block; }
  .side { flex: 1; display: flex; flex-direction: column; align-items: flex-start; min-width: 0; }
  h1 { font-size: 3rem; margin: 0 0 0.6rem; }
  .lead { font-size: 1.5rem; color: var(--muted); margin: 0 0 1.4rem; }
  h2 { font-size: 1.6rem; margin: 2.4rem 0 0.8rem; color: var(--muted); font-weight: 600; }
  ol { margin: 0; padding-left: 2.4rem; font-size: 1.6rem; line-height: 1.5; color: var(--muted); }
  ol b { color: var(--text); font-weight: 700; }
  .code { display: flex; gap: 1.2rem; margin: 1.8rem 0 1.4rem; }
  .code span {
    width: 6.4rem; height: 8rem; display: grid; place-items: center; border-radius: 1.4rem;
    background: rgb(var(--fg-rgb) / 0.08); font-size: 4.4rem; font-weight: 700; font-variant-numeric: tabular-nums;
  }
  .small { font-size: 1.3rem; color: var(--muted); margin: 0; }
  .list { display: flex; flex-direction: column; gap: 0.4rem; width: 100%; max-height: 16rem; overflow: hidden; }
  .phone {
    display: flex; align-items: baseline; justify-content: space-between; gap: 1rem; width: 100%;
    padding: 0.9rem 1.4rem; border-radius: 1.2rem; text-align: left; font-size: 1.5rem; font-weight: 600;
  }
  .phone small { font-size: 1.2rem; color: var(--muted); font-weight: 400; margin-left: auto; }
  .phone .x { display: none; font-style: normal; font-size: 1.2rem; font-weight: 700; color: var(--danger); }
  .phone:focus .x { display: inline; }
  /* plain state change, no transition (see tween.ts) */
  .phone:focus { background: var(--focus-bg); color: var(--focus-text); }
  .phone:focus small { color: rgb(var(--focus-text-rgb) / 0.65); }
  .phone.armed:focus { background: var(--danger); color: var(--on-accent); }
  .btn { margin-top: auto; }
  .side > .btn { margin-top: 2.4rem; }
  .hint { margin-top: 1.4rem; font-size: 1.15rem; color: var(--muted); }
  kbd {
    font: inherit; font-size: 0.95rem; font-weight: 700; padding: 0.05rem 0.45rem;
    border: 1px solid var(--glass-edge); border-radius: 0.4rem; color: var(--text);
  }
</style>
