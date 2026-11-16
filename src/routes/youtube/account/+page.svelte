<script lang="ts">
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
    showToast('Account Google scollegato: restano i consigli di AzotoTV', 4000);
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
      showToast(`Account collegato: ${e.payload.text}`, 4000);
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
      <div class="qr" aria-label="Codice QR">
        <!-- eslint-disable-next-line svelte/no-at-html-tags -->
        {#if code}{@html code.qr}{/if}
      </div>
      <div class="side">
        <h1>Accedi con Google</h1>
        <p class="lead">Così su {auth.name} vedi la home e le iscrizioni del tuo account YouTube.</p>
        <ol>
          <li>Sul telefono inquadra il codice QR, oppure apri <b>{(code?.url ?? 'google.com/device').replace(/^https:\/\/(www\.)?/, '')}</b></li>
          <li>Scrivi questo codice:</li>
        </ol>
        <div class="code">{code?.code ?? '···-···-···'}</div>
        <ol start="3"><li>Scegli il tuo account e tocca <b>Consenti</b>.</li></ol>
        {#if step === 'error'}
          <p class="err">{error}</p>
          <div class="row">
            <button class="btn primary" data-focusable onclick={start}>Nuovo codice</button>
            <button class="btn" data-focusable onclick={() => history.back()}>Annulla</button>
          </div>
        {:else}
          <p class="small">{code ? 'In attesa del telefono…' : 'Chiedo un codice a Google…'}</p>
          <p class="small">Nessuna password sulla TV. Mi piace, iscrizioni e cronologia di AzotoTV restano qui: non vanno su YouTube.</p>
          <button class="btn" data-focusable onclick={() => history.back()}>Annulla</button>
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
        <h1>{info.name || 'Account Google'}</h1>
        {#if info.handle}<p class="handle">{info.handle}</p>{/if}
        <p>Collegato a {auth.name}: su YouTube vedi la home e le iscrizioni di questo account. Mi piace, iscrizioni e cronologia di AzotoTV restano qui.</p>
        <div class="row">
          <button class="btn" class:warn={armed} data-focusable onblur={() => (armed = false)} onclick={unlink}>
            {armed ? 'Premi di nuovo per scollegare' : 'Scollega account'}
          </button>
          <button class="btn primary" data-focusable onclick={() => history.back()}>Fatto</button>
        </div>
      {:else if step === 'nopin'}
        <span class="pic lock" aria-hidden="true">
          <svg viewBox="0 0 24 24"><rect x="5" y="10.5" width="14" height="10" rx="2" /><path d="M8.5 10.5V7.5a3.5 3.5 0 0 1 7 0v3" /></svg>
        </span>
        <h1>Prima un PIN</h1>
        <p>Con un account Google collegato, {auth.name} deve avere un PIN: così nessun altro entra nel tuo account dalla TV.</p>
        <div class="row">
          <button class="btn primary" data-focusable onclick={() => goto('/pin')}>Imposta il PIN</button>
          <button class="btn" data-focusable onclick={() => history.back()}>Annulla</button>
        </div>
      {:else}
        <h1>Accedi con Google</h1>
        <p class="err">{error}</p>
        <div class="row">
          <button class="btn primary" data-focusable onclick={start}>Riprova</button>
          <button class="btn" data-focusable onclick={() => history.back()}>Annulla</button>
        </div>
      {/if}
    </section>
  {/if}
</div>

<style>
  .wrap { position: absolute; inset: 0; display: grid; place-items: center; }
  .panel {
    display: flex; gap: 4rem; width: 110rem; max-width: calc(100% - 8rem); padding: 4rem;
    border-radius: 2.8rem; background: rgba(16, 18, 32, 0.95);
  }
  .qr { flex: none; width: 36rem; height: 36rem; border-radius: 2rem; background: #fff; padding: 1.2rem; }
  .qr :global(svg) { width: 100%; height: 100%; display: block; }
  .side { flex: 1; display: flex; flex-direction: column; align-items: flex-start; gap: 0.8rem; min-width: 0; }
  .lead { font-size: 1.5rem; color: var(--muted); margin: 0 0 0.8rem; }
  ol { margin: 0; padding-left: 2.4rem; font-size: 1.6rem; line-height: 1.5; color: var(--muted); }
  ol b { color: var(--text); font-weight: 700; }
  .code {
    margin: 0.6rem 0 0.8rem 2.4rem; padding: 0.6rem 2rem; border-radius: 1.4rem; background: rgba(255, 255, 255, 0.1);
    font-size: 4rem; font-weight: 700; letter-spacing: 0.12em; font-variant-numeric: tabular-nums;
  }
  .small { font-size: 1.3rem; color: var(--muted); max-width: 60rem; }
  .card { width: 56rem; border-radius: 2.4rem; padding: 3rem 4rem; display: grid; gap: 1.2rem; justify-items: center; text-align: center; }
  .pic {
    position: relative; width: 9rem; height: 9rem; border-radius: 50%; overflow: hidden;
    display: grid; place-items: center; background: linear-gradient(135deg, #ff4e45, #a3001b);
    font-size: 4rem; font-weight: 700;
  }
  .pic img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .pic svg { width: 4.6rem; height: 4.6rem; fill: none; stroke: #fff; stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; }
  h1 { font-size: 2.6rem; font-weight: 600; }
  .handle { margin-top: -0.8rem; }
  p { font-size: 1.5rem; color: var(--muted); }
  p.err { color: #ff8a8a; }
  .row { display: flex; gap: 1.2rem; margin-top: 0.6rem; }
  .btn.warn { background: #e0162b; color: #fff; }
  .btn.warn:focus { background: #ff4e45; color: #fff; }
</style>
