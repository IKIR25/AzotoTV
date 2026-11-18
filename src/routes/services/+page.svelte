<script lang="ts">
  import { t } from '$lib/i18n';
  import { onMount, tick } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { restoreFocus } from '$lib/nav';
  import { loadServices, restartApp, services, setupFound } from '$lib/services.svelte';
  import { showToast } from '$lib/stage.svelte';

  // START › Services (Jellyfin administrators only): the optional services and
  // whether each is on. A opens one (services/[id]).
  let error = $state('');
  let loading = $state(true);
  let busy = $state(false);
  let results = $state<{ name: string; ok: boolean; text: string }[]>([]);
  const found = $derived(services.list.filter((s) => s.found));

  // every service found complete gets saved (each tried first); the rest say what they need
  async function setupAll() {
    if (busy) return;
    busy = true;
    error = '';
    try {
      const out = await setupFound();
      results = out.map(([name, r]) => ({ name, ok: !!r.Ok, text: r.Ok ? r.Ok.join(' · ') : (r.Err ?? '') }));
      if (results.some((r) => r.ok)) {
        showToast(t('Saved: AzotoTV starts again'), 4000);
        setTimeout(() => void restartApp(), 4000);
      }
    } catch (e) {
      error = String(e);
    }
    busy = false;
  }

  onMount(async () => {
    try {
      await loadServices();
    } catch (e) {
      error = String(e);
    }
    loading = false;
    await tick();
    restoreFocus(page.url.pathname, '.found-all, .list [data-focusable]');
  });
</script>

<div class="wrap">
  <section class="glass">
    <h1>{t('Services')}</h1>
    <p class="lead">{t('What AzotoTV can use besides Jellyfin. Each one is optional.')}</p>
    {#if error}
      <p class="err">{error}</p>
    {:else if loading}
      <p class="lead">{t('Looking for them next to Jellyfin…')}</p>
    {:else}
      {#if found.length}
        <button class="opt found-all" data-focusable data-key="setup-found" onclick={setupAll}>
          <span class="txt">{busy ? t('Setting up…') : t('Set up what was found')}<small>{t('Found next to Jellyfin: {names}', { names: found.map((s) => s.title).join(', ') })}</small></span>
        </button>
        {#if results.length}
          <ul class="results">
            {#each results as r (r.name)}<li class:ok={r.ok}>{r.ok ? '✓' : '·'} <b>{r.name}</b> {r.text}</li>{/each}
          </ul>
        {/if}
      {/if}
      <div class="list" data-remember>
        {#each services.list as s (s.id)}
          <button class="opt" data-focusable data-key={s.id} onclick={() => goto(`/services/${s.id}`)}>
            <span class="txt">{s.title}<small>{s.what}</small></span>
            <span class="state" class:on={s.configured} class:found={s.found}>{s.configured ? t('On|service') : s.found ? t('Found|service') : t('Off|service')}</span>
          </button>
        {/each}
      </div>
    {/if}
  </section>
</div>

<style>
  .wrap { position: absolute; inset: 0; display: grid; place-items: center; }
  section { width: 72rem; border-radius: 2.4rem; padding: 3rem 3rem 2.4rem; }
  h1 { font-size: 2.6rem; font-weight: 600; padding: 0 1.4rem; }
  .lead { font-size: 1.4rem; color: var(--muted); margin: 0.4rem 1.4rem 1.6rem; }
  .err { color: var(--danger-text); font-size: 1.5rem; padding: 0 1.4rem; }
  .opt {
    display: flex; align-items: center; gap: 1.4rem; width: 100%; text-align: left;
    padding: 1.1rem 1.4rem; border-radius: 1.4rem; margin-bottom: 0.3rem;
  }
  .txt { font-size: 1.7rem; font-weight: 600; }
  .txt small { display: block; font-size: 1.2rem; font-weight: 400; color: var(--muted); margin-top: 0.2rem; }
  .state { margin-left: auto; font-size: 1.3rem; font-weight: 700; color: var(--muted); }
  .state.on { color: var(--ok); }
  .state.found { color: var(--accent, var(--text)); }
  .found-all { border: 1px solid var(--glass-edge); margin-bottom: 1rem; }
  .results { list-style: none; font-size: 1.3rem; color: var(--muted); margin: -0.4rem 1.4rem 1.2rem; display: grid; gap: 0.3rem; }
  .results li.ok { color: var(--ok); }
  .opt:focus { background: var(--focus-bg); color: var(--focus-text); }
  .opt:focus small, .opt:focus .state { color: rgb(var(--focus-text-rgb) / 0.65); }
  .opt:focus .state.on { color: var(--ok-on-focus); }
  .opt:focus .state.found { color: var(--accent-on-focus); }
</style>
