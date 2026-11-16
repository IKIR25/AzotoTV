<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { showToast } from '$lib/stage.svelte';
  import PinPad from '$lib/PinPad.svelte';

  // START › Collega la TV (or A on the "La TV non prende più…" notification):
  // the TV shows a 4-digit code over its picture, typed here. A wrong code
  // gets a new one on the TV. Leaving the page takes the code off the TV.
  let step = $state<'wait' | 'pin' | 'off'>('wait');
  let pin = $state('');
  let error = $state('');
  let done = false;

  async function start() {
    step = 'wait';
    try {
      await invoke('tv_pair_start');
      step = 'pin';
    } catch (e) {
      error = String(e);
      step = 'off';
      await tick();
      document.querySelector<HTMLElement>('.wrap [data-focusable]')?.focus({ preventScroll: true });
    }
  }

  async function full(typed: string) {
    step = 'wait';
    try {
      await invoke('tv_pair_pin', { pin: typed });
      done = true;
      showToast("TV collegata: Spegni e l'accensione automatica vanno di nuovo", 4000);
      history.back();
    } catch (e) {
      error = `${e}: ne compare uno nuovo sulla TV`;
      pin = '';
      void start();
    }
  }

  onMount(() => {
    void start();
    return () => {
      if (!done) invoke('tv_pair_cancel').catch(() => {});
    };
  });
</script>

<div class="wrap">
  <section class="glass">
    <span class="pic" aria-hidden="true">
      <svg viewBox="0 0 24 24"><rect x="3" y="4.5" width="18" height="12.5" rx="2" /><path d="M8 20.5h8" /></svg>
    </span>
    <h1>Collega la TV</h1>
    {#if step === 'off'}
      <p class="err">{error}</p>
      <div class="row">
        <button class="btn primary" data-focusable onclick={() => ((error = ''), start())}>Riprova</button>
        <button class="btn" data-focusable onclick={() => history.back()}>Annulla</button>
      </div>
    {:else}
      <p class:err={!!error}>{error || (step === 'wait' ? 'Un attimo…' : 'Scrivi il codice di 4 cifre che compare sulla TV')}</p>
      {#if step === 'pin'}<PinPad bind:value={pin} onfull={full} />{/if}
    {/if}
  </section>
</div>

<style>
  .wrap { position: absolute; inset: 0; display: grid; place-items: center; }
  section { width: 52rem; border-radius: 2.4rem; padding: 3rem 4rem; display: grid; gap: 1.2rem; justify-items: center; text-align: center; }
  .pic {
    width: 9rem; height: 9rem; border-radius: 50%; display: grid; place-items: center;
    background: linear-gradient(135deg, #3fb6ff, #7c8cff);
  }
  .pic svg { width: 4.6rem; height: 4.6rem; fill: none; stroke: #fff; stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; }
  h1 { font-size: 2.6rem; font-weight: 600; }
  p { font-size: 1.5rem; color: var(--muted); min-height: 2rem; }
  p.err { color: #ff8a8a; }
  .row { display: flex; gap: 1.2rem; margin-top: 0.6rem; }
</style>
