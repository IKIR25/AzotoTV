<script lang="ts">
  import { t } from '$lib/i18n';
  import { onMount, tick } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { auth, login, profiles, server, userImage, type Profile } from '$lib/session.svelte';
  import { openKeyboard } from '$lib/osk.svelte';

  let p = $state<Profile | null>(null);
  let pass = $state('');
  let busy = $state(false);
  let input = $state<HTMLInputElement>();

  onMount(async () => {
    auth.error = '';
    p = (await profiles().catch(() => ({ profiles: [] }))).profiles.find((x) => x.id === page.params.id) ?? null;
    if (!p) return goto('/profiles', { replaceState: true });
    await tick();
    input?.focus({ preventScroll: true });
    if (input) openKeyboard(input);
  });

  async function submit(e: Event) {
    e.preventDefault();
    const base = await server();
    if (!p || !base || busy) return;
    busy = true;
    if (await login(base, p.name, pass)) return goto('/', { replaceState: true });
    busy = false;
    pass = '';
    await tick();
    if (input) openKeyboard(input); // wrong password: straight back to typing
  }
</script>

{#if p}
  {@const src = userImage(p.id, p.image_tag)}
  <div class="wrap">
    <form class="glass" onsubmit={submit}>
      <span class="pic">{#if src}<img {src} alt="" />{:else}<em>{p.name[0]?.toUpperCase()}</em>{/if}</span>
      <h1>{p.name}</h1>
      <label>Password<input bind:this={input} bind:value={pass} type="password" data-focusable data-osk-submit /></label>
      {#if auth.error}<p class="err">{auth.error}</p>{/if}
      <button class="btn primary" data-focusable>{busy ? t('Signing in…') : t('Sign in')}</button>
    </form>
  </div>
{/if}

<style>
  .wrap { position: absolute; inset: 0; display: grid; place-items: center; }
  /* make room for the on-screen keyboard at the bottom */
  :global(.osk-open) .wrap { place-items: start center; padding-top: 3rem; }
  form { width: 46rem; border-radius: 2.4rem; padding: 3rem 4rem; display: grid; gap: 1.4rem; justify-items: center; }
  .pic {
    position: relative; width: 9rem; height: 9rem; border-radius: 50%; overflow: hidden;
    display: grid; place-items: center; background: var(--avatar-grad);
  }
  .pic img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .pic em { font-style: normal; font-size: 4rem; font-weight: 700; }
  h1 { font-size: 2.6rem; font-weight: 600; }
  label { display: grid; gap: 0.5rem; font-size: 1.3rem; color: var(--muted); width: 100%; }
  input {
    font: inherit; font-size: 1.7rem; color: var(--text); padding: 1.1rem 1.4rem; border-radius: 1.2rem;
    border: 0.2rem solid transparent; background: rgb(var(--fg-rgb) / 0.1); outline: none;
  }
  input:focus { border-color: var(--ring); background: rgb(var(--fg-rgb) / 0.16); }
  .err { color: var(--danger-text); font-size: 1.3rem; }
  .btn { justify-content: center; width: 100%; }
</style>
