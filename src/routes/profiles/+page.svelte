<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { goto } from '$app/navigation';
  import { auth, enterProfile, profiles, signOut, userImage, type Profile } from '$lib/session.svelte';
  import { liftOnFocus } from '$lib/tween';
  import Logo from '$lib/Logo.svelte';
  import Splash from '$lib/Splash.svelte';

  // Right after the TV boots Jellyfin is usually still starting: show the
  // splash instead of the offline picker for a while, until it answers.
  const BOOT_MS = 90_000;
  let booting = $state(performance.now() < BOOT_MS);
  let slow = $state(false);

  let list = $state<Profile[]>([]);
  let manage = $state(false);
  let failed = $state('');
  let offline = $state(false);
  // pictures that didn't load (server down and not cached): show the initial
  let noPic = $state<Record<string, boolean>>({});
  let busy = $state('');

  // one row while it fits, pictures shrinking from 15rem down to 10rem; more
  // profiles than that wrap into a second row of 10rem pictures
  const count = $derived(list.length + (manage ? 0 : 1));
  const pic = $derived(Math.max(10, Math.min(15, (112 - (count - 1) * 3) / count - 2)));

  async function load() {
    clearTimeout(retry);
    failed = '';
    try {
      ({ profiles: list, offline } = await profiles());
    } catch (e) {
      failed = String(e);
    }
    // at boot Jellyfin is often still starting: keep asking until it answers
    if (failed || offline) retry = setTimeout(load, 3000);
    else noPic = {};
    booting = (!!failed || offline) && performance.now() < BOOT_MS;
    slow = performance.now() > 8000;
    await tick();
    // land on the active / last used profile, unless a refresh kept the focus
    if (document.activeElement?.closest('.wrap')) return;
    const el =
      document.querySelector<HTMLElement>(`.prof[data-key="${auth.id}"]`) ??
      document.querySelector<HTMLElement>('.prof, .tools [data-focusable]');
    el?.focus({ preventScroll: true });
  }
  let retry: ReturnType<typeof setTimeout> | undefined;
  onMount(() => {
    load();
    return () => clearTimeout(retry);
  });

  async function pick(p: Profile) {
    if (busy) return;
    if (manage) {
      if (p.signed_in) {
        await signOut(p.id);
        await load();
      }
      return;
    }
    busy = p.id;
    if (!(await enterProfile(p))) busy = '';
  }
</script>

{#if booting}
  <Splash note={slow ? 'In attesa del server…' : ''} />
{:else}
<div class="wrap">
  <Logo size={2} />
  <h1>{manage ? 'Gestisci profili' : 'Chi sta guardando?'}</h1>

  {#if failed}
    <p class="note">Impossibile contattare il server: {failed}</p>
  {:else if offline}
    <p class="note">Jellyfin non risponde: profili salvati su questo dispositivo</p>
  {/if}

  <div class="row" style="--pic: {pic}rem">
    {#each list as p (p.id)}
      {@const src = userImage(p.id, p.image_tag)}
      <button class="prof" class:manage data-focusable data-key={p.id} use:liftOnFocus={1.1} onclick={() => pick(p)}>
        <span class="pic">
          {#if src && !noPic[p.id]}<img {src} alt="" onerror={() => (noPic[p.id] = true)} />{:else}<em
              >{p.name[0]?.toUpperCase()}</em
            >{/if}
          {#if manage && p.signed_in}<i class="x">✕</i>{/if}
        </span>
        <span class="name">{p.name}</span>
        <small>
          {#if busy === p.id}Accesso…
          {:else if manage}{p.signed_in ? 'Esci da questo dispositivo' : 'Non connesso'}
          {:else if !p.signed_in && p.has_password}Password richiesta
          {:else if p.pin}PIN richiesto{/if}
        </small>
      </button>
    {/each}
    {#if !manage}
      <button class="prof add" data-focusable data-key="other" use:liftOnFocus={1.1} onclick={() => goto('/login')}>
        <span class="pic"><em>＋</em></span>
        <span class="name">Altro utente</span>
        <small></small>
      </button>
    {/if}
  </div>

  <div class="tools">
    {#if failed || offline}<button class="btn" data-focusable onclick={load}>Riprova</button>{/if}
    <button class="btn" data-focusable data-key="manage" onclick={() => (manage = !manage)}>
      {manage ? 'Fatto' : 'Gestisci profili'}
    </button>
  </div>
</div>
{/if}

<style>
  .wrap { position: absolute; inset: 0; display: grid; place-content: center; justify-items: center; gap: 3rem; }
  h1 { font-size: 3.6rem; font-weight: 600; letter-spacing: -0.01em; }
  .row { display: flex; gap: 3rem; justify-content: center; flex-wrap: wrap; max-width: 112rem; margin-top: 1rem; }
  .prof { display: grid; justify-items: center; gap: 1rem; width: calc(var(--pic) + 2rem); will-change: transform; }
  .pic {
    position: relative; width: var(--pic); height: var(--pic); border-radius: 50%; overflow: hidden;
    display: grid; place-items: center; background: linear-gradient(135deg, #ff4fa3, #7c8cff);
    box-shadow: 0 1rem 2.4rem rgba(0, 0, 0, 0.45);
  }
  .add .pic { background: rgba(255, 255, 255, 0.1); }
  .pic img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .pic em { font-style: normal; font-size: calc(var(--pic) * 0.4); font-weight: 700; color: rgba(255, 255, 255, 0.9); }
  .manage .pic img { opacity: 0.45; }
  .x {
    position: absolute; inset: 0; display: grid; place-items: center; font-style: normal;
    font-size: calc(var(--pic) * 0.33); color: #fff;
  }
  /* plain state change, no transition (see tween.ts) */
  .prof:focus .pic { box-shadow: 0 0 0 0.4rem #fff, 0 1.4rem 3rem rgba(0, 0, 0, 0.55); }
  .name { font-size: 1.9rem; font-weight: 600; color: var(--muted); }
  .prof:focus .name { color: var(--text); }
  small { font-size: 1.2rem; color: var(--muted); min-height: 1.6rem; }
  .tools { display: flex; gap: 1rem; margin-top: 1rem; }
  .note { color: #ff8a8a; font-size: 1.5rem; }
</style>
