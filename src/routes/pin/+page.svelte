<script lang="ts">
  import { t } from '$lib/i18n';
  import { onMount, tick } from 'svelte';
  import { auth, loadPrefs, prefs, setPin, userImage } from '$lib/session.svelte';
  import { showToast } from '$lib/stage.svelte';
  import PinPad from '$lib/PinPad.svelte';

  // START › PIN del profilo: choose one (twice), change it or take it away.
  // No need for the old one: whoever is here is already in the profile.
  let step = $state<'ready' | 'new' | 'repeat'>('new');
  let first = '';
  let pin = $state('');
  let error = $state('');
  let avatarImg = $state(true);

  onMount(async () => {
    await loadPrefs();
    if (prefs.pin) step = 'ready';
    await tick();
    document.querySelector<HTMLElement>('.wrap [data-focusable]')?.focus({ preventScroll: true });
  });

  async function full(typed: string) {
    if (step === 'new') {
      first = typed;
      error = '';
      step = 'repeat';
    } else if (typed !== first) {
      error = t('The two PINs don\'t match: pick one again');
      step = 'new';
    } else {
      try {
        await setPin(typed);
        showToast(t('PIN set: it will be needed to enter {name}', { name: auth.name }), 3500);
        return history.back();
      } catch (e) {
        error = String(e);
        step = 'new';
      }
    }
    pin = '';
  }

  async function remove() {
    try {
      await setPin(null);
      showToast('PIN tolto: si entra senza', 3000);
      history.back();
    } catch (e) {
      showToast(t('Couldn\'t: {e}', { e }), 4000);
    }
  }
</script>

<div class="wrap">
  <section class="glass">
    <span class="pic"
      >{auth.name[0]?.toUpperCase() ?? '?'}{#if avatarImg}<img
          src={userImage(auth.id, 'current', 200)}
          alt=""
          onerror={() => (avatarImg = false)}
        />{/if}</span
    >
    <h1>{t('{name}\'s PIN', { name: auth.name })}</h1>
    {#if step === 'ready'}
      <p>{t('The PIN is asked when you pick this profile in “Who is watching?”.')}</p>
      <div class="row">
        <button class="btn primary" data-focusable onclick={() => (step = 'new')}>{t('Change PIN')}</button>
        <button class="btn" data-focusable onclick={remove}>{t('Remove the PIN')}</button>
      </div>
    {:else}
      <p class:err={!!error}>{error || (step === 'new' ? t('Pick a 4-digit PIN') : t('Type it again'))}</p>
      {#key step}<PinPad bind:value={pin} onfull={full} />{/key}
    {/if}
  </section>
</div>

<style>
  .wrap { position: absolute; inset: 0; display: grid; place-items: center; }
  section { width: 52rem; border-radius: 2.4rem; padding: 3rem 4rem; display: grid; gap: 1.2rem; justify-items: center; text-align: center; }
  .pic {
    position: relative; width: 9rem; height: 9rem; border-radius: 50%; overflow: hidden;
    display: grid; place-items: center; background: var(--avatar-grad);
    font-size: 4rem; font-weight: 700;
  }
  .pic img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  h1 { font-size: 2.6rem; font-weight: 600; }
  p { font-size: 1.5rem; color: var(--muted); min-height: 2rem; }
  p.err { color: var(--danger-text); }
  .row { display: flex; gap: 1.2rem; margin-top: 0.6rem; }
</style>
