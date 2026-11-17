<script lang="ts">
  import { t } from '$lib/i18n';
  import { onMount, tick } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { auth } from '$lib/session.svelte';
  import { showToast } from '$lib/stage.svelte';
  import { editGame, findGame, loadGames, removeGame, steamSearch, type SteamHit } from '$lib/games.svelte';

  // A game's Edit page (Jellyfin administrators, from the game's page): its
  // name, how many play together, which Steam game it is (pictures and
  // description), looking the pictures up again, taking it out of AzotoTV.
  const id = page.params.id ?? '';
  let name = $state('');
  let players = $state(1);
  let appid = $state<number | null>(null);
  let hits = $state<SteamHit[]>([]);
  let again = $state(false);
  let added = $state<string | null>(null);
  let ready = $state(false);
  let busy = $state(false);
  let error = $state('');
  let armed = $state(false);
  let armTimer: ReturnType<typeof setTimeout>;
  let searchTimer: ReturnType<typeof setTimeout>;
  let isConsole = $state(false);

  onMount(() => {
    void (async () => {
      await loadGames(auth.id);
      const g = findGame(id);
      if (!g) return void (error = t('No such game'));
      name = g.name;
      players = g.players;
      appid = g.steam_appid;
      added = g.added;
      isConsole = g.type === 'switch' || g.type === 'wii' || g.type === '3ds';
      ready = true;
      if (!isConsole) void search();
      await tick();
      document.querySelector<HTMLElement>('.wrap [data-focusable]')?.focus({ preventScroll: true });
    })();
    return () => {
      clearTimeout(armTimer);
      clearTimeout(searchTimer);
    };
  });

  async function search() {
    const term = name.trim();
    if (!term) return;
    try {
      hits = await steamSearch(term);
    } catch {
      hits = [];
    }
  }

  function typed() {
    clearTimeout(searchTimer);
    if (!isConsole) searchTimer = setTimeout(search, 700);
  }

  async function save() {
    if (busy) return;
    busy = true;
    error = '';
    try {
      await editGame(id, { name, players, steam_appid: appid, refresh: again });
      showToast(again ? t('Saved: new pictures arrive in a moment') : t('Saved'), 3000);
      history.back();
    } catch (e) {
      error = String(e);
    }
    busy = false;
  }

  async function remove() {
    if (!armed) {
      armed = true;
      clearTimeout(armTimer);
      armTimer = setTimeout(() => (armed = false), 3500);
      return;
    }
    armed = false;
    try {
      await removeGame(id);
      showToast(t('{name} is no longer in AzotoTV', { name }), 3000);
      await loadGames(auth.id);
      await goto('/giochi', { replaceState: true });
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div class="wrap">
  <section class="glass">
    {#if ready}
      <h1>{t('Edit')}</h1>
      <label>
        <span class="k">{t('Name')}</span>
        <input data-focusable bind:value={name} oninput={typed} autocomplete="off" spellcheck="false" />
      </label>
      {#if !isConsole}
        <div class="k">{t('On Steam')}</div>
        <div class="hits">
          {#each hits as h (h.appid)}
            <button class="hit" class:sel={appid === h.appid} data-focusable onclick={() => (appid = h.appid)}>
              {#if h.image}<img src={h.image} alt="" />{/if}
              <span>{h.name}</span>
            </button>
          {/each}
          {#if appid !== null && !hits.some((h) => h.appid === appid)}
            <button class="hit sel" data-focusable><span>Steam {appid}</span></button>
          {/if}
          <button class="hit none" class:sel={appid === null} data-focusable onclick={() => (appid = null)}>
            <span>{t('Not on Steam')}</span>
          </button>
        </div>
      {/if}
      <div class="k">{t('Players together on this TV')}</div>
      <div class="chips">
        {#each [1, 2, 3, 4] as n (n)}
          <button class="btn chip" class:sel={players === n} data-focusable onclick={() => (players = n)}>{n}</button>
        {/each}
      </div>
      <button class="opt" data-focusable onclick={() => (again = !again)}>
        <span class="txt">{t('Look up pictures and description again')}</span>
        <span class="state">{again ? t('Yes') : 'No'}</span>
      </button>
      {#if error}<p class="err">{error}</p>{/if}
      <div class="row">
        <button class="btn primary" data-focusable onclick={save}>{t('Save')}</button>
        <button class="btn" class:armed data-focusable onblur={() => (armed = false)} onclick={remove}>
          {armed ? t('Press again to take it out') : t('Remove from AzotoTV')}
        </button>
      </div>
      <p class="tip">
        {added ? t('Removing it deletes nothing but what AzotoTV made for it (pictures, description).') : t('Removing it only stops it showing: its folder and files stay where they are.')}
      </p>
    {:else if error}
      <p class="err">{error}</p>
    {/if}
  </section>
</div>

<style>
  .wrap { position: absolute; inset: 0; display: grid; place-items: center; overflow-y: auto; padding: 3rem 0; }
  :global(.osk-open) .wrap { place-items: start center; }
  section { width: 76rem; border-radius: 2.4rem; padding: 3rem 4rem; display: grid; gap: 1.2rem; }
  h1 { font-size: 2.6rem; font-weight: 600; }
  .k { font-size: 1.3rem; color: var(--muted); }
  label { display: grid; gap: 0.4rem; }
  input {
    font: inherit; font-size: 1.6rem; color: var(--text); padding: 1rem 1.3rem; border-radius: 1.2rem;
    border: 0.2rem solid transparent; background: rgb(var(--fg-rgb) / 0.1); outline: none;
    user-select: text; -webkit-user-select: text; cursor: text;
  }
  input:focus { border-color: var(--ring); background: rgb(var(--fg-rgb) / 0.16); }
  .hits { display: grid; grid-template-columns: repeat(3, 1fr); gap: 0.8rem; }
  .hit {
    display: flex; align-items: center; gap: 1rem; padding: 0.6rem; border-radius: 1rem; text-align: left;
    background: rgb(var(--fg-rgb) / 0.08); border: 0.2rem solid transparent; min-height: 5.2rem;
  }
  .hit img { width: 9rem; height: 3.4rem; object-fit: cover; border-radius: 0.5rem; flex: none; }
  .hit span { font-size: 1.25rem; font-weight: 600; overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  .hit.none span { padding-left: 0.8rem; }
  .hit.sel { border-color: var(--ok); }
  .hit:focus { background: var(--focus-bg); color: var(--focus-text); }
  .chips { display: flex; gap: 0.8rem; }
  .chip { font-size: 1.4rem; padding: 0.7rem 1.8rem; border-radius: 1.4rem; }
  .chip.sel { background: rgb(var(--fg-rgb) / 0.34); }
  .chip.sel:focus { background: var(--focus-bg); }
  .opt { display: flex; align-items: center; gap: 1.4rem; text-align: left; padding: 1rem 1.4rem; border-radius: 1.4rem; background: rgb(var(--fg-rgb) / 0.08); }
  .txt { font-size: 1.5rem; font-weight: 600; }
  .state { margin-left: auto; font-size: 1.3rem; font-weight: 700; color: var(--muted); }
  .opt:focus { background: var(--focus-bg); color: var(--focus-text); }
  .opt:focus .state { color: rgb(var(--focus-text-rgb) / 0.65); }
  .row { display: grid; grid-template-columns: 1.2fr 1fr; gap: 1.2rem; margin-top: 0.4rem; }
  .row .btn { justify-content: center; }
  .btn.armed:focus { background: var(--danger); color: var(--on-accent); }
  .err { color: var(--danger-text); font-size: 1.35rem; }
  .tip { font-size: 1.15rem; color: var(--muted); }
</style>
