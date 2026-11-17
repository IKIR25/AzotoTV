<script lang="ts">
  import { t, tn } from '$lib/i18n';
  import { onMount, tick } from 'svelte';
  import { showToast } from '$lib/stage.svelte';
  import DiskPicker from '$lib/DiskPicker.svelte';
  import { gameFolders, scanGames, setGameFolders, type Folders } from '$lib/games.svelte';

  // Giochi › Game folders: where AzotoTV looks for games. Game folders hold one
  // folder per game (the first one is where added games go); ROM folders are
  // searched for Switch, Wii and 3DS files, which then show up by themselves.
  // A on a folder takes it out (a second A within a few seconds); nothing on
  // disk is touched.
  let f = $state<Folders | null>(null);
  let error = $state('');
  let picking = $state<'folders' | 'roms' | null>(null);
  let armed = $state('');
  let armTimer: ReturnType<typeof setTimeout>;

  onMount(() => {
    void (async () => {
      try {
        f = await gameFolders();
      } catch (e) {
        error = String(e);
      }
      await tick();
      document.querySelector<HTMLElement>('.wrap [data-focusable]')?.focus({ preventScroll: true });
    })();
    return () => clearTimeout(armTimer);
  });

  async function save(folders: string[], roms: string[], unhide = false) {
    error = '';
    try {
      f = await setGameFolders(folders, roms, unhide);
      const n = await scanGames();
      if (n) showToast(tn(n, 'Found {n} new game', 'Found {n} new games', { n }), 3500);
      return true;
    } catch (e) {
      error = String(e);
      return false;
    }
  }

  async function take(kind: 'folders' | 'roms', dir: string) {
    const k = `${kind}:${dir}`;
    if (armed !== k) {
      armed = k;
      clearTimeout(armTimer);
      armTimer = setTimeout(() => (armed = ''), 3500);
      return;
    }
    armed = '';
    if (!f) return;
    const folders = kind === 'folders' ? f.folders.filter((d) => d !== dir) : f.folders;
    const roms = kind === 'roms' ? f.roms.filter((d) => d !== dir) : f.roms;
    if (await save(folders, roms)) {
      await tick();
      document.querySelector<HTMLElement>(`[data-key="add-${kind}"]`)?.focus({ preventScroll: true });
    }
  }

  async function picked(dir: string) {
    const kind = picking!;
    picking = null;
    if (!f) return;
    const folders = kind === 'folders' ? [...f.folders, dir] : f.folders;
    const roms = kind === 'roms' ? [...f.roms, dir] : f.roms;
    await save(folders, roms);
    await tick();
    document.querySelector<HTMLElement>(`[data-key="add-${kind}"]`)?.focus({ preventScroll: true });
  }
</script>

<div class="wrap">
  <section class="glass">
    <h1>{t('Game folders')}</h1>
    {#if f}
      <h2>{t('Games')}</h2>
      <p class="lead">{t('Folders with one folder per game in them. New games are added to the first one.')}</p>
      {#each f.folders as dir, i (dir)}
        <button class="opt" class:armed={armed === `folders:${dir}`} data-focusable onblur={() => armed === `folders:${dir}` && (armed = '')} onclick={() => take('folders', dir)}>
          <span class="txt">{dir}</span>
          <span class="state">{armed === `folders:${dir}` ? t('Press again to take it out') : i === 0 ? t('main') : ''}</span>
        </button>
      {/each}
      <button class="opt add" data-focusable data-key="add-folders" onclick={() => (picking = 'folders')}>
        <span class="txt">{t('＋ Add a folder')}</span>
      </button>

      <h2>{t('ROMs')}</h2>
      <p class="lead">{t('Searched, subfolders too, for Switch (.nsp, .xci), Wii (.rvz, .wbfs, .iso) and 3DS (.3ds, .cci) games: new ones show up in Games by themselves.')}</p>
      {#each f.roms as dir (dir)}
        <button class="opt" class:armed={armed === `roms:${dir}`} data-focusable onblur={() => armed === `roms:${dir}` && (armed = '')} onclick={() => take('roms', dir)}>
          <span class="txt">{dir}</span>
          <span class="state">{armed === `roms:${dir}` ? t('Press again to take it out') : ''}</span>
        </button>
      {/each}
      <button class="opt add" data-focusable data-key="add-roms" onclick={() => (picking = 'roms')}>
        <span class="txt">{t('＋ Add a folder')}</span>
      </button>
      {#if f.hidden}
        <button
          class="opt add"
          data-focusable
          onclick={async () => {
            if (!f || !(await save(f.folders, f.roms, true))) return;
            await tick(); // this button is gone now
            document.querySelector<HTMLElement>('[data-key="add-roms"]')?.focus({ preventScroll: true });
          }}
        >
          <span class="txt">{tn(f.hidden, 'Bring back {n} removed ROM', 'Bring back {n} removed ROMs', { n: f.hidden })}</span>
        </button>
      {/if}
      <p class="tip">{t('Taking a folder out only stops AzotoTV looking there: nothing is deleted.')}</p>
    {/if}
    {#if error}<p class="err">{error}</p>{/if}
  </section>
</div>

{#if picking}
  <DiskPicker
    mode="dir"
    title={picking === 'roms' ? t('A folder with ROMs') : t('A folder with games')}
    onpick={picked}
    onclose={async () => {
      const kind = picking;
      picking = null;
      await tick();
      document.querySelector<HTMLElement>(`[data-key="add-${kind}"]`)?.focus({ preventScroll: true });
    }}
  />
{/if}

<style>
  .wrap { position: absolute; inset: 0; display: grid; place-items: center; overflow-y: auto; padding: 3rem 0; }
  section { width: 76rem; border-radius: 2.4rem; padding: 3rem 3rem 2.4rem; }
  h1 { font-size: 2.6rem; font-weight: 600; padding: 0 1.4rem; }
  h2 { font-size: 1.8rem; font-weight: 600; padding: 0 1.4rem; margin-top: 1.8rem; }
  .lead { font-size: 1.25rem; color: var(--muted); margin: 0.3rem 1.4rem 0.8rem; }
  .opt {
    display: flex; align-items: center; gap: 1.4rem; width: 100%; text-align: left;
    padding: 1rem 1.4rem; border-radius: 1.4rem; margin-bottom: 0.3rem;
  }
  .txt { font-size: 1.5rem; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .add .txt { color: var(--muted); }
  .state { margin-left: auto; font-size: 1.2rem; font-weight: 700; color: var(--muted); flex: none; }
  .opt:focus { background: var(--focus-bg); color: var(--focus-text); }
  .opt:focus .txt, .opt:focus .state { color: var(--focus-text); }
  .opt.armed:focus { background: var(--danger); color: var(--on-accent); }
  .opt.armed:focus .txt, .opt.armed:focus .state { color: var(--on-accent); }
  .tip { font-size: 1.15rem; color: var(--muted); margin: 1.4rem 1.4rem 0; }
  .err { color: var(--danger-text); font-size: 1.35rem; margin: 1rem 1.4rem 0; }
</style>
