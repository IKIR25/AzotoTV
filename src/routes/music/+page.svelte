<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { goto } from '$app/navigation';
  import { type Item } from '$lib/jf';
  import { stage } from '$lib/stage.svelte';
  import { clock, cycleRepeat, jump, music, next, prev, stopMusic, toggleShuffle, togglePause } from '$lib/music.svelte';
  import { addToPlaylist, songCover } from '$lib/songs.svelte';
  import PlaylistSheet from '$lib/PlaylistSheet.svelte';
  import { input } from '$lib/input.svelte';

  const cur = $derived<Item | null>(music.queue[music.index] ?? null);
  // what comes next (a window, so a huge queue stays cheap to draw)
  const upcoming = $derived(music.queue.slice(music.index + 1, music.index + 41));
  const pct = $derived(music.duration ? Math.min(100, (music.shown / music.duration) * 100) : 0);

  const cover = (it: Item | null, rem: number) => songCover(it, rem);
  // "＋ Playlist" on the song playing (a YouTube one is downloaded into it)
  let picking = $state<Item | null>(null);
  async function closeSheet() {
    picking = null;
    await tick();
    document.querySelector<HTMLElement>('[data-key="addpl"]')?.focus({ preventScroll: true });
  }
  const artist = (it: Item) => it.AlbumArtist ?? it.Artists?.[0] ?? '';

  $effect(() => {
    stage.backdrop = cover(cur, 40); // small is fine: it's blurred
    stage.ambient = true;
  });

  let pp: HTMLButtonElement | undefined = $state();
  onMount(async () => {
    await tick();
    pp?.focus({ preventScroll: true });
  });
  // the focused queue row can vanish when the song changes: don't leave focus nowhere
  $effect(() => {
    void music.index;
    tick().then(() => {
      if (!document.activeElement || document.activeElement === document.body) pp?.focus({ preventScroll: true });
    });
  });

  const browse = () => goto('/musica');
</script>

{#if cur}
  <div class="np">
    <section class="left">
      <div class="art">
        {#if cover(cur, 52)}
          <img src={cover(cur, 52)} alt="" />
        {:else}
          <div class="ph">♪</div>
        {/if}
      </div>
      <h1>{cur.Name}</h1>
      <p class="by">{artist(cur)}</p>

      <div class="bar"><b style="transform:scaleX({pct / 100})"></b></div>
      <div class="times">
        <span>{clock(music.shown)}</span>
        <span>-{clock(Math.max(0, music.duration - music.shown))}</span>
      </div>

      <div class="controls">
        <button
          class="btn icon ctl"
          class:on={music.shuffle}
          data-focusable
          data-key="shuffle"
          aria-label="Casuale"
          onclick={toggleShuffle}
        >
          <svg viewBox="0 0 24 24"
            ><path
              d="M10.59 9.17 5.41 4 4 5.41l5.17 5.17zM14.5 4l2.04 2.04L4 18.59 5.41 20 17.96 7.46 20 9.5V4zm.33 9.41-1.41 1.41 3.13 3.13L14.5 20H20v-5.5l-2.04 2.04z"
            /></svg
          >
        </button>
        <button class="btn icon ctl" data-focusable data-key="prev" aria-label="Precedente" onclick={prev}>
          <svg viewBox="0 0 24 24"><path d="M6 6h2v12H6zm3.5 6L18 6v12z" /></svg>
        </button>
        <button
          class="btn icon ctl big"
          data-focusable
          data-key="pp"
          aria-label={music.active && !music.paused ? 'Pausa' : 'Riproduci'}
          bind:this={pp}
          onclick={togglePause}
        >
          {#if music.active && !music.paused}
            <svg viewBox="0 0 24 24"><path d="M6 5h4v14H6zm8 0h4v14h-4z" /></svg>
          {:else}
            <svg viewBox="0 0 24 24"><path d="M8 5v14l11-7z" /></svg>
          {/if}
        </button>
        <button class="btn icon ctl" data-focusable data-key="next" aria-label="Successivo" onclick={next}>
          <svg viewBox="0 0 24 24"><path d="M16 6h2v12h-2zM6 18V6l8.5 6z" /></svg>
        </button>
        <button
          class="btn icon ctl"
          class:on={music.repeat !== 'off'}
          data-focusable
          data-key="repeat"
          aria-label="Ripeti"
          onclick={cycleRepeat}
        >
          <svg viewBox="0 0 24 24"><path d="M7 7h10v3l4-4-4-4v3H5v6h2zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2z" /></svg>
          {#if music.repeat === 'one'}<span class="one">1</span>{/if}
        </button>
        <button
          class="btn icon ctl"
          data-focusable
          data-key="stop"
          aria-label="Ferma"
          onclick={async () => {
            await stopMusic();
            history.back();
          }}
        >
          <svg viewBox="0 0 24 24"><path d="M7 7h10v10H7z" /></svg>
        </button>
        <button class="btn icon ctl" data-focusable data-key="addpl" aria-label="Aggiungi a una playlist" onclick={() => (picking = cur)}>
          <svg viewBox="0 0 24 24"><path d="M3 6h12v2H3zm0 4h12v2H3zm0 4h8v2H3zm14 0v-4h2v4h4v2h-4v4h-2v-4h-4v-2z" /></svg>
        </button>
      </div>
      <p class="hint">
        {#if input.remote}
          OK pausa · ■ ferma · ≡+ in una playlist · ⏮ ⏭ brano precedente / successivo · ⏪ ⏩ −15 / +15 s
        {:else}
          A pausa · ■ ferma · ≡+ in una playlist · LB / RB brano precedente / successivo · LT / RT −15 / +15 s
        {/if}
      </p>
    </section>

    <section class="queue glass">
      <h2>
        In coda <small>{music.queue.length - music.index - 1 > 0 ? `${music.queue.length - music.index - 1} brani` : ''}</small>
      </h2>
      <div class="list">
        <div class="scroller">
          {#each upcoming as it, k (music.index + 1 + k)}
            <button class="row" data-focusable data-key={`q${k}`} onclick={() => jump(music.index + 1 + k)}>
              <span class="thumb">
                {#if cover(it, 7)}<img src={cover(it, 7)} alt="" loading="lazy" />{/if}
              </span>
              <span class="txt">{it.Name}<small>{artist(it)}</small></span>
              <span class="dur">{clock((it.RunTimeTicks ?? 0) / 1e7)}</span>
            </button>
          {:else}
            <p class="empty">
              {music.repeat === 'all' ? 'Poi si ricomincia dal primo brano' : 'Nient’altro in coda'}
            </p>
          {/each}
        </div>
      </div>
    </section>
  </div>
  {#if picking}
    <PlaylistSheet
      onpick={async (pid) => {
        const it = picking;
        await closeSheet();
        if (it) addToPlaylist(pid, it);
      }}
      onclose={closeSheet}
    />
  {/if}
{:else}
  <div class="none">
    <p>Niente in riproduzione</p>
    <button class="btn primary" data-focusable data-key="browse" onclick={browse}>Sfoglia la musica</button>
  </div>
{/if}

<style>
  .np { position: absolute; inset: 0; display: flex; gap: 5rem; padding: 11rem 6rem 4.5rem; }
  .left { flex: 0 0 54rem; display: flex; flex-direction: column; justify-content: flex-end; }
  .left > * { flex-shrink: 0; }
  .art { height: 26rem; display: flex; align-items: flex-end; margin-bottom: 2rem; }
  .art img, .ph {
    max-width: 54rem; max-height: 26rem; border-radius: 1.6rem;
    box-shadow: 0 2.4rem 5rem rgba(0, 0, 0, 0.55);
  }
  .ph {
    width: 26rem; height: 26rem; display: grid; place-items: center; font-size: 10rem; opacity: 0.4;
    background: linear-gradient(160deg, #2a3157, #151833);
  }
  h1 {
    font-size: 2.8rem; font-weight: 700; letter-spacing: -0.01em; line-height: 1.15;
    display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
  }
  .by { font-size: 1.7rem; color: var(--muted); margin-top: 0.4rem; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  .bar { margin-top: 2rem; height: 0.6rem; border-radius: 0.3rem; background: rgba(255, 255, 255, 0.22); overflow: hidden; }
  .bar b { display: block; height: 100%; background: #fff; transform-origin: left; }
  .times {
    display: flex; justify-content: space-between; margin-top: 0.7rem;
    font-size: 1.25rem; color: var(--muted); font-variant-numeric: tabular-nums;
  }

  .controls { display: flex; align-items: center; justify-content: center; gap: 1.6rem; margin-top: 1.6rem; }
  .ctl { position: relative; width: 5.2rem; height: 5.2rem; padding: 0; justify-content: center; border-radius: 50%; background: rgba(255, 255, 255, 0.1); }
  .ctl svg { width: 2.6rem; height: 2.6rem; fill: currentColor; }
  .ctl.big { width: 7rem; height: 7rem; background: rgba(255, 255, 255, 0.24); }
  .ctl.big svg { width: 3.6rem; height: 3.6rem; }
  .ctl:not(.on):not(.big):not(:focus) { color: var(--muted); }
  .ctl[data-key='prev']:not(:focus), .ctl[data-key='next']:not(:focus) { color: var(--text); }
  .ctl.on::after {
    content: ''; position: absolute; bottom: 0.55rem; left: 50%; width: 0.45rem; height: 0.45rem;
    margin-left: -0.225rem; border-radius: 50%; background: currentColor;
  }
  .one { position: absolute; top: 1.1rem; right: 1.1rem; font-size: 0.95rem; font-weight: 800; }
  .hint { margin-top: 1.4rem; text-align: center; font-size: 1.1rem; color: var(--muted); }

  .queue { flex: 1; position: relative; border-radius: 2.2rem; display: flex; flex-direction: column; min-width: 0; }
  h2 { font-size: 1.8rem; font-weight: 600; padding: 2rem 2.4rem 1rem; }
  h2 small { font-size: 1.2rem; font-weight: 400; color: var(--muted); margin-left: 0.8rem; }
  .list { position: relative; flex: 1; }
  .list .scroller { padding: 0.4rem 1.2rem 1.6rem; }
  .row {
    width: 100%; display: flex; align-items: center; gap: 1.4rem; padding: 0.8rem 1.2rem;
    border-radius: 1.2rem; text-align: left;
  }
  .row:focus { background: rgba(255, 255, 255, 0.92); color: #10121c; }
  .row:focus small, .row:focus .dur { color: rgba(16, 18, 28, 0.7); }
  .thumb {
    flex: 0 0 7.2rem; height: 4.05rem; border-radius: 0.6rem; overflow: hidden;
    background: linear-gradient(160deg, #2a3157, #151833);
  }
  .thumb img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .txt { flex: 1; min-width: 0; font-size: 1.35rem; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .txt small { display: block; font-size: 1.1rem; font-weight: 400; color: var(--muted); overflow: hidden; text-overflow: ellipsis; }
  .dur { font-size: 1.15rem; color: var(--muted); font-variant-numeric: tabular-nums; }
  .empty { padding: 1rem 1.2rem; color: var(--muted); font-size: 1.3rem; }

  .none { position: absolute; inset: 0; display: grid; place-content: center; justify-items: center; gap: 2rem; }
  .none p { font-size: 2.2rem; color: var(--muted); }
</style>
