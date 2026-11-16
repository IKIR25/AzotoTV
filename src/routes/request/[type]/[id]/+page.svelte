<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import {
    MS, dlProgress, inFlight, seerr, sTitle, sYear, statusLabel, tmdbImg,
    type DlProgress, type SDetails, type SeerrStatus,
  } from '$lib/seerr';
  import { api } from '$lib/jf';
  import DownloadBar from '$lib/DownloadBar.svelte';
  import { showToast, stage } from '$lib/stage.svelte';
  import { restoreFocus } from '$lib/nav';

  const type = $derived(page.params.type === 'tv' ? 'tv' : 'movie');
  const id = $derived(Number(page.params.id));
  let d = $state<SDetails | null>(null);
  let status = $state<SeerrStatus | null>(null);
  let failed = $state('');
  let sending = $state(false);
  /** seasons ticked for the request */
  let picked = $state<number[]>([]);
  /** how far the request is (null: nothing on its way, or no *arr config on this machine) */
  let prog = $state<DlProgress | null>(null);
  /** the Jellyfin item, found as soon as it lands (before Seerr notices) */
  let ready = $state<string | null>(null);

  const st = $derived(d?.mediaInfo?.status ?? MS.UNKNOWN);
  const jfId = $derived(d?.mediaInfo?.jellyfinMediaId ?? ready);
  const watching = $derived(!!d && inFlight(d.mediaInfo));
  const seasonStatus = (n: number) =>
    d?.mediaInfo?.seasons?.find((s) => s.seasonNumber === n)?.status ?? MS.UNKNOWN;
  // season 0 = specials: not offered
  const seasons = $derived((d?.seasons ?? []).filter((s) => s.seasonNumber > 0));
  const requestable = $derived(seasons.filter((s) => seasonStatus(s.seasonNumber) < MS.PENDING));
  const canAsk = $derived(
    !!status?.can_request &&
      (type === 'movie' ? st < MS.PENDING : requestable.length > 0),
  );
  const runtime = $derived(
    d?.runtime ? (d.runtime >= 60 ? `${Math.floor(d.runtime / 60)} h ${d.runtime % 60} min` : `${d.runtime} min`) : '',
  );

  async function load(t: 'movie' | 'tv', n: number) {
    try {
      // untrack: reading `status` here must not make the $effect below re-run
      const [det, s] = await Promise.all([seerr.details(t, n), untrack(() => status) ?? seerr.status()]);
      if (t !== type || n !== id) return;
      d = det;
      status = s;
      stage.backdrop = tmdbImg(det.backdropPath, 'w1280');
      picked = (det.seasons ?? [])
        .map((x) => x.seasonNumber)
        .filter((x) => x > 0 && seasonStatus(x) < MS.PENDING);
      await tick();
      restoreFocus(page.url.pathname, '.actions [data-focusable]');
    } catch (e) {
      failed = String(e);
    }
  }

  $effect(() => {
    d = null;
    failed = '';
    prog = null;
    ready = null;
    load(type, id);
  });

  // While it is on its way: progress every 3 s, Seerr's status every 30 s,
  // Jellyfin looked up when the file is being imported.
  $effect(() => {
    if (!watching) {
      prog = null;
      return;
    }
    const t = type;
    const n = id;
    let stop = false;
    let ticks = 0;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const p = await dlProgress(t, n);
        if (stop) return;
        prog = p && p.stage !== 'none' ? p : null;
        ticks++;
        if (!jfId && (p?.stage === 'import' || ticks % 10 === 0)) {
          const hit = await api.byTmdb(n, t === 'tv' ? 'Series' : 'Movie');
          if (hit && !stop) {
            ready = hit.Id;
            await tick();
            // the Richiedi button went away with the request: land on the new one
            if (!document.activeElement || document.activeElement === document.body)
              document.querySelector<HTMLElement>('[data-key="open"]')?.focus();
          }
        }
        if (ticks % 10 === 0) {
          const det = await seerr.details(t, n);
          if (!stop) d = det;
        }
      } catch {
        // a hiccup (server busy): try again next round
      }
      if (!stop) timer = setTimeout(poll, 3000);
    };
    timer = setTimeout(poll, 0);
    return () => {
      stop = true;
      clearTimeout(timer);
    };
  });

  function toggle(n: number) {
    picked = picked.includes(n) ? picked.filter((x) => x !== n) : [...picked, n].sort((a, b) => a - b);
  }

  async function ask() {
    if (!d || sending) return;
    if (type === 'tv' && !picked.length) return showToast('Scegli almeno una stagione');
    sending = true;
    try {
      await seerr.request(type, id, type === 'tv' ? picked : undefined);
      showToast('Richiesta inviata');
      d = await seerr.details(type, id);
    } catch (e) {
      showToast(`Richiesta non riuscita: ${e}`, 4000);
    }
    sending = false;
  }
</script>

<div class="scroller">
  {#if failed}
    <p class="note">Impossibile caricare: {failed}</p>
  {:else if d}
    <section class="info">
      <h1>{sTitle(d)}</h1>
      {#if d.tagline}<div class="sub">{d.tagline}</div>{/if}
      <div class="meta">
        {#if sYear(d)}<span>{sYear(d)}</span>{/if}
        <span>{type === 'tv' ? 'Serie' : 'Film'}</span>
        {#if runtime}<span>{runtime}</span>{/if}
        {#if type === 'tv' && seasons.length}<span>{seasons.length} stagioni</span>{/if}
        {#if d.voteAverage}<span>★ {d.voteAverage.toFixed(1)}</span>{/if}
        {#if d.genres?.length}<span>{d.genres.slice(0, 3).map((g) => g.name).join(' · ')}</span>{/if}
        {#if statusLabel(d.mediaInfo)}<span class="pill">{statusLabel(d.mediaInfo)}</span>{/if}
      </div>
      {#if d.overview}<p>{d.overview}</p>{/if}

      {#if prog && !(type === 'movie' && ready)}
        <DownloadBar p={prog} />
      {:else if st === MS.PENDING && !prog}
        <div class="hint wait">Richiesta inviata: aspetta che venga approvata.</div>
      {/if}

      {#if type === 'tv' && seasons.length}
        <div class="seasons">
          {#each seasons as s (s.seasonNumber)}
            {@const ss = seasonStatus(s.seasonNumber)}
            {#if ss >= MS.PENDING || !status?.can_request}
              <span class="chip done">Stagione {s.seasonNumber}{ss >= MS.PENDING ? ` · ${statusLabel({ status: ss })}` : ''}</span>
            {:else}
              <button
                class="chip"
                class:sel={picked.includes(s.seasonNumber)}
                data-focusable
                data-key={`season-${s.seasonNumber}`}
                onclick={() => toggle(s.seasonNumber)}
                >{picked.includes(s.seasonNumber) ? '✓' : '○'} Stagione {s.seasonNumber}</button
              >
            {/if}
          {/each}
        </div>
      {/if}

      <div class="actions">
        {#if jfId && (st >= MS.PARTIAL || ready)}
          <button class="btn primary" data-focusable data-key="open" onclick={() => goto(`/item/${jfId}`)}>
            {type === 'movie' && st < MS.AVAILABLE ? '▶ È arrivato: guardalo' : '▶ Apri nella libreria'}
          </button>
        {/if}
        {#if canAsk}
          <button class="btn primary" data-focusable data-key="ask" onclick={ask}>
            {sending
              ? 'Invio…'
              : type === 'tv'
                ? `＋ Richiedi ${picked.length === requestable.length ? 'tutte le stagioni' : `${picked.length} ${picked.length === 1 ? 'stagione' : 'stagioni'}`}`
                : '＋ Richiedi'}
          </button>
        {:else if status && !status.can_request && st < MS.PENDING}
          <span class="hint">
            {status.user ? 'Il tuo utente Seerr non può fare richieste.' : 'Il tuo utente non è collegato a Seerr.'}
          </span>
        {/if}
      </div>
    </section>
  {/if}
</div>

<style>
  .info { padding: 17rem 6rem 6rem; width: 80rem; }
  h1 { font-size: 5rem; font-weight: 700; letter-spacing: -0.02em; line-height: 1.05; }
  .sub { font-size: 1.8rem; color: var(--muted); margin-top: 0.6rem; font-style: italic; }
  .meta { margin-top: 1.4rem; }
  p {
    margin-top: 1.4rem; font-size: 1.55rem; line-height: 1.55; color: rgba(244, 245, 250, 0.85); max-width: 66rem;
    display: -webkit-box; -webkit-line-clamp: 5; line-clamp: 5; -webkit-box-orient: vertical; overflow: hidden;
  }
  .seasons { display: flex; flex-wrap: wrap; gap: 0.8rem; margin-top: 2rem; max-width: 74rem; }
  .chip {
    font-size: 1.3rem; padding: 0.7rem 1.4rem; border-radius: 1.4rem; background: rgba(255, 255, 255, 0.12);
  }
  .chip.sel { background: rgba(255, 255, 255, 0.3); }
  .chip:focus { background: #fff; color: #10121c; }
  .chip.done { color: var(--muted); border: 1px solid var(--glass-edge); background: none; }
  .actions { display: flex; align-items: center; gap: 1rem; margin-top: 2.2rem; }
  .hint { font-size: 1.4rem; color: var(--muted); }
  .hint.wait { margin-top: 2rem; }
  .note { margin: 17rem 6rem; font-size: 1.8rem; color: var(--muted); }
</style>
