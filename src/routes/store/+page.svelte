<script lang="ts">
  import { tick } from 'svelte';
  import { page } from '$app/state';
  import { APPS } from '$lib/apps';
  import { isInstalled } from '$lib/apps/installed.svelte';
  import { install, uninstall } from '$lib/apps/store.svelte';
  import { auth } from '$lib/session.svelte';
  import { restoreFocus } from '$lib/nav';

  // The Store: every app AzotoTV has, each with what it is and whether it's on
  // this profile's home. A adds or removes it right away (data and pins stay,
  // so removing is never a loss). The order never changes under the cursor.
  const apps = APPS.filter((a) => !a.fixed);
  const count = $derived(apps.filter((a) => isInstalled(auth.id, a.key)).length);

  $effect(() => {
    void auth.id;
    tick().then(() => restoreFocus(page.url.pathname, '.list [data-focusable]'));
  });
</script>

<div class="scroller">
  <header>
    <h1>Store</h1>
    <p>Scegli le app della tua Home. Ogni profilo ha le sue: togliere un'app non cancella niente, e puoi rimetterla quando vuoi.</p>
  </header>

  <div class="list">
    {#each apps as a (a.key)}
      {@const on = isInstalled(auth.id, a.key)}
      <button class="entry glass" class:on data-focusable data-key="app-{a.key}" onclick={() => (on ? uninstall(a) : install(a))}>
        <span class="tile" style="background: {a.color}"><svg viewBox="0 0 48 48" aria-hidden="true">{@html a.icon}</svg></span>
        <span class="txt">
          <strong>{a.name}</strong>
          <span class="d">{a.description}</span>
        </span>
        <span class="state">{on ? '✓ Nella Home' : '＋ Aggiungi'}</span>
      </button>
    {/each}
  </div>
  <p class="foot">{count === apps.length ? 'Hai tutte le app.' : `${count} di ${apps.length} app nella tua Home.`}</p>
</div>

<style>
  header { padding: 3.8rem 6rem 1.6rem; }
  h1 { font-size: 3.6rem; font-weight: 700; letter-spacing: -0.02em; }
  header p { margin-top: 0.8rem; font-size: 1.5rem; color: var(--muted); max-width: 80rem; line-height: 1.5; }
  .list { display: grid; grid-template-columns: repeat(2, 1fr); gap: 1.4rem; padding: 1rem 6rem; max-width: 120rem; }
  .entry { display: flex; align-items: center; gap: 2rem; padding: 1.4rem 1.8rem 1.4rem 1.4rem; border-radius: 1.8rem; text-align: left; transition: background 0.15s; }
  .entry:focus { background: rgba(255, 255, 255, 0.2); box-shadow: 0 0 0 0.3rem #fff; }
  .tile {
    flex: 0 0 12rem; height: 7.2rem; border-radius: 1.4rem; display: grid; place-items: center;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.35), 0 0.6rem 1.4rem rgba(0, 0, 0, 0.35);
  }
  .tile svg { width: 4.4rem; height: 4.4rem; fill: none; stroke: #fff; stroke-width: 2.6; stroke-linecap: round; stroke-linejoin: round; }
  .tile :global(svg .fill) { fill: #fff; stroke-width: 1.6; }
  .txt { flex: 1; display: grid; gap: 0.4rem; min-width: 0; }
  .txt strong { font-size: 1.8rem; }
  .d { font-size: 1.25rem; line-height: 1.45; color: rgba(244, 245, 250, 0.75); }
  .state { flex: 0 0 auto; font-size: 1.25rem; font-weight: 600; padding: 0.6rem 1.2rem; border-radius: 1.2rem; background: rgba(255, 255, 255, 0.12); }
  .entry.on .state { background: transparent; color: var(--muted); }
  .entry:focus .state { background: #fff; color: #10121c; }
  .foot { margin: 1.4rem 6rem 8rem; font-size: 1.3rem; color: var(--muted); }
</style>
