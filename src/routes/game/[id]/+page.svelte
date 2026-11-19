<script lang="ts">
  import { locale, t } from '$lib/i18n';
  import { tick } from 'svelte';
  import { page } from '$app/state';
  import { auth } from '$lib/session.svelte';
  import { refresh, showToast, stage } from '$lib/stage.svelte';
  import { input, key } from '$lib/input.svelte';
  import { restoreFocus } from '$lib/nav';
  import Picker from '$lib/Picker.svelte';
  import PinButton from '$lib/PinButton.svelte';
  import { gamePin } from '$lib/pins.svelte';
  import { admin, checkAdmin } from '$lib/admin.svelte';
  import { goto } from '$app/navigation';
  import { cancelDownload, download, size, speed, startUpdate, updateInfo, watchDownloads, type Progress, type Update } from '$lib/peers.svelte';
  import {
    achievementIcon, artUrl, chapterArt, findGame, getAchievements, getSettings, heroUrl, lastPlayed, launch, library, loadGames,
    modArt, platform, playTrailer, playtime, saveSettings, savesInfo, setMod, setSavesMode, worldMode,
    type Achievement, type Mod, type SavesInfo,
  } from '$lib/games.svelte';

  // A game's page: hero art behind, logo, what it is, Gioca / Trailer, the
  // cursor sensitivity for games played with a stick-driven mouse pointer, and
  // this profile's Steam achievements.
  const id = $derived(page.params.id ?? '');
  const g = $derived(library.user === auth.id ? findGame(id) : null);
  const logo = $derived(g ? artUrl(g, 'logo') : null);
  let missing = $state(false);

  const SENS = [0.5, 0.75, 1, 1.25, 1.5, 2];
  let sens = $state(1);
  let sheet = $state<'trailer' | 'sens' | 'saves' | null>(null);
  // a game that is also on a linked AzotoTV: its saves go along, or stay here
  let saves = $state<SavesInfo | null>(null);
  const SAVE_MODES = [null, 'shared', 'local'] as const;
  // games with chapters: Gioca carries on with the one played last
  const current = $derived(g?.chapters.length ? (g.played?.chapter ?? 0) : null);
  // Minecraft: Gioca goes back into the world played last; new worlds are made
  // from its title screen
  const world = $derived(g?.worlds[0] ?? null);
  let achievements = $state<Achievement[]>([]);
  // a game downloaded from a linked AzotoTV that changed there since (an
  // update, a new mod): Update fetches only what changed
  let upd = $state<Update | null>(null);
  const ud = $derived(upd ? download({ peer: upd.peer, id: upd.game }) : null);
  let seenDone: Progress | null = null;

  $effect(() => {
    const cur = id;
    const user = auth.id;
    void refresh.n;
    missing = false;
    achievements = [];
    saves = null;
    savesInfo(cur).then((s) => cur === id && (saves = s), () => {});
    upd = null;
    watchDownloads();
    updateInfo(cur).then((u) => cur === id && (upd = u), () => {});
    loadGames(user).then(async () => {
      if (cur !== id) return;
      const found = findGame(cur);
      missing = !found;
      if (found?.achievements) {
        getAchievements(cur).then((a) => cur === id && (achievements = a), () => {});
      }
      await tick();
      restoreFocus(page.url.pathname, '[data-key="play"]');
    });
    getSettings().then((s) => (sens = s.cursor_sensitivity), () => {});
  });

  $effect(() => {
    stage.backdrop = g ? heroUrl(g) : null;
  });

  // an update that just finished: the game as it is now, and anything left
  $effect(() => {
    if (ud?.state !== 'done' || ud === seenDone) return;
    seenDone = ud;
    const cur = id;
    const focused = document.activeElement?.getAttribute('data-key');
    void loadGames(auth.id);
    updateInfo(cur).then(
      async (u) => {
        if (cur !== id) return;
        upd = u;
        await tick();
        // the button it was on went away
        if (!u && (focused === 'update' || document.activeElement === document.body))
          document.querySelector<HTMLElement>('[data-key="play"]')?.focus({ preventScroll: true });
      },
      () => {},
    );
  });

  async function update() {
    try {
      await startUpdate(id);
    } catch (e) {
      showToast(String(e), 4000);
    }
  }

  const updPct = $derived(ud && ud.total ? Math.floor((ud.done / ud.total) * 100) : 0);
  const updWhat = (u: Update) =>
    [
      u.files === 1 ? t('1 file to fetch') : u.files ? t('{n} files to fetch', { n: u.files }) : '',
      u.bytes ? size(u.bytes) : '',
      u.removed === 1 ? t('1 to remove') : u.removed ? t('{n} to remove', { n: u.removed }) : '',
    ].filter(Boolean).join(' · ');

  $effect(() => {
    void auth.id;
    checkAdmin();
  });

  const padLabel = $derived(
    !g ? '' : g.pad === 'xbox' ? 'Controller' : g.cursor ? t('Controller as a mouse') : t('Controller as a keyboard'),
  );

  async function closeSheet() {
    const was = sheet;
    sheet = null;
    await tick();
    document.querySelector<HTMLElement>(`[data-key="${was}"]`)?.focus({ preventScroll: true });
  }

  function trailer() {
    if (!g) return;
    if (g.trailers.length > 1) sheet = 'trailer';
    else playTrailer(g);
  }

  async function pickSens(key: number) {
    try {
      sens = (await saveSettings({ cursor_sensitivity: key / 100 })).cursor_sensitivity;
    } catch (e) {
      showToast(t('Setting not saved: {e}', { e }));
    }
    closeSheet();
  }

  async function pickSaves(i: number) {
    const mode = SAVE_MODES[i];
    try {
      await setSavesMode(id, mode);
      if (saves) saves.mode = mode;
    } catch (e) {
      showToast(t('Setting not saved: {e}', { e }));
    }
    closeSheet();
  }

  // by itself, a game without mods simply shares them
  const savesLabel = (s: SavesInfo) =>
    s.mode === 'local'
      ? t('Only on this one')
      : s.mode === 'shared' || !g?.mods.length
        ? t('Shared with {name}', { name: s.peer_name })
        : t('With {name}, while the mods are the same', { name: s.peer_name });

  const fmt = (v: number) => `${v.toLocaleString(locale)}×`;

  // mods: A switches one on or off for this profile; it applies at the next launch
  const modsOn = $derived(g?.mods.filter((m) => m.on).length ?? 0);
  let toggling = false;
  async function toggleMod(m: Mod) {
    if (!g || toggling) return;
    toggling = true;
    try {
      await setMod(g, m, !m.on);
      showToast(`${m.name} ${m.on ? 'disattivata' : 'attivata'}`);
    } catch (e) {
      showToast(t('Mod not changed: {e}', { e }));
    } finally {
      toggling = false;
    }
  }
</script>

<!-- Steam's hero art is brighter than film stills: extra shade behind the text -->
<div class="shade" aria-hidden="true"></div>
<div class="scroller">
  {#if g}
    <section class="info">
      {#if logo}
        <img class="logo" src={logo} alt={g.name} />
      {:else}
        <h1>{g.name}</h1>
      {/if}
      <div class="meta">
        {#if g.year}<span>{g.year}</span>{/if}
        {#if g.genres.length}<span>{g.genres.slice(0, 3).join(' · ')}</span>{/if}
        {#if g.players > 1}<span>{t('Up to {players} players · also with the phone', { players: g.players })}</span>{/if}
        {#if g.developers.length}<span>{g.developers[0]}</span>{/if}
        <span class="pill">{platform(g)}</span>
        <span class="pill">{padLabel}</span>
        {#if modsOn}<span class="pill">{modsOn === 1 ? t('1 mod on') : t('{n} mods on', { n: modsOn })}</span>{/if}
      </div>
      {#if g.description}<p>{g.description}</p>{/if}
      {#if g.played || g.achievements}
        <div class="played">
          {#if g.played}{t('Played {playtime} · last time {lastPlayed}', { playtime: playtime(g.played.seconds), lastPlayed: lastPlayed(g.played.last) })}{/if}
          {#if g.played && g.achievements}{' · '}{/if}
          {#if g.achievements}{t('Achievements {a} of {b}', { a: g.achievements[0], b: g.achievements[1] })}{/if}
        </div>
      {/if}
      <div class="actions">
        <button class="btn primary" data-focusable data-key="play" data-scroll-top onclick={() => launch(g!.id, current, world)}>
          {t('▶ Play|game')}{#if current !== null && g.chapters[current]}{' · '}{g.chapters[current].name}{:else if world}{' · '}{world.name}{/if}
        </button>
        {#if world}
          <button class="btn icon" data-focusable data-key="new" data-scroll-top onclick={() => launch(g!.id)}>{t('＋ New world')}</button>
        {/if}
        {#if g.trailers.length}
          <button class="btn icon" data-focusable data-key="trailer" data-scroll-top onclick={trailer}>
            {t('▷ Trailer')}
          </button>
        {/if}
        {#if upd && ud?.state === 'running'}
          <button class="btn icon" data-focusable data-key="update" data-scroll-top onclick={() => upd && cancelDownload({ peer: upd.peer, id: upd.game })}>
            {t('Stop the update')}
          </button>
        {:else if upd}
          <button class="btn icon" data-focusable data-key="update" data-scroll-top onclick={update}>{t('↓ Update')}</button>
        {/if}
        <PinButton pin={gamePin(g)} />
        {#if admin.is}
          <button class="btn icon" data-focusable data-key="edit" data-scroll-top onclick={() => goto(`/game/${g!.id}/edit`)}>{t('Edit')}</button>
        {/if}
      </div>
      {#if upd && ud?.state === 'running'}
        <div class="upd">
          <div class="upd-bar"><span style="width:{updPct}%"></span></div>
          <div class="nums">
            <span>{ud.step ?? `${t('Updating from {name}', { name: upd.peer_name })} · ${updPct}% · ${size(ud.done)} / ${size(ud.total)}`}</span>
            {#if ud.speed}<span>{speed(ud.speed)}</span>{/if}
          </div>
        </div>
      {:else if upd}
        <div class="note">{t('Changed on {name} since it came here', { name: upd.peer_name })}: {updWhat(upd)}</div>
      {/if}
      {#if upd && ud?.state === 'error'}<div class="err">{ud.error}</div>{/if}
      {#if g.cursor || saves}
        <div class="tracks">
          {#if g.cursor}
            <button class="btn chip" data-focusable data-key="sens" data-scroll-top onclick={() => (sheet = 'sens')}>
              <span class="k">{t('Cursor sensitivity')}</span>
              {fmt(sens)}
            </button>
          {/if}
          {#if saves}
            <button class="btn chip" data-focusable data-key="saves" data-scroll-top onclick={() => (sheet = 'saves')}>
              <span class="k">{t('Saves')}</span>
              {savesLabel(saves)}
            </button>
          {/if}
        </div>
      {/if}
      {#if g.type === 'minecraft'}
        <div class="hint">
          {t('To leave: Pause › Save and quit › Quit game. Holding {a} closes it at once, without saving the last few minutes.', { a: key('HOME') })}
        </div>
      {:else if g.type === 'steam'}
        <div class="hint">
          {t('Starts through Steam, which keeps its saves and runs the controllers: quit the game to go back to AzotoTV')}
        </div>
      {:else}
        <div class="hint">
          {input.desk
            ? t('While playing: close the game (or its window) to go back to AzotoTV; on a controller, hold {a}', { a: key('HOME') })
            : t('While playing: hold {a} to go back to AzotoTV', { a: key('HOME') })}
        </div>
      {/if}
    </section>
    {#if g.chapters.length}
      <h2>{t('Chapters')}</h2>
      <div class="eps">
        {#each g.chapters as c, i (i)}
          <button class="ep glass" data-focusable data-key="ch-{i}" onclick={() => launch(g!.id, i)}>
            <span class="thumb">
              {#if chapterArt(g, i)}<img src={chapterArt(g, i)} alt="" loading="lazy" decoding="async" />{/if}
              {#if i === current && g.played}<span class="tag">{t('Last played')}</span>{/if}
            </span>
            <span class="txt">
              <strong>{c.name}{#if c.subtitle}<span class="st">{' · '}{c.subtitle}</span>{/if}</strong>
              {#if c.description}<span class="ov">{c.description}</span>{/if}
            </span>
          </button>
        {/each}
      </div>
    {/if}
    {#if g.mods.length}
      <h2>{t('Mods')} <span class="count">{modsOn ? t('{modsOn} of {length} on', { modsOn, length: g.mods.length }) : t('none on')}</span></h2>
      <div class="eps">
        {#each g.mods as m (m.id)}
          <button class="ep glass mod" class:on={m.on} data-focusable data-key="mod-{m.id}" onclick={() => toggleMod(m)}>
            <span class="thumb">
              {#if modArt(g, m)}<img src={modArt(g, m)} alt="" loading="lazy" decoding="async" />{:else}<span class="ph">{m.name}</span>{/if}
              {#if m.on}<span class="tag">{t('On')}</span>{/if}
            </span>
            <span class="txt">
              <strong>{m.name}{#if m.subtitle}<span class="st">{' · '}{m.subtitle}</span>{/if}</strong>
              {#if m.description}<span class="ov">{m.description}</span>{/if}
            </span>
            <span class="sw" aria-label={m.on ? 'attiva' : 'disattivata'}><span></span></span>
          </button>
        {/each}
      </div>
    {/if}
    {#if g.worlds.length}
      <h2>{t('Worlds')}</h2>
      <div class="eps">
        {#each g.worlds as w, i (w.id)}
          <button class="ep glass" data-focusable data-key="w-{w.id}" onclick={() => launch(g!.id, null, w)}>
            <span class="thumb">
              {#if w.icon}<img class="fill" src={w.icon} alt="" /><img class="pixelated" src={w.icon} alt="" />{/if}
              {#if i === 0}<span class="tag">{t('Last played')}</span>{/if}
            </span>
            <span class="txt">
              <strong>{w.name}</strong>
              <span class="ov">{t('{worldMode} · played {lastPlayed}', { worldMode: worldMode(w), lastPlayed: lastPlayed(w.last) })}</span>
            </span>
          </button>
        {/each}
      </div>
    {/if}
    {#if achievements.length}
      {@const got = achievements.filter((a) => a.earned).length}
      <h2>{t('Achievements')} <span class="count">{t('{got} of {length}', { got, length: achievements.length })}</span></h2>
      <div class="bar" aria-hidden="true"><span style="width: {(got / achievements.length) * 100}%"></span></div>
      <div class="achs">
        {#each achievements as a (a.icon)}
          {@const secret = a.hidden && !a.earned}
          <button class="ach" class:locked={!a.earned} data-focusable data-key="ach-{a.icon}">
            {#if secret}
              <span class="icon secret">?</span>
            {:else}
              <img class="icon" src={achievementIcon(g, a)} alt="" loading="lazy" decoding="async" />
            {/if}
            <span class="txt">
              <strong>{secret ? t('Secret achievement') : a.name}</strong>
              <span class="ov">{secret ? t('Keep playing to find out') : a.description}</span>
              {#if a.earned}<span class="when">{t('Unlocked {lastPlayed}', { lastPlayed: lastPlayed(a.earned) })}</span>{/if}
            </span>
          </button>
        {/each}
      </div>
    {/if}
  {:else if missing}
    <p class="msg">{t('Game not found.')}</p>
  {/if}
</div>

{#if g && sheet === 'trailer'}
  <Picker
    title="Trailer"
    options={g.trailers.map((t, i) => ({ key: i, main: t }))}
    value={null}
    onpick={(i) => {
      sheet = null;
      playTrailer(g!, i);
    }}
    onclose={closeSheet}
  />
{:else if sheet === 'saves' && saves}
  <Picker
    title={t('Saves')}
    options={[
      { key: 0, main: t('By themselves'), sub: t('Shared with {name}, but each stays on its own machine while the mods on are different', { name: saves.peer_name }) },
      { key: 1, main: t('Shared with {name}', { name: saves.peer_name }), sub: t('Always the same saves, before and after every game') },
      { key: 2, main: t('Only on this one'), sub: t('This machine keeps its own saves (e.g. a game modded only here)') },
    ]}
    value={SAVE_MODES.indexOf(saves.mode)}
    onpick={pickSaves}
    onclose={closeSheet}
  />
{:else if sheet === 'sens'}
  <Picker
    title={t('Cursor sensitivity')}
    options={SENS.map((v) => ({ key: Math.round(v * 100), main: fmt(v), sub: v === 1 ? 'normale' : v < 1 ? t('slower') : t('faster') }))}
    value={Math.round(sens * 100)}
    onpick={pickSens}
    onclose={closeSheet}
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
  .played { margin-top: 1rem; font-size: 1.3rem; color: var(--muted); }
  .actions { display: flex; gap: 1rem; margin-top: 2.2rem; }
  .tracks { display: flex; gap: 1rem; margin-top: 1.2rem; }
  .chip { font-size: 1.3rem; font-weight: 500; padding: 0.8rem 1.6rem; border-radius: 1.4rem; background: rgb(var(--fg-rgb) / 0.1); }
  .chip .k { font-weight: 700; margin-right: 0.3rem; }
  .hint { margin-top: 2.4rem; font-size: 1.15rem; color: var(--muted); }
  .note { margin-top: 1.2rem; font-size: 1.3rem; color: var(--muted); }
  .err { margin-top: 1.2rem; font-size: 1.3rem; color: var(--danger-text); }
  .upd { margin-top: 1.6rem; max-width: 60rem; }
  .upd-bar { height: 0.8rem; border-radius: 0.4rem; background: rgb(var(--fg-rgb) / 0.16); overflow: hidden; }
  .upd-bar span { display: block; height: 100%; background: var(--accent); transition: width 0.6s; }
  .nums { display: flex; justify-content: space-between; margin-top: 0.8rem; font-size: 1.3rem; color: var(--muted); }
  /* chapters: the same list as a series' episodes (item page) */
  h2 { font-size: 1.9rem; font-weight: 600; margin: 0 6rem 1rem; position: relative; }
  .eps { position: relative; display: grid; gap: 1.2rem; padding: 0 6rem 8rem; max-width: 120rem; }
  .ep { display: flex; gap: 2rem; padding: 1.2rem; border-radius: 1.6rem; text-align: left; transition: background 0.15s; }
  .ep:focus { background: rgb(var(--fg-rgb) / 0.22); box-shadow: 0 0 0 0.25rem var(--ring); }
  .thumb { position: relative; flex: 0 0 22rem; aspect-ratio: 16 / 9; border-radius: 1rem; overflow: hidden; background: var(--placeholder); }
  .thumb img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .thumb img.fill { filter: blur(1.4rem) brightness(0.75); transform: scale(1.3); }
  .thumb img.pixelated { image-rendering: pixelated; object-fit: contain; }
  .tag {
    position: absolute; left: 0.6rem; bottom: 0.6rem; padding: 0.2rem 0.7rem; border-radius: 0.6rem;
    font-size: 1rem; font-weight: 700; background: var(--accent); color: var(--on-accent);
  }
  .txt { display: grid; align-content: start; gap: 0.4rem; min-width: 0; }
  .txt strong { font-size: 1.7rem; }
  .st { font-weight: 500; color: rgb(var(--text-rgb) / 0.8); }
  .ov { font-size: 1.3rem; line-height: 1.45; color: rgb(var(--text-rgb) / 0.75); display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  /* mods: the episode row plus an on/off switch at the end */
  .mod { align-items: center; }
  .mod .txt { flex: 1; }
  .ph { position: absolute; inset: 0; display: grid; place-items: center; padding: 1rem; text-align: center; font-size: 1.5rem; font-weight: 700; color: rgb(var(--text-rgb) / 0.7); }
  .sw { flex: 0 0 auto; width: 5.6rem; height: 3rem; border-radius: 1.5rem; background: rgb(var(--fg-rgb) / 0.18); position: relative; transition: background 0.15s; margin-right: 1rem; }
  .sw span { position: absolute; top: 0.35rem; left: 0.35rem; width: 2.3rem; height: 2.3rem; border-radius: 50%; background: var(--fg); transition: transform 0.15s; }
  .mod.on .sw { background: var(--accent); }
  .mod.on .sw span { transform: translateX(2.6rem); }
  .count { font-weight: 500; color: var(--muted); margin-left: 0.6rem; }
  .bar { position: relative; margin: 0 6rem 1.6rem; max-width: 40rem; height: 0.5rem; border-radius: 0.25rem; background: rgb(var(--fg-rgb) / 0.14); overflow: hidden; }
  .bar span { display: block; height: 100%; background: var(--accent); border-radius: inherit; }
  .achs { position: relative; display: grid; grid-template-columns: repeat(3, 1fr); gap: 1.2rem; padding: 0 6rem 8rem; max-width: 150rem; }
  /* plain, not .glass: hundreds of backdrop blurs are too much for the TV's GPU */
  .ach {
    display: flex; align-items: center; gap: 1.4rem; padding: 1rem; border-radius: 1.4rem; text-align: left; transition: background 0.15s;
    background: rgb(var(--scrim-rgb) / 0.55); border: 1px solid rgb(var(--fg-rgb) / 0.08);
  }
  .ach:focus { background: rgb(var(--fg-rgb) / 0.22); box-shadow: 0 0 0 0.25rem var(--ring); }
  .ach .icon { flex: 0 0 6.4rem; width: 6.4rem; height: 6.4rem; border-radius: 0.8rem; object-fit: cover; }
  .ach.locked .icon { filter: grayscale(1) brightness(0.55); }
  .ach.locked strong { color: rgb(var(--text-rgb) / 0.7); }
  .secret { display: grid; place-items: center; font-size: 3rem; font-weight: 700; color: rgb(var(--text-rgb) / 0.5); background: rgb(var(--fg-rgb) / 0.08); }
  .ach strong { font-size: 1.45rem; }
  .ach .ov { font-size: 1.15rem; }
  .when { font-size: 1.05rem; font-weight: 600; color: var(--accent-soft); }
  .msg { position: absolute; top: 12rem; left: 6rem; font-size: 1.8rem; color: var(--muted); }
</style>
