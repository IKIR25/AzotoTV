<script lang="ts">
  import { tick } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { auth } from '$lib/session.svelte';
  import { restoreFocus } from '$lib/nav';
  import { showToast } from '$lib/stage.svelte';
  import { displayName, findNew, importFound, lib, linkKnownTomodachi, loadMiis, mainOf, type Found, type PersonEntry } from '$lib/mii/library.svelte';
  import { canDraw } from '$lib/mii/render';
  import { face } from '$lib/mii/pics.svelte';
  import { favorite } from '$lib/mii/colors';
  import { nextTrack, trackName } from '$lib/mii/music.svelte';

  // The Mii app's home: the profile's Miis, white and calm like Nintendo's Mii
  // makers. Miis on the consoles that aren't here yet are offered on top.
  let found = $state<Found[]>([]);
  let drawable = $state(true);
  let importing = $state(false);

  async function refresh() {
    await loadMiis(auth.id);
    found = await findNew().catch(() => []);
    await linkKnownTomodachi().catch((e) => console.warn('[mii] tomodachi', e));
    drawable = await canDraw();
    await tick();
    restoreFocus(page.url.pathname, '.found [data-focusable], .grid [data-focusable]');
  }

  $effect(() => {
    if (auth.id) refresh();
  });

  async function importAll() {
    importing = true;
    try {
      const n = found.length;
      await importFound(found);
      found = [];
      await linkKnownTomodachi().catch(() => {});
      showToast(n === 1 ? '1 Mii aggiunto' : `${n} Mii aggiunti`);
      // the banner is gone: focus the first Mii
      await tick();
      document.querySelector<HTMLElement>('.grid [data-focusable]')?.focus();
    } catch (e) {
      showToast(`Non sono riuscito ad aggiungerli: ${e}`);
    } finally {
      importing = false;
    }
  }

  const consoles = (f: Found[]) => {
    const n = (c: string) => f.filter((x) => x.console === c).length;
    const where = [
      n('switch') && `${n('switch')} su Switch`,
      n('wii') && `${n('wii')} su Wii`,
      n('miitopia') && `${n('miitopia')} in Miitopia`,
      n('tomodachi') && `${n('tomodachi')} in Tomodachi Life`,
    ].filter(Boolean) as string[];
    return where.length > 1 ? `${where.slice(0, -1).join(', ')} e ${where.at(-1)}` : (where[0] ?? '');
  };

  const sub = (p: PersonEntry) => {
    const full = [p.names.first, p.names.surname].filter(Boolean).join(' ');
    const text = p.names.nickname || full;
    return text && text !== displayName(p) ? text : '';
  };
</script>

<div class="mii scroller">
  <header>
    <h1>Mii</h1>
    <div class="line">
      <p>{lib.people.length ? `${lib.people.length} Mii di ${auth.name}` : `I Mii di ${auth.name}`}</p>
      <button class="music" data-focusable data-key="music" onclick={() => nextTrack(auth.id)}>
        ♪ {trackName()}
      </button>
    </div>
  </header>

  {#if found.length}
    <section class="found">
      <div class="faces">
        {#each found.slice(0, 6) as f, i (i)}
          <span class="dot" style="background: {favorite(f.look.general.favoriteColor).hex}"></span>
        {/each}
      </div>
      <div class="txt">
        <strong>{found.length === 1 ? 'Un Mii' : `${found.length} Mii`} non ancora qui</strong>
        <span>{consoles(found)}: {found.map((f) => f.look.meta.name).join(', ')}</span>
      </div>
      <button class="btn" data-focusable data-key="import" disabled={importing} onclick={importAll}>
        {importing ? 'Aggiungo…' : 'Aggiungi tutti'}
      </button>
    </section>
  {/if}

  {#if !drawable}
    <p class="note">Per disegnare i Mii serve il file FFLResHigh.dat della Wii U nella cartella dei dati di AzotoTV (mii/).</p>
  {/if}

  {#if lib.people.length}
    <div class="grid">
      {#each lib.people as p (p.id)}
        {@const m = mainOf(p)}
        {@const c = favorite(m?.look.general.favoriteColor ?? 0).hex}
        {@const pic = drawable && m ? face(m.nx) : null}
        <button class="card" data-focusable data-key="mii-{p.id}" style="--fav: {c}" onclick={() => goto(`/mii/${p.id}`)}>
          <span class="pic">{#if pic}<img src={pic} alt="" />{/if}</span>
          <strong>{displayName(p)}</strong>
          <span class="sub">{sub(p) || ' '}</span>
        </button>
      {/each}
      <button class="card new" data-focusable data-key="mii-new" onclick={() => goto('/mii/editor')}>
        <span class="pic">+</span>
        <strong>Nuovo Mii</strong>
        <span class="sub"> </span>
      </button>
    </div>
  {:else if !found.length}
    <p class="empty">Non hai ancora nessun Mii.</p>
    <div class="grid">
      <button class="card new" data-focusable data-key="mii-new" onclick={() => goto('/mii/editor')}>
        <span class="pic">+</span>
        <strong>Nuovo Mii</strong>
        <span class="sub"> </span>
      </button>
    </div>
  {/if}
</div>

<style>
  /* Nintendo's Mii makers: white, soft greys, colour only from the Miis */
  .mii {
    --ink: #2b2f38;
    --soft: #7b818d;
    --line: #e3e6eb;
    --tint: #3aa6c9;
    position: absolute;
    inset: 0;
    background: linear-gradient(#ffffff, #f1f3f6);
    color: var(--ink);
  }
  header { padding: 3.8rem 6rem 2rem; }
  .line { display: flex; align-items: center; gap: 1.2rem; margin-top: 0.6rem; }
  .line p { margin: 0; }
  /* the music: a small pill next to the count */
  .music {
    font-size: 1.15rem; font-weight: 600; padding: 0.35rem 1.1rem; border-radius: 2rem;
    background: #eef1f4; color: var(--soft);
  }
  .music:focus { background: var(--tint); color: #fff; }
  h1 { font-size: 3.6rem; font-weight: 700; letter-spacing: -0.02em; }
  header p { margin-top: 0.6rem; font-size: 1.5rem; color: var(--soft); }

  .found {
    margin: 0 6rem 2.4rem; padding: 1.6rem 1.8rem; border-radius: 1.8rem;
    display: flex; align-items: center; gap: 2rem;
    background: #fff; border: 1px solid var(--line); box-shadow: 0 0.6rem 2rem rgba(30, 40, 60, 0.06);
  }
  .faces { display: flex; }
  .dot { width: 3.4rem; height: 3.4rem; border-radius: 50%; border: 0.3rem solid #fff; margin-left: -0.9rem; }
  .dot:first-child { margin-left: 0; }
  .found .txt { flex: 1; display: grid; gap: 0.3rem; min-width: 0; }
  .found strong { font-size: 1.6rem; }
  .found .txt span { font-size: 1.25rem; color: var(--soft); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .btn {
    font-size: 1.4rem; font-weight: 600; padding: 1rem 2rem; border-radius: 2rem;
    background: #eef1f4; color: var(--ink); transition: background 0.15s, color 0.15s;
  }
  .btn:focus { background: var(--tint); color: #fff; box-shadow: 0 0 0 0.3rem rgba(58, 166, 201, 0.3); }

  .note, .empty { margin: 0 6rem 2rem; font-size: 1.4rem; color: var(--soft); }

  .grid {
    display: grid; grid-template-columns: repeat(auto-fill, 17rem); gap: 2rem;
    padding: 0 6rem 8rem;
  }
  .card {
    display: grid; justify-items: center; gap: 0.5rem; padding: 1.4rem 1rem 1.4rem;
    border-radius: 2rem; background: #fff; border: 1px solid var(--line);
    box-shadow: 0 0.4rem 1.4rem rgba(30, 40, 60, 0.05);
    transition: box-shadow 0.15s, border-color 0.15s;
  }
  .card:focus { border-color: var(--fav); box-shadow: 0 0 0 0.35rem var(--fav), 0 1rem 2.4rem rgba(30, 40, 60, 0.12); }
  .pic {
    width: 13rem; height: 13rem; border-radius: 50%; overflow: hidden;
    background: radial-gradient(circle at 50% 40%, #ffffff 0%, color-mix(in srgb, var(--fav) 14%, #f4f6f8) 100%);
  }
  .pic img { width: 100%; height: 100%; object-fit: cover; }
  .new .pic { display: grid; place-items: center; font-size: 5rem; font-weight: 300; color: var(--tint); background: #eef8fb; }
  .card strong { margin-top: 0.4rem; font-size: 1.5rem; font-weight: 600; }
  .sub { font-size: 1.15rem; color: var(--soft); min-height: 1.4em; }
</style>
