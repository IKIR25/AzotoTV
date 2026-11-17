<script lang="ts">
  import { t } from '$lib/i18n';
  import { onMount, tick } from 'svelte';
  import { goto } from '$app/navigation';
  import { showToast } from '$lib/stage.svelte';
  import DiskPicker from '$lib/DiskPicker.svelte';
  import { addGame, steamSearch, type SteamHit } from '$lib/games.svelte';

  // Giochi › Add a game: pick the file that starts it (a Windows .exe, a Linux
  // program, a Switch / Wii / 3DS file), check its name, say which Steam game
  // it is (for its pictures and description; or none) and how many play
  // together. The game stays where it is on disk.
  const ROM = /\.(nsp|xci|rvz|wia|wbfs|iso|3ds|cci|cxi)$/i;
  let exe = $state('');
  let name = $state('');
  let players = $state(1);
  let hits = $state<SteamHit[]>([]);
  let appid = $state<number | null>(null);
  let searching = $state(false);
  let picking = $state(false);
  let busy = $state(false);
  let error = $state('');
  const rom = $derived(ROM.test(exe));

  /** "Hollow_Knight (v1.5) [GOG]" -> "Hollow Knight" */
  const clean = (s: string) =>
    s.replace(/\[[^\]]*\]|\([^)]*\)/g, ' ').replace(/[_]+/g, ' ').replace(/\s+/g, ' ').trim().replace(/^[-.\s]+|[-.\s]+$/g, '');

  async function picked(path: string) {
    picking = false;
    exe = path;
    const parts = path.split('/');
    const file = parts.at(-1) ?? '';
    // a program is named after its folder ("Hollow Knight/hollow_knight.x86_64"), a ROM after itself
    name = clean(ROM.test(file) ? file.replace(/\.[^.]+$/, '') : (parts.at(-2) ?? file));
    appid = null;
    hits = [];
    if (!ROM.test(file)) void search();
    await tick();
    document.querySelector<HTMLElement>('[data-key="name"]')?.focus({ preventScroll: true });
  }

  let searchTimer: ReturnType<typeof setTimeout>;
  async function search() {
    const term = name.trim();
    if (!term) return;
    searching = true;
    try {
      hits = await steamSearch(term);
      // the same name: that's surely it
      const same = hits.find((h) => h.name.toLowerCase() === term.toLowerCase());
      if (same && appid === null) appid = same.appid;
    } catch {
      hits = [];
    }
    searching = false;
  }

  function typed() {
    clearTimeout(searchTimer);
    if (!rom) searchTimer = setTimeout(search, 700);
  }

  function choose(h: SteamHit | null) {
    appid = h?.appid ?? null;
    if (h) name = h.name;
  }

  async function add() {
    if (busy) return;
    busy = true;
    error = '';
    try {
      const id = await addGame({ exe, name, steam_appid: appid, players });
      showToast(t('{name} added: its pictures arrive in a moment', { name }), 3500);
      await goto(`/game/${id}`, { replaceState: true });
    } catch (e) {
      error = String(e);
    }
    busy = false;
  }

  onMount(() => {
    picking = true;
    return () => clearTimeout(searchTimer);
  });
</script>

<div class="wrap">
  <section class="glass">
    <h1>{t('Add a game')}</h1>
    <button class="file" data-focusable data-key="file" onclick={() => (picking = true)}>
      <span class="k">{t('Game file')}</span>
      <span class="v"><bdi>{exe || t('Pick the file that starts the game…')}</bdi></span>
    </button>
    {#if exe}
      <label>
        <span class="k">{t('Name')}</span>
        <input data-focusable data-key="name" bind:value={name} oninput={typed} autocomplete="off" spellcheck="false" />
      </label>
      {#if !rom}
        <div class="k">{t('On Steam')}{#if searching}<em> · {t('looking…')}</em>{/if}</div>
        <div class="hits">
          {#each hits as h (h.appid)}
            <button class="hit" class:sel={appid === h.appid} data-focusable onclick={() => choose(h)}>
              {#if h.image}<img src={h.image} alt="" />{/if}
              <span>{h.name}</span>
            </button>
          {/each}
          <button class="hit none" class:sel={appid === null} data-focusable onclick={() => choose(null)}>
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
      {#if error}<p class="err">{error}</p>{/if}
      <button class="btn primary add" data-focusable onclick={add}>{busy ? t('Adding…') : t('Add')}</button>
    {/if}
  </section>
</div>

{#if picking}
  <DiskPicker
    mode="file"
    title={t('The game\'s file')}
    start={exe ? exe.split('/').slice(0, -1).join('/') : null}
    onpick={picked}
    onclose={async () => {
      picking = false;
      if (!exe) return history.back();
      await tick();
      document.querySelector<HTMLElement>('[data-key="file"]')?.focus({ preventScroll: true });
    }}
  />
{/if}

<style>
  .wrap { position: absolute; inset: 0; display: grid; place-items: center; overflow-y: auto; padding: 3rem 0; }
  :global(.osk-open) .wrap { place-items: start center; }
  section { width: 76rem; border-radius: 2.4rem; padding: 3rem 4rem; display: grid; gap: 1.2rem; }
  h1 { font-size: 2.6rem; font-weight: 600; }
  .k { font-size: 1.3rem; color: var(--muted); }
  .k em { font-style: normal; }
  .file {
    display: grid; gap: 0.4rem; text-align: left; padding: 1.1rem 1.4rem; border-radius: 1.2rem;
    background: rgba(255, 255, 255, 0.1);
  }
  .file .v { font-size: 1.5rem; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left; }
  .file:focus { background: #fff; color: #10121c; }
  .file:focus .k { color: rgba(16, 18, 28, 0.65); }
  label { display: grid; gap: 0.4rem; }
  input {
    font: inherit; font-size: 1.6rem; color: var(--text); padding: 1rem 1.3rem; border-radius: 1.2rem;
    border: 0.2rem solid transparent; background: rgba(255, 255, 255, 0.1); outline: none;
    user-select: text; -webkit-user-select: text; cursor: text;
  }
  input:focus { border-color: #fff; background: rgba(255, 255, 255, 0.16); }
  .hits { display: grid; grid-template-columns: repeat(3, 1fr); gap: 0.8rem; }
  .hit {
    display: flex; align-items: center; gap: 1rem; padding: 0.6rem; border-radius: 1rem; text-align: left;
    background: rgba(255, 255, 255, 0.08); border: 0.2rem solid transparent; min-height: 5.2rem;
  }
  .hit img { width: 9rem; height: 3.4rem; object-fit: cover; border-radius: 0.5rem; flex: none; }
  .hit span { font-size: 1.25rem; font-weight: 600; overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  .hit.none span { padding-left: 0.8rem; }
  .hit.sel { border-color: #6fe3a1; }
  .hit:focus { background: #fff; color: #10121c; }
  .chips { display: flex; gap: 0.8rem; }
  .chip { font-size: 1.4rem; padding: 0.7rem 1.8rem; border-radius: 1.4rem; }
  .chip.sel { background: rgba(255, 255, 255, 0.34); }
  .chip.sel:focus { background: #fff; }
  .err { color: #ff8a8a; font-size: 1.35rem; }
  .add { justify-content: center; margin-top: 0.6rem; }
</style>
