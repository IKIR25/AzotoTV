<script lang="ts">
  import { t } from '$lib/i18n';
  import { onMount, tick } from 'svelte';
  import { goto } from '$app/navigation';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { auth, loadPrefs, prefs } from '$lib/session.svelte';
  import { showToast } from '$lib/stage.svelte';
  import { accountLinked, unlinkAccount, ytImage, type AccountInfo } from '$lib/youtube.svelte';

  // YouTube › Accedi con Google (or the account's name): link this profile's
  // own Google account the way the TV's YouTube app does. The TV shows a code
  // (and a QR code of the page), the phone approves it at google.com/device,
  // and the event `yt-account` says how it went (ytaccount.rs). Only a profile
  // with a PIN may link one: else anyone picking the profile is in the account.
  // Linked: who it is, and Scollega (A twice).
  interface Code {
    code: string;
    url: string;
    qr: string;
    expires_in: number;
  }

  let step = $state<'load' | 'nopin' | 'code' | 'error' | 'linked'>('load');
  let code = $state<Code | null>(null);
  let error = $state('');
  let armed = $state(false);
  let armTimer: ReturnType<typeof setTimeout>;
  let photoOk = $state(true);
  let info = $state<AccountInfo | null>(null);
  const user = auth.id;

  async function focusFirst() {
    await tick();
    document.querySelector<HTMLElement>('.wrap [data-focusable]')?.focus({ preventScroll: true });
  }

  async function start() {
    error = '';
    code = null;
    step = 'code';
    try {
      code = await invoke<Code>('yt_link_start');
    } catch (e) {
      error = String(e);
      step = 'error';
    }
    await focusFirst();
  }

  async function unlink() {
    clearTimeout(armTimer);
    if (!armed) {
      armed = true;
      armTimer = setTimeout(() => (armed = false), 4000);
      return;
    }
    armed = false;
    await unlinkAccount().catch(() => {});
    showToast(t('Google account unlinked: AzotoTV\'s recommendations remain'), 4000);
    goto('/youtube', { replaceState: true });
  }

  onMount(() => {
    const un = listen<{ user: string; ok: boolean; text: string }>('yt-account', async (e) => {
      if (e.payload.user !== user) return;
      if (!e.payload.ok) {
        error = e.payload.text;
        step = 'error';
        return focusFirst();
      }
      showToast(t('Account linked: {text}', { text: e.payload.text }), 4000);
      await accountLinked(user);
      goto('/youtube', { replaceState: true });
    });
    (async () => {
      await loadPrefs();
      info = await invoke<AccountInfo | null>('yt_account').catch(() => null);
      if (info) step = 'linked';
      else if (!prefs.pin) step = 'nopin';
      else return start();
      await focusFirst();
    })();
    return () => {
      clearTimeout(armTimer);
      void un.then((f) => f());
      void invoke('yt_link_cancel').catch(() => {});
    };
  });
</script>

<div class="wrap">
  {#if step === 'code' || (step === 'error' && code)}
    <section class="panel glass">
      <div class="qr" aria-label={t('QR code')}>
        <!-- eslint-disable-next-line svelte/no-at-html-tags -->
        {#if code}{@html code.qr}{/if}
      </div>
      <div class="side">
        <h1>{t('Sign in with Google')}</h1>
        <p class="lead">{t('So that on {name} you see your YouTube account\'s home and subscriptions.', { name: auth.name })}</p>
        <ol>
          <li>{t('On the phone, scan the QR code, or open')} <b>{(code?.url ?? 'google.com/device').replace(/^https:\/\/(www\.)?/, '')}</b></li>
          <li>{t('Type this code:')}</li>
        </ol>
        <div class="code">{code?.code ?? '···-···-···'}</div>
        <ol start="3"><li>{t('Pick your account and tap')} <b>{t('Allow')}</b>.</li></ol>
        {#if step === 'error'}
          <p class="err">{error}</p>
          <div class="row">
            <button class="btn primary" data-focusable onclick={start}>{t('New code')}</button>
            <button class="btn" data-focusable onclick={() => history.back()}>{t('Cancel')}</button>
          </div>
        {:else}
          <p class="small">{code ? t('Waiting for the phone…') : t('Asking Google for a code…')}</p>
          <p class="small">{t('No password on the TV. AzotoTV\'s likes, subscriptions and history stay here: they don\'t go to YouTube.')}</p>
          <button class="btn" data-focusable onclick={() => history.back()}>{t('Cancel')}</button>
        {/if}
      </div>
    </section>
  {:else if step !== 'load'}
    <section class="card glass">
      {#if step === 'linked' && info}
        <span class="pic"
          >{info.name[0]?.toUpperCase() ?? 'G'}{#if info.photo && photoOk}<img
              src={ytImage(info.photo)}
              alt=""
              onerror={() => (photoOk = false)}
            />{/if}</span
        >
        <h1>{info.name || t('Google account')}</h1>
        {#if info.handle}<p class="handle">{info.handle}</p>{/if}
        <p>{t('Linked to {name}: in YouTube you see this account\'s home and subscriptions. AzotoTV\'s likes, subscriptions and history stay here.', { name: auth.name })}</p>
        <div class="row">
          <button class="btn" class:warn={armed} data-focusable onblur={() => (armed = false)} onclick={unlink}>
            {armed ? t('Press again to unlink') : t('Unlink account')}
          </button>
          <button class="btn primary" data-focusable onclick={() => history.back()}>{t('Done')}</button>
        </div>
      {:else if step === 'nopin'}
        <span class="pic lock" aria-hidden="true">
          <svg viewBox="0 0 24 24"><rect x="5" y="10.5" width="14" height="10" rx="2" /><path d="M8.5 10.5V7.5a3.5 3.5 0 0 1 7 0v3" /></svg>
        </span>
        <h1>{t('First, a PIN')}</h1>
        <p>{t('With a Google account linked, {name} must have a PIN: that way nobody else gets into your account from the TV.', { name: auth.name })}</p>
        <div class="row">
          <button class="btn primary" data-focusable onclick={() => goto('/pin')}>{t('Set the PIN')}</button>
          <button class="btn" data-focusable onclick={() => history.back()}>{t('Cancel')}</button>
        </div>
      {:else}
        <h1>{t('Sign in with Google')}</h1>
        <p class="err">{error}</p>
        <div class="row">
          <button class="btn primary" data-focusable onclick={start}>{t('Try again')}</button>
          <button class="btn" data-focusable onclick={() => history.back()}>{t('Cancel')}</button>
        </div>
      {/if}
    </section>
  {/if}
</div>

<style>
  .wrap { position: absolute; inset: 0; display: grid; place-items: center; }
  .panel {
    display: flex; gap: 4rem; width: 110rem; max-width: calc(100% - 8rem); padding: 4rem;
    border-radius: 2.8rem; background: rgb(var(--panel-rgb) / 0.95);
  }
  .qr { flex: none; width: 36rem; height: 36rem; border-radius: 2rem; background: #fff; padding: 1.2rem; }
  .qr :global(svg) { width: 100%; height: 100%; display: block; }
  .side { flex: 1; display: flex; flex-direction: column; align-items: flex-start; gap: 0.8rem; min-width: 0; }
  .lead { font-size: 1.5rem; color: var(--muted); margin: 0 0 0.8rem; }
  ol { margin: 0; padding-left: 2.4rem; font-size: 1.6rem; line-height: 1.5; color: var(--muted); }
  ol b { color: var(--text); font-weight: 700; }
  .code {
    margin: 0.6rem 0 0.8rem 2.4rem; padding: 0.6rem 2rem; border-radius: 1.4rem; background: rgb(var(--fg-rgb) / 0.1);
    font-size: 4rem; font-weight: 700; letter-spacing: 0.12em; font-variant-numeric: tabular-nums;
  }
  .small { font-size: 1.3rem; color: var(--muted); max-width: 60rem; }
  .card { width: 56rem; border-radius: 2.4rem; padding: 3rem 4rem; display: grid; gap: 1.2rem; justify-items: center; text-align: center; }
  .pic {
    position: relative; width: 9rem; height: 9rem; border-radius: 50%; overflow: hidden;
    display: grid; place-items: center; background: linear-gradient(135deg, var(--destroy-hi), #a3001b);
    font-size: 4rem; font-weight: 700;
  }
  .pic img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .pic svg { width: 4.6rem; height: 4.6rem; fill: none; stroke: var(--on-accent); stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; }
  h1 { font-size: 2.6rem; font-weight: 600; }
  .handle { margin-top: -0.8rem; }
  p { font-size: 1.5rem; color: var(--muted); }
  p.err { color: var(--danger-text); }
  .row { display: flex; gap: 1.2rem; margin-top: 0.6rem; }
  .btn.warn { background: var(--destroy); color: var(--on-accent); }
  .btn.warn:focus { background: var(--destroy-hi); color: var(--on-accent); }
</style>
