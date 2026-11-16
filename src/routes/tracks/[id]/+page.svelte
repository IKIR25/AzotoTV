<script lang="ts">
  import { tick } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api, img, type Item } from '$lib/jf';
  import { refresh, showToast, stage } from '$lib/stage.svelte';
  import { restoreFocus } from '$lib/nav';
  import { openKeyboard } from '$lib/osk.svelte';
  import { clock, music, playTracks } from '$lib/music.svelte';
  import PinButton from '$lib/PinButton.svelte';
  import { itemPin } from '$lib/pins.svelte';

  // A music playlist (this profile's own): its songs, "Riproduci" / "Casuale",
  // A on a song plays from there, ✕ next to it takes it out. "Aggiungi brani"
  // opens Musica's search for this playlist; "Rinomina" types a new name;
  // "Elimina" asks for a second A within a few seconds (no dialog).
  const id = $derived(page.params.id ?? '');
  let list = $state<Item | null>(null);
  let items = $state<Item[]>([]);

  const total = $derived(items.reduce((t, x) => t + (x.RunTimeTicks ?? 0), 0) / 1e7);
  const cover = (it: Item | null, rem: number) =>
    it?.ImageTags?.Primary ? img(it.Id, 'Primary', rem, it.ImageTags.Primary) : null;
  const artist = (it: Item) => it.AlbumArtist ?? it.Artists?.[0] ?? '';
  const playing = $derived(music.active ? music.queue[music.index]?.Id : undefined);
  const pin = $derived(list ? itemPin(list) : null);

  $effect(() => {
    const pid = id;
    void refresh.n;
    Promise.all([api.item(pid), api.playlistItems(pid)]).then(async ([it, p]) => {
      if (pid !== id) return;
      list = it;
      items = p.Items;
      stage.backdrop = cover(items[0] ?? it, 40);
      stage.ambient = true;
      await tick();
      restoreFocus(page.url.pathname, '[data-key="play"]');
    });
  });

  async function reload() {
    const p = await api.playlistItems(id);
    items = p.Items;
  }

  // ---- rename ----
  let renaming = $state(false);
  let newName = $state('');
  let nameInput: HTMLInputElement | undefined = $state();
  async function startRename() {
    newName = list?.Name ?? '';
    renaming = true;
    await tick();
    nameInput?.focus({ preventScroll: true });
    if (nameInput) openKeyboard(nameInput);
  }
  async function rename(e: SubmitEvent) {
    e.preventDefault();
    const n = newName.trim();
    renaming = false;
    if (n && list && n !== list.Name) {
      try {
        await api.renamePlaylist(list.Id, n);
        list = { ...list, Name: n };
      } catch (err) {
        showToast(`Non riuscito: ${err}`, 4000);
      }
    }
    await tick();
    document.querySelector<HTMLElement>('[data-key="rename"]')?.focus({ preventScroll: true });
  }

  // ---- delete: a second A within 4 s ----
  let confirmDelete = $state(false);
  let delTimer: ReturnType<typeof setTimeout>;
  async function remove() {
    clearTimeout(delTimer);
    if (!confirmDelete) {
      confirmDelete = true;
      delTimer = setTimeout(() => (confirmDelete = false), 4000);
      return;
    }
    confirmDelete = false;
    try {
      await api.deletePlaylist(id);
      showToast(`Playlist «${list?.Name}» eliminata`);
      history.back();
    } catch (err) {
      showToast(`Non riuscito: ${err}`, 4000);
    }
  }
  $effect(() => () => clearTimeout(delTimer));

  async function takeOut(it: Item, i: number) {
    if (!it.PlaylistItemId) return;
    try {
      await api.removeFromPlaylist(id, [it.PlaylistItemId]);
      showToast(`${it.Name}: tolto dalla playlist`);
      await reload();
      await tick();
      // stay on the same row (now the next song), or the last one
      const n = Math.min(i, items.length - 1);
      document.querySelector<HTMLElement>(n >= 0 ? `[data-key="r${n}"]` : '[data-key="add"]')?.focus({ preventScroll: true });
    } catch (err) {
      showToast(`Non riuscito: ${err}`, 4000);
    }
  }

  const addSongs = () => goto(`/musica/cerca?playlist=${id}&name=${encodeURIComponent(list?.Name ?? '')}`);

  const minutes = (s: number) => (s >= 3600 ? `${Math.floor(s / 3600)} h ${Math.round((s % 3600) / 60)} min` : `${Math.round(s / 60)} min`);
</script>

<div class="scroller">
  {#if list}
    <header>
      <div class="art">
        {#if cover(list, 18)}<img src={cover(list, 18)} alt="" />{:else}<span>♪</span>{/if}
      </div>
      <div class="info">
        <p class="kind">Playlist</p>
        {#if renaming}
          <form onsubmit={rename}>
            <input class="name" bind:this={nameInput} bind:value={newName} data-focusable data-osk-submit aria-label="Nome della playlist" autocomplete="off" />
          </form>
        {:else}
          <h1>{list.Name}</h1>
        {/if}
        <p class="meta">{items.length ? `${items.length} brani · ${minutes(total)}` : 'Ancora vuota'}</p>
        <div class="actions">
          <button class="btn primary" data-focusable data-key="play" data-scroll-top onclick={() => playTracks(items, 0)}>
            <svg viewBox="0 0 24 24"><path d="M8 5v14l11-7z" /></svg> Riproduci
          </button>
          <button class="btn" data-focusable data-key="shuffle" data-scroll-top onclick={() => playTracks(items, 0, { shuffle: true })}>
            <svg viewBox="0 0 24 24"
              ><path
                d="M10.59 9.17 5.41 4 4 5.41l5.17 5.17zM14.5 4l2.04 2.04L4 18.59 5.41 20 17.96 7.46 20 9.5V4zm.33 9.41-1.41 1.41 3.13 3.13L14.5 20H20v-5.5l-2.04 2.04z"
              /></svg
            > Casuale
          </button>
          {#if pin}<PinButton {pin} />{/if}
        </div>
        <div class="actions small">
          <button class="btn" data-focusable data-key="add" data-scroll-top onclick={addSongs}>＋ Aggiungi brani</button>
          <button class="btn" data-focusable data-key="rename" data-scroll-top onclick={startRename}>Rinomina</button>
          <button class="btn" class:warn={confirmDelete} data-focusable data-key="delete" data-scroll-top onclick={remove}>
            {confirmDelete ? 'Premi di nuovo per eliminarla' : 'Elimina'}
          </button>
        </div>
      </div>
    </header>

    <ol class="songs">
      {#each items as it, i (it.PlaylistItemId ?? i)}
        <li>
          <button class="song" data-focusable data-key={`s${i}`} onclick={() => playTracks(items, i)}>
            <span class="n">{#if playing === it.Id}<b class="eq">♪</b>{:else}{i + 1}{/if}</span>
            <span class="thumb">{#if cover(it, 7)}<img src={cover(it, 7)} alt="" loading="lazy" />{/if}</span>
            <span class="txt">{it.Name}<small>{artist(it)}</small></span>
            <span class="dur">{clock((it.RunTimeTicks ?? 0) / 1e7)}</span>
          </button>
          {#if it.PlaylistItemId}
            <button class="rm" data-focusable data-key={`r${i}`} aria-label="Togli dalla playlist" onclick={() => takeOut(it, i)}>✕</button>
          {/if}
        </li>
      {/each}
    </ol>
  {/if}
</div>

<style>
  header { display: flex; align-items: flex-end; gap: 3.2rem; padding: 11rem 6rem 2.4rem; }
  .art {
    flex: 0 0 18rem; height: 18rem; border-radius: 1.6rem; overflow: hidden; display: grid; place-items: center;
    background: linear-gradient(160deg, #2a3157, #151833); box-shadow: 0 1.6rem 3.6rem rgba(0, 0, 0, 0.5);
    font-size: 7rem; color: rgba(255, 255, 255, 0.35);
  }
  .art img { width: 100%; height: 100%; object-fit: cover; }
  .kind { font-size: 1.2rem; text-transform: uppercase; letter-spacing: 0.18em; color: var(--muted); }
  h1 { font-size: 3.6rem; font-weight: 700; letter-spacing: -0.02em; margin: 0.3rem 0 0.4rem; }
  .meta { font-size: 1.4rem; color: var(--muted); }
  .actions { display: flex; gap: 1rem; margin-top: 1.8rem; }
  .actions svg { width: 1.9rem; height: 1.9rem; fill: currentColor; }
  .actions.small { margin-top: 1rem; }
  .actions.small .btn { font-size: 1.25rem; padding: 0.7rem 1.4rem; }
  .btn.warn { background: #e0162b; color: #fff; }
  .btn.warn:focus { background: #ff4e45; color: #fff; }
  .name {
    font: inherit; font-size: 3.2rem; font-weight: 700; color: var(--text); margin: 0.3rem 0 0.4rem; padding: 0.2rem 1rem;
    width: 60rem; border-radius: 1rem; border: 0.2rem solid #fff; background: rgba(255, 255, 255, 0.12); outline: none;
  }
  li { display: flex; align-items: center; gap: 0.8rem; }
  .rm {
    flex: 0 0 4rem; height: 4rem; border-radius: 50%; font-size: 1.4rem; color: var(--muted);
  }
  .rm:focus { background: #e0162b; color: #fff; }

  .songs { list-style: none; padding: 0.6rem 6rem 8rem; }
  .song {
    flex: 1; min-width: 0; display: flex; align-items: center; gap: 1.6rem; padding: 0.7rem 1.4rem;
    border-radius: 1.2rem; text-align: left;
  }
  .song:focus { background: rgba(255, 255, 255, 0.92); color: #10121c; }
  .song:focus small, .song:focus .dur, .song:focus .n { color: rgba(16, 18, 28, 0.7); }
  .n { flex: 0 0 2.6rem; text-align: right; font-size: 1.2rem; color: var(--muted); font-variant-numeric: tabular-nums; }
  .eq { color: var(--accent); font-size: 1.5rem; }
  .thumb { flex: 0 0 7.2rem; height: 4.05rem; border-radius: 0.6rem; overflow: hidden; background: linear-gradient(160deg, #2a3157, #151833); }
  .thumb img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .txt { flex: 1; min-width: 0; font-size: 1.4rem; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .txt small { display: block; font-size: 1.15rem; font-weight: 400; color: var(--muted); overflow: hidden; text-overflow: ellipsis; }
  .dur { font-size: 1.2rem; color: var(--muted); font-variant-numeric: tabular-nums; }
</style>
