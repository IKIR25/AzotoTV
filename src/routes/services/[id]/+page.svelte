<script lang="ts">
  import { t } from '$lib/i18n';
  import { onMount, tick } from 'svelte';
  import { page } from '$app/state';
  import { showToast } from '$lib/stage.svelte';
  import { loadServices, removeService, restartApp, saveService, services, testService, type Service } from '$lib/services.svelte';

  // One service of START › Services: its fields, Try, Save (which tries
  // first, then AzotoTV starts again so the service reads its new settings),
  // Turn off. Long values (keys) are easiest typed on a connected phone.
  const id = page.params.id ?? '';
  let svc = $state<Service | null>(null);
  let values = $state<Record<string, string>>({});
  let busy = $state(false);
  let lines = $state<string[]>([]);
  let error = $state('');
  let armed = $state(false);
  let armTimer: ReturnType<typeof setTimeout>;

  onMount(() => {
    void (async () => {
      try {
        const list = services.list.length ? services.list : await loadServices();
        svc = list.find((s) => s.id === id) ?? null;
        if (!svc) return void (error = t('No such service'));
        values = Object.fromEntries(svc.fields.map((f) => [f.key, f.value]));
      } catch (e) {
        error = String(e);
      }
      await tick();
      document.querySelector<HTMLElement>('.wrap [data-focusable]')?.focus({ preventScroll: true });
    })();
    return () => clearTimeout(armTimer);
  });

  async function run(save: boolean) {
    // (not `disabled` while busy: a disabled button loses the focus)
    if (busy) return;
    busy = true;
    error = '';
    lines = [];
    try {
      lines = await (save ? saveService(id, values) : testService(id, values));
      if (save) {
        services.list = [];
        // Google sign-in and SteamGridDB read their file each time; the others only at start
        if (id === 'youtube' || id === 'steamgriddb') {
          showToast(t('Saved'), 2500);
          history.back();
        } else {
          showToast(t('Saved: AzotoTV starts again'), 3000);
          setTimeout(() => void restartApp(), 1500);
        }
      }
    } catch (e) {
      error = String(e);
    }
    busy = false;
  }

  async function turnOff() {
    if (!armed) {
      armed = true;
      clearTimeout(armTimer);
      armTimer = setTimeout(() => (armed = false), 3500);
      return;
    }
    armed = false;
    try {
      await removeService(id);
      services.list = [];
      showToast(t('Turned off: AzotoTV starts again'), 3000);
      setTimeout(() => void restartApp(), 1500);
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div class="wrap">
  <form class="glass" onsubmit={(e) => (e.preventDefault(), run(true))}>
    {#if svc}
      <h1>{svc.title}</h1>
      <p class="lead">{svc.what}</p>
      {#each svc.fields as f (f.key)}
        <label>
          <span>{f.label}{#if f.optional}<em>{t('optional')}</em>{/if}</span>
          <input
            data-focusable
            type={f.kind === 'secret' ? 'password' : 'text'}
            bind:value={values[f.key]}
            autocomplete="off"
            spellcheck="false"
          />
          {#if f.hint}<small>{f.hint}</small>{/if}
        </label>
      {/each}
      {#if lines.length}<ul class="ok">{#each lines as l (l)}<li>✓ {l}</li>{/each}</ul>{/if}
      {#if error}<p class="err">{error}</p>{/if}
      <div class="row">
        <button class="btn primary" data-focusable>{busy ? t('Trying…') : t('Save')}</button>
        <button type="button" class="btn" data-focusable onclick={() => run(false)}>{t('Try')}</button>
        {#if svc.configured}
          <button type="button" class="btn" class:armed data-focusable onblur={() => (armed = false)} onclick={turnOff}>
            {armed ? t('Press again to turn it off') : t('Turn off')}
          </button>
        {/if}
      </div>
      <p class="tip">{t('Tip: long keys are easier to paste from a connected phone')}</p>
    {:else if error}
      <p class="err">{error}</p>
    {/if}
  </form>
</div>

<style>
  .wrap { position: absolute; inset: 0; display: grid; place-items: center; overflow-y: auto; padding: 3rem 0; }
  :global(.osk-open) .wrap { place-items: start center; }
  form { width: 64rem; border-radius: 2.4rem; padding: 3rem 4rem; display: grid; gap: 1.3rem; }
  h1 { font-size: 2.6rem; font-weight: 600; }
  .lead { font-size: 1.4rem; color: var(--muted); margin-top: -0.6rem; }
  label { display: grid; gap: 0.4rem; font-size: 1.3rem; color: var(--muted); }
  label em { font-style: normal; margin-left: 0.8rem; opacity: 0.7; }
  label small { font-size: 1.1rem; opacity: 0.8; }
  input {
    font: inherit; font-size: 1.6rem; color: var(--text); padding: 1rem 1.3rem; border-radius: 1.2rem;
    border: 0.2rem solid transparent; background: rgb(var(--fg-rgb) / 0.1); outline: none;
    user-select: text; -webkit-user-select: text; cursor: text;
  }
  input:focus { border-color: var(--ring); background: rgb(var(--fg-rgb) / 0.16); }
  .ok { list-style: none; padding: 0; color: var(--ok); font-size: 1.35rem; display: grid; gap: 0.3rem; }
  .err { color: var(--danger-text); font-size: 1.35rem; }
  /* Save spans the form and Try reaches past its middle, so Down goes field -> Save -> Try */
  .row { display: grid; grid-template-columns: 1.2fr 1fr; gap: 1.2rem; margin-top: 0.4rem; }
  .row .btn { justify-content: center; }
  .row .primary { grid-column: 1 / -1; }
  .btn.armed:focus { background: var(--danger); color: var(--on-accent); }
  .tip { font-size: 1.1rem; color: var(--muted); }
</style>
