<script lang="ts">
  import { t } from '$lib/i18n';
  import { onMount, tick } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { showToast } from '$lib/stage.svelte';
  import { findPeer, linkPeer, peerStatus, unlinkPeer, type Found, type PeerState } from '$lib/peers.svelte';

  // START › Linked AzotoTVs (Jellyfin administrators): link this AzotoTV with
  // another one on the home network (the one next to Jellyfin is offered by
  // itself), see which are on, unlink one (A twice).
  let list = $state<PeerState[]>([]);
  let found = $state<Found | null>(null);
  let address = $state('');
  let busy = $state(false);
  let loading = $state(true);
  let error = $state('');
  let armed = $state('');
  let armTimer: ReturnType<typeof setTimeout>;

  async function load() {
    const [l, f] = await Promise.all([peerStatus(), findPeer().catch(() => null)]);
    list = l;
    found = f && !f.linked ? f : null;
    loading = false;
  }

  onMount(() => {
    void load().then(async () => {
      await tick();
      document.querySelector<HTMLElement>('.wrap [data-focusable]')?.focus({ preventScroll: true });
    });
    const un = listen('peers-changed', () => void load());
    return () => {
      clearTimeout(armTimer);
      void un.then((f) => f());
    };
  });

  async function link(url: string) {
    if (busy || !url.trim()) return;
    busy = true;
    error = '';
    try {
      const p = await linkPeer(url);
      showToast(t('Linked with {name}', { name: p.name }), 3000);
      address = '';
      await load();
    } catch (e) {
      error = String(e);
    }
    busy = false;
  }

  async function unlink(p: PeerState) {
    if (armed !== p.id) {
      armed = p.id;
      clearTimeout(armTimer);
      armTimer = setTimeout(() => (armed = ''), 3500);
      return;
    }
    armed = '';
    try {
      await unlinkPeer(p.id);
      await load();
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div class="wrap">
  <section class="glass">
    <h1>{t('Linked AzotoTVs')}</h1>
    <p class="lead">{t('Another AzotoTV on the home network (the TV, a computer): each one sees the games of the other and can download them, to play with its own hardware.')}</p>

    {#if loading}
      <p class="lead">{t('Looking for them next to Jellyfin…')}</p>
    {:else}
      {#if found}
        <button class="opt found" data-focusable onclick={() => link(found!.url)}>
          <span class="txt">{busy ? t('Linking…') : t('Link with {name}', { name: found.name })}<small>{t('Found next to Jellyfin: {url}', { url: found.url })}</small></span>
        </button>
      {/if}

      {#each list as p (p.id)}
        <button class="opt" data-focusable onblur={() => armed === p.id && (armed = '')} onclick={() => unlink(p)}>
          <span class="txt">{p.name}<small>{armed === p.id ? t('Press again to unlink it') : p.url}</small></span>
          <span class="state" class:on={p.online}>{p.online ? t('On|service') : t('Not answering')}</span>
        </button>
      {/each}
      {#if !list.length && !found}
        <p class="lead">{t('None linked yet.')}</p>
      {/if}

      <form class="manual" onsubmit={(e) => (e.preventDefault(), link(address))}>
        <input data-focusable data-osk-submit bind:value={address} placeholder={t('Address of another AzotoTV (e.g. 192.168.1.20)')} autocomplete="off" spellcheck="false" />
        <button class="btn" data-focusable>{t('Link')}</button>
      </form>
      {#if error}<p class="err">{error}</p>{/if}
      <p class="tip">{t('Both have to use the same Jellyfin, and you have to be one of its administrators. On a computer with a firewall, port 8686 has to be open to the home network, or the other one can\'t see its games.')}</p>
    {/if}
  </section>
</div>

<style>
  .wrap { position: absolute; inset: 0; display: grid; place-items: center; }
  :global(.osk-open) .wrap { place-items: start center; padding-top: 4rem; }
  section { width: 72rem; border-radius: 2.4rem; padding: 3rem 3rem 2.4rem; }
  h1 { font-size: 2.6rem; font-weight: 600; padding: 0 1.4rem; }
  .lead { font-size: 1.4rem; color: var(--muted); margin: 0.4rem 1.4rem 1.6rem; }
  .err { color: var(--danger-text); font-size: 1.4rem; padding: 0 1.4rem; margin-top: 1rem; }
  .tip { font-size: 1.15rem; color: var(--muted); margin: 1.6rem 1.4rem 0; }
  .opt {
    display: flex; align-items: center; gap: 1.4rem; width: 100%; text-align: left;
    padding: 1.1rem 1.4rem; border-radius: 1.4rem; margin-bottom: 0.3rem;
  }
  .opt.found { border: 1px solid var(--glass-edge); margin-bottom: 1rem; }
  .txt { font-size: 1.7rem; font-weight: 600; }
  .txt small { display: block; font-size: 1.2rem; font-weight: 400; color: var(--muted); margin-top: 0.2rem; }
  .state { margin-left: auto; font-size: 1.3rem; font-weight: 700; color: var(--muted); }
  .state.on { color: var(--ok); }
  .opt:focus { background: var(--focus-bg); color: var(--focus-text); }
  .opt:focus small, .opt:focus .state { color: rgb(var(--focus-text-rgb) / 0.65); }
  .opt:focus .state.on { color: var(--ok-on-focus); }
  .manual { display: flex; gap: 1rem; margin: 1.4rem 1.4rem 0; }
  .manual input {
    flex: 1; font: inherit; font-size: 1.5rem; padding: 1rem 1.4rem; border-radius: 1.2rem;
    background: rgb(var(--fg-rgb) / 0.08); border: 1px solid var(--glass-edge); color: var(--text);
  }
  .manual input:focus { outline: none; box-shadow: 0 0 0 0.25rem var(--ring); }
</style>
