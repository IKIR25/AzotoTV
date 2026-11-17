<script lang="ts">
  import { t } from '$lib/i18n';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { checkPin, enterProfile, profiles, userImage, type Profile } from '$lib/session.svelte';
  import PinPad from '$lib/PinPad.svelte';

  // The picker's PIN for a profile that has one (profile.rs). A forgotten PIN:
  // the Jellyfin password opens it too, when it has one.
  let p = $state<Profile | null>(null);
  let pin = $state('');
  let error = $state('');
  let busy = false;

  onMount(async () => {
    p = (await profiles().catch(() => ({ profiles: [] }))).profiles.find((x) => x.id === page.params.id) ?? null;
    if (!p) return goto('/profiles', { replaceState: true });
  });

  async function full(typed: string) {
    if (!p || busy) return;
    busy = true;
    try {
      if (await checkPin(p.id, typed)) {
        if (!(await enterProfile(p, true))) busy = false;
        return;
      }
      error = t('Wrong PIN');
    } catch (e) {
      error = String(e);
    }
    pin = '';
    busy = false;
  }
</script>

{#if p}
  {@const src = userImage(p.id, p.image_tag)}
  <div class="wrap">
    <section class="glass">
      <span class="pic">{#if src}<img {src} alt="" />{:else}<em>{p.name[0]?.toUpperCase()}</em>{/if}</span>
      <h1>{p.name}</h1>
      <p class:err={!!error}>{error || t('Enter the PIN')}</p>
      {#key p.id}<PinPad bind:value={pin} onfull={full} />{/key}
      {#if p.has_password}
        <button class="link" data-focusable onclick={() => goto(`/profiles/${p!.id}`, { replaceState: true })}>
          {t('Forgot the PIN? Use the password')}
        </button>
      {/if}
    </section>
  </div>
{/if}

<style>
  .wrap { position: absolute; inset: 0; display: grid; place-items: center; }
  section { width: 46rem; border-radius: 2.4rem; padding: 3rem 4rem; display: grid; gap: 1.2rem; justify-items: center; }
  .pic {
    position: relative; width: 9rem; height: 9rem; border-radius: 50%; overflow: hidden;
    display: grid; place-items: center; background: linear-gradient(135deg, #ff4fa3, #7c8cff);
  }
  .pic img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .pic em { font-style: normal; font-size: 4rem; font-weight: 700; }
  h1 { font-size: 2.6rem; font-weight: 600; }
  p { font-size: 1.5rem; color: var(--muted); min-height: 2rem; }
  p.err { color: #ff8a8a; }
  .link { font-size: 1.3rem; color: var(--muted); padding: 0.6rem 1.2rem; border-radius: 1rem; }
  .link:focus { background: #fff; color: #10121c; }
</style>
