<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { auth, loadPrefs, login, prefs, server } from '$lib/session.svelte';
  import { focusFirst } from '$lib/nav';
  import Logo from '$lib/Logo.svelte';

  // "Altro utente" / first start: any user, any server.
  let base = $state('http://localhost:8096');
  let user = $state('');
  let pass = $state('');
  let busy = $state(false);

  onMount(async () => {
    auth.error = '';
    base = (await server()) ?? base;
    focusFirst();
  });

  async function submit(e: Event) {
    e.preventDefault();
    busy = true;
    if (await login(base, user, pass)) {
      // no password typed: a AzotoTV PIN still has to be
      await loadPrefs();
      if (!pass && prefs.pin) {
        auth.locked = true;
        return goto(`/profiles/${auth.id}/pin`, { replaceState: true });
      }
      goto('/', { replaceState: true });
    }
    busy = false;
  }
</script>

<div class="wrap">
  <form class="glass" onsubmit={submit}>
    <div class="logo"><Logo size={2.4} /></div>
    <label>Server<input data-focusable bind:value={base} autocomplete="off" /></label>
    <label>Utente<input data-focusable bind:value={user} autocomplete="username" /></label>
    <label>Password<input data-focusable data-osk-submit type="password" bind:value={pass} autocomplete="current-password" /></label>
    {#if auth.error}<p class="err">{auth.error}</p>{/if}
    <button class="btn primary" data-focusable disabled={busy}>{busy ? 'Accesso…' : 'Accedi'}</button>
  </form>
</div>

<style>
  .wrap { position: absolute; inset: 0; display: grid; place-items: center; }
  /* make room for the on-screen keyboard at the bottom */
  :global(.osk-open) .wrap { place-items: start center; padding-top: 3rem; }
  form { width: 52rem; border-radius: 2.4rem; padding: 3.4rem 4rem; display: grid; gap: 1.6rem; }
  .logo { text-align: center; margin-bottom: 0.6rem; }
  label { display: grid; gap: 0.5rem; font-size: 1.3rem; color: var(--muted); }
  input {
    font: inherit; font-size: 1.7rem; color: var(--text); padding: 1.1rem 1.4rem; border-radius: 1.2rem;
    border: 0.2rem solid transparent; background: rgba(255, 255, 255, 0.1); outline: none;
    user-select: text; -webkit-user-select: text; cursor: text;
  }
  input:focus { border-color: #fff; background: rgba(255, 255, 255, 0.16); }
  .err { color: #ff8a8a; font-size: 1.3rem; }
  .btn { justify-content: center; margin-top: 0.6rem; }
</style>
