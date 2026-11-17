<script lang="ts">
  import { t } from '$lib/i18n';
  import { onMount, tick } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { restoreFocus } from '$lib/nav';
  import { loadServices, services } from '$lib/services.svelte';

  // START › Services (Jellyfin administrators only): the optional services and
  // whether each is on. A opens one (services/[id]).
  let error = $state('');

  onMount(async () => {
    try {
      await loadServices();
    } catch (e) {
      error = String(e);
    }
    await tick();
    restoreFocus(page.url.pathname, '.list [data-focusable]');
  });
</script>

<div class="wrap">
  <section class="glass">
    <h1>{t('Services')}</h1>
    <p class="lead">{t('What AzotoTV can use besides Jellyfin. Each one is optional.')}</p>
    {#if error}
      <p class="err">{error}</p>
    {:else}
      <div class="list" data-remember>
        {#each services.list as s (s.id)}
          <button class="opt" data-focusable data-key={s.id} onclick={() => goto(`/services/${s.id}`)}>
            <span class="txt">{s.title}<small>{s.what}</small></span>
            <span class="state" class:on={s.configured}>{s.configured ? t('On|service') : t('Off|service')}</span>
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
  .err { color: #ff8a8a; font-size: 1.5rem; padding: 0 1.4rem; }
  .opt {
    display: flex; align-items: center; gap: 1.4rem; width: 100%; text-align: left;
    padding: 1.1rem 1.4rem; border-radius: 1.4rem; margin-bottom: 0.3rem;
  }
  .txt { font-size: 1.7rem; font-weight: 600; }
  .txt small { display: block; font-size: 1.2rem; font-weight: 400; color: var(--muted); margin-top: 0.2rem; }
  .state { margin-left: auto; font-size: 1.3rem; font-weight: 700; color: var(--muted); }
  .state.on { color: #6fe3a1; }
  .opt:focus { background: #fff; color: #10121c; }
  .opt:focus small, .opt:focus .state { color: rgba(16, 18, 28, 0.65); }
  .opt:focus .state.on { color: #128a4a; }
</style>
