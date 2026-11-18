<script lang="ts">
  import { t } from '$lib/i18n';
  import { onMount, tick } from 'svelte';
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { showToast, stage } from '$lib/stage.svelte';
  import { platform } from '$lib/games.svelte';
  import DiskPicker from '$lib/DiskPicker.svelte';
  import {
    cancelDownload, download, loadRemote, peers, peerTargets, remoteArt, size, speed, startDownload, watchDownloads,
  } from '$lib/peers.svelte';

  // A game on a linked AzotoTV (peer.rs): what it is, how big, and Download
  // into one of the game folders (or another folder). While it comes, its
  // progress; once here, Play opens it as a game of this library.
  const peerId = $derived(page.params.peer ?? '');
  const id = $derived(page.params.id ?? '');
  const r = $derived(peers.remote.find((x) => x.peer === peerId && x.id === id) ?? null);
  const d = $derived(r ? download(r) : null);
  const logo = $derived(r ? remoteArt(r, 'logo') : null);
  let targets = $state<string[]>([]);
  let into = $state('');
  let picking = $state(false);
  let gone = $state(false);

  onMount(() => {
    watchDownloads();
    void (async () => {
      if (!peers.remote.length) await loadRemote().catch(() => {});
      gone = !r;
      targets = await peerTargets().catch(() => []);
      into = targets[0] ?? '~/Games';
      await tick();
      document.querySelector<HTMLElement>('[data-key="download"]')?.focus({ preventScroll: true });
    })();
  });

  $effect(() => {
    stage.backdrop = r ? (remoteArt(r, 'hero') ?? remoteArt(r, 'header')) : null;
  });

  // the button changes (Download -> Stop -> Play): keep the focus on it
  $effect(() => {
    void d?.state;
    void r?.here;
    tick().then(() => {
      if (!document.activeElement || document.activeElement === document.body)
        document.querySelector<HTMLElement>('[data-key="download"]')?.focus({ preventScroll: true });
    });
  });

  async function start() {
    if (!r) return;
    try {
      await startDownload(r, into);
    } catch (e) {
      showToast(String(e), 4000);
    }
  }

  const pct = $derived(d && d.total ? Math.floor((d.done / d.total) * 100) : 0);
  const left = $derived(d && d.speed > 0 ? Math.round((d.total - d.done) / d.speed / 60) : null);
  const choices = $derived([...new Set([...targets, into].filter(Boolean))]);
</script>

<div class="shade" aria-hidden="true"></div>
<div class="scroller">
  {#if r}
    <section class="info">
      {#if logo}
        <img class="logo" src={logo} alt={r.name} />
      {:else}
        <h1>{r.name}</h1>
      {/if}
      <div class="meta">
        {#if r.year}<span>{r.year}</span>{/if}
        {#if r.genres.length}<span>{r.genres.slice(0, 3).join(' · ')}</span>{/if}
        <span class="pill">{platform(r)}</span>
        <span class="pill">{size(r.size)}</span>
        <span class="pill">{t('On {name}', { name: r.peer_name })}</span>
      </div>
      {#if r.description}<p>{r.description}</p>{/if}

      {#if r.missing && r.brings}
        <p class="note">{t('This computer doesn\'t have {what} yet: it comes along with the game', { what: r.missing })}{#if r.runner_size}{' '}({size(r.runner_size)}){/if}</p>
      {:else if r.missing}
        <p class="warn">{t('To play it here, this computer also needs {what}: install it with the system\'s packages.', { what: r.missing })}</p>
      {/if}

      {#if r.here || d?.state === 'done'}
        <div class="actions">
          <button class="btn primary" data-focusable data-key="download" onclick={() => goto(`/game/${r.here ?? d?.id}`)}>{t('▶ Play|game')}</button>
        </div>
        {#if d?.error}<p class="warn">{d.error}</p>{/if}
        <div class="hint">{t('Downloaded: it is a game of this library now')}</div>
      {:else if d?.state === 'running'}
        <div class="progress">
          <div class="bar"><span style="width:{pct}%"></span></div>
          <div class="nums">
            <span>{d.step ?? `${pct}% · ${size(d.done)} / ${size(d.total)}`}</span>
            {#if d.speed}<span>{speed(d.speed)}{#if left !== null} · {left < 1 ? t('less than a minute') : t('about {m} min', { m: left })}{/if}</span>{/if}
          </div>
        </div>
        <div class="actions">
          <button class="btn" data-focusable data-key="download" onclick={() => r && cancelDownload(r)}>{t('Stop')}</button>
        </div>
        <div class="hint">{t('Stopped or interrupted, it picks up where it was next time')}</div>
      {:else}
        <div class="where">
          <span class="k">{t('Into')}</span>
          {#each choices as c (c)}
            <button class="btn chip" class:sel={c === into} data-focusable onclick={() => (into = c)}>{c}</button>
          {/each}
          <button class="btn chip" data-focusable onclick={() => (picking = true)}>{t('Another folder…')}</button>
        </div>
        <div class="actions">
          <button class="btn primary" data-focusable data-key="download" onclick={start}>
            {d?.state === 'error' || d?.state === 'cancelled' ? t('Download again') : t('Download · {size}', { size: size(r.size) })}
          </button>
        </div>
        {#if d?.state === 'error'}<p class="err">{d.error}</p>{/if}
        <div class="hint">{t('It comes from {name} over the home network; {name} has to stay on meanwhile', { name: r.peer_name })}</div>
      {/if}
    </section>
  {:else if gone}
    <section class="info"><h1>{t('Not found')}</h1><p>{t('The other AzotoTV doesn\'t answer, or it no longer has this game.')}</p></section>
  {/if}
</div>

{#if picking}
  <DiskPicker
    mode="dir"
    title={t('Where the game goes')}
    start={into}
    onpick={(p) => ((into = p), (picking = false))}
    onclose={() => (picking = false)}
  />
{/if}

<style>
  .shade {
    position: fixed; inset: 0; pointer-events: none;
    background: linear-gradient(90deg, rgb(var(--bg-rgb) / 0.72) 0%, rgb(var(--bg-rgb) / 0.5) 45%, transparent 75%);
  }
  .info { position: relative; padding: 17rem 6rem 3rem; width: 78rem; min-height: 56rem; }
  h1 { font-size: 5rem; font-weight: 700; letter-spacing: -0.02em; line-height: 1.05; }
  .logo { max-width: 46rem; max-height: 14rem; object-fit: contain; object-position: left bottom; display: block; }
  .meta { margin-top: 1.4rem; }
  p {
    margin-top: 1.4rem; font-size: 1.55rem; line-height: 1.55; color: rgb(var(--text-rgb) / 0.85); max-width: 66rem;
    display: -webkit-box; -webkit-line-clamp: 4; line-clamp: 4; -webkit-box-orient: vertical; overflow: hidden;
  }
  .warn { color: var(--danger-text); font-size: 1.4rem; }
  .note { color: var(--muted); font-size: 1.4rem; }
  .err { color: var(--danger-text); font-size: 1.4rem; }
  .where { display: flex; flex-wrap: wrap; align-items: center; gap: 0.8rem; margin-top: 2rem; }
  .where .k { font-size: 1.3rem; color: var(--muted); margin-right: 0.4rem; }
  .chip { font-size: 1.3rem; padding: 0.7rem 1.5rem; border-radius: 1.4rem; background: rgb(var(--fg-rgb) / 0.1); }
  .chip.sel { background: rgb(var(--fg-rgb) / 0.34); }
  .chip.sel:focus { background: var(--focus-bg); }
  .actions { display: flex; gap: 1rem; margin-top: 2.2rem; }
  .hint { margin-top: 2.4rem; font-size: 1.15rem; color: var(--muted); }
  .progress { margin-top: 2.2rem; max-width: 60rem; }
  .bar { height: 0.8rem; border-radius: 0.4rem; background: rgb(var(--fg-rgb) / 0.16); overflow: hidden; }
  .bar span { display: block; height: 100%; background: var(--accent); transition: width 0.6s; }
  .nums { display: flex; justify-content: space-between; margin-top: 0.8rem; font-size: 1.3rem; color: var(--muted); }
</style>
