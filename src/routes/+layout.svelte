<script lang="ts">
  import { locale, t } from '$lib/i18n';
  import '../app.css';
  import { onMount } from 'svelte';
  import { beforeNavigate, goto } from '$app/navigation';
  import { page } from '$app/state';
  import { invoke } from '@tauri-apps/api/core';
  import { auth, initAuth, server, signOut, userImage } from '$lib/session.svelte';
  import { api, onUnauthorized, type Item } from '$lib/jf';
  import { control, modal, shoulder, showToast, stage, toast } from '$lib/stage.svelte';
  import ControlMenu from '$lib/ControlMenu.svelte';
  import Notifications from '$lib/Notifications.svelte';
  import PhonePair from '$lib/PhonePair.svelte';
  import PhoneAsk from '$lib/PhoneAsk.svelte';
  import Viewer from '$lib/Viewer.svelte';
  import { ask, initPhone, sent } from '$lib/phone.svelte';
  import { initNotify, loadNotes, notes } from '$lib/notify.svelte';
  import { initTvLink } from '$lib/tvlink.svelte';
  import { startKeyboard, rememberFocus } from '$lib/nav';
  import { startPad } from '$lib/pad';
  import { dlog, initDebug, timeline } from '$lib/debug';
  import { emit, listen } from '@tauri-apps/api/event';
  import { initPlayer, play, player } from '$lib/player.svelte';
  import { initYoutube } from '$lib/youtube.svelte';
  import { pic, slideshow, wallpaper } from '$lib/gallery.svelte';
  import { initSongs, songCover } from '$lib/songs.svelte';
  import { initInstalled } from '$lib/apps/installed.svelte';
  import { artUrl, findGame, game, heroUrl, initGames, launch } from '$lib/games.svelte';
  import { startLift } from '$lib/tween';
  import Backdrop from '$lib/Backdrop.svelte';
  import Keyboard from '$lib/Keyboard.svelte';
  import { osk, oskPad } from '$lib/osk.svelte';
  import { initMusic, music, next, prev, seekBy, stopMusic, togglePause } from '$lib/music.svelte';
  import { initLive } from '$lib/live.svelte';
  import { poke, saver, startIdle } from '$lib/idle.svelte';
  import Screensaver from '$lib/Screensaver.svelte';
  import Splash from '$lib/Splash.svelte';
  import { uiSound } from '$lib/sounds.svelte';
  import { applyProfileTheme } from '$lib/themes/index.svelte';
  import { key } from '$lib/input.svelte';

  let { children } = $props();

  let views = $state<Item[]>([]);
  let clock = $state('');

  const path = $derived(page.url.pathname);
  // login + profile picker: no top bar, shown even while signed out
  const onLogin = $derived(path === '/login' || path.startsWith('/profiles'));
  // the AzotoTV home (app launcher) and the apps that aren't the TV one (Diretta
  // TV, Musica with its player and playlists, Giochi): no TV top bar, just the corner cluster
  const APP_ROOTS = ['/tv', '/live', '/musica', '/giochi', '/youtube', '/store', '/galleria', '/mii'];
  const bare = $derived(
    path === '/' || path === '/live' || path.startsWith('/musica') || path === '/music' || path.startsWith('/tracks/') ||
      path.startsWith('/giochi') || path.startsWith('/game/') || path.startsWith('/youtube') || path === '/store' || path.startsWith('/galleria') || path.startsWith('/mii') || path === '/pin' || path === '/theme' || path === '/tvpair' || path === '/controllers' || path.startsWith('/services'),
  );

  function back() {
    if (player.active) return; // mpv owns the screen; B is handled there
    if (modal.close) {
      uiSound('back');
      return modal.close();
    }
    if (path === '/' || (path === '/profiles' && (auth.state !== 'in' || auth.locked))) return;
    uiSound('back');
    if (APP_ROOTS.includes(path)) return goto('/'); // leave the app
    history.back();
  }

  /** START: the control menu (not over another overlay, nor before signing in). */
  function toggleControl() {
    if (control.open || control.notes || control.phones) return modal.close?.();
    if (modal.close || auth.state !== 'in' || onLogin) return;
    control.open = true;
  }

  onMount(() => {
    try {
      localStorage.removeItem('azoto.lite'); // the old "lite effects" toggle is gone
    } catch {}
    const tick = () =>
      (clock = new Date().toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' }));
    tick();
    const clockTimer = setInterval(tick, 10_000);

    const stops: (() => void)[] = [
      startKeyboard(back),
      startPad(
        (a) => {
          if (osk.target && oskPad(a)) return;
          if (path === '/music' && musicPad(a)) return;
          if (remoteKey(a)) return;
          if (a === 'b') back();
          if (a === 'start') toggleControl();
          if (a === 'lb' || a === 'rb') (shoulder.tab ?? switchTab)(a === 'lb' ? -1 : 1);
        },
        // an input that wakes the screensaver does nothing else
        () => poke() || player.active || game.active,
      ),
      startIdle(
        () => player.active || game.active || slideshow.on,
        () => auth.state === 'in' && !onLogin,
        () => !(music.active && !music.paused),
      ),
      // focus scale of buttons that have no `use:liftOnFocus` (see tween.ts)
      startLift([
        ['.btn', 1.07],
        ['.avatar', 1.18],
        ['.ep', 1.015],
        ['.person', 1.1],
        ['.ph', 1.08],
      ]),
    ];
    stops.push(initPlayer(), initYoutube(), initSongs(), initMusic(), initLive(), initGames(), initNotify(), initPhone(), initTvLink());
    initDebug().then(() => {
      const onFocus = (e: FocusEvent) => {
        const el = e.target as HTMLElement;
        const cap = el.querySelector?.('.cap')?.firstChild?.textContent ?? el.textContent?.trim().slice(0, 20);
        dlog(`focusin ${cap}`);
      };
      document.addEventListener('focusin', onFocus);
      const un = listen<string>('pad', (e) => {
        if (['up', 'down', 'left', 'right'].includes(e.payload)) timeline(e.payload);
      });
      stops.push(() => {
        document.removeEventListener('focusin', onFocus);
        void un.then((f) => f());
      });
    });

    // Token refused mid-use (e.g. password changed elsewhere): forget it, back to the picker.
    let expiring = false;
    onUnauthorized.fn = async () => {
      if (expiring || auth.state !== 'in') return;
      // only for the profile in use (a switch in progress checks its own token)
      const s = await invoke<{ user_id: string } | null>('jf_session').catch(() => null);
      if (!s || s.user_id !== auth.id) return;
      expiring = true;
      await signOut(auth.id);
      showToast(t('Sign-in expired: enter the password again'), 4000);
      await goto('/profiles', { replaceState: true });
      expiring = false;
    };
    stops.push(() => (onUnauthorized.fn = null));

    (async () => {
      await initAuth();
      initInstalled();
      // Start on "who's watching" (Netflix-style); dev runs can jump to a route.
      const route = await invoke<string | null>('dev_route').catch(() => null);
      if (route && auth.state === 'in') goto(route);
      else if (await server()) goto('/profiles', { replaceState: true });
      else goto('/login', { replaceState: true });
      // Dev hook: AZOTOTV_KEYS=Right,Down,pad:a,... replays arrow keys / other keys /
      // controller buttons (`pad:<action>`) / TV remote keys (`remote:<action>`),
      // logging the focused element after each.
      const keySeq = await invoke<string | null>('dev_keys').catch(() => null);
      if (keySeq) {
        const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
        await sleep(4000);
        const t0 = performance.now();
        document.addEventListener('focusin', (e) => {
          const el = e.target as HTMLElement;
          const cap = el.querySelector?.('.cap')?.firstChild?.textContent ?? el.textContent?.trim().slice(0, 20);
          void invoke('dev_log', { line: `  focusin +${Math.round(performance.now() - t0)}ms ${cap}` });
        });
        for (const k of keySeq.split(',')) {
          timeline(k);
          if (k.startsWith('pad:')) await emit('pad', k.slice(4));
          else if (k.startsWith('remote:')) await emit('remote', k.slice(7));
          else
            window.dispatchEvent(
              new KeyboardEvent('keydown', { key: ['Up', 'Down', 'Left', 'Right'].includes(k) ? `Arrow${k}` : k }),
            );
          await sleep(1800);
          const el = document.activeElement as HTMLElement | null;
          const row = el?.closest('.shelf')?.querySelector('h2')?.textContent ?? '(no row)';
          const cap = el?.querySelector('.cap')?.firstChild?.textContent ?? el?.textContent?.trim().slice(0, 30);
          const typed = osk.target ? ` osk="${osk.value}"@${osk.caret} ${osk.shift}` : '';
          await invoke('dev_log', { line: `after ${k}: ${location.pathname} [${row}] ${cap}${typed}` });
        }
        await invoke('system_exit');
      }
      // Dev hook: AZOTOTV_GAME=<id> launches a game from the library.
      const devGame = await invoke<string | null>('dev_game').catch(() => null);
      if (devGame) await launch(devGame);
      // Dev hook: AZOTOTV_AUTOPLAY=<itemId>:<seconds> plays, stops, then exits.
      const auto = await invoke<string | null>('dev_autoplay').catch(() => null);
      if (auto) {
        const [aid, secs, startS] = auto.split(':');
        await play(await api.item(aid), startS ? { startTicks: Number(startS) * 10_000_000 } : {});
        // secs === 'exit': leave the app while playing to test shutdown cleanup
        if (secs === 'exit') setTimeout(() => invoke('system_exit'), 8000);
        else setTimeout(() => invoke('player_stop'), Number(secs ?? 20) * 1000);
        const un = await (await import('@tauri-apps/api/event')).listen('player-ended', () =>
          setTimeout(() => invoke('system_exit'), 9000),
        );
        stops.push(un);
      }
    })();

    return () => {
      clearInterval(clockTimer);
      stops.forEach((s) => s());
    };
  });

  $effect(() => {
    void auth.id; // reload on profile switch
    if (auth.state === 'in') api.views().then((v) => (views = v.Items));
  });
  let avatarImg = $state(true);
  $effect(() => {
    void auth.id;
    avatarImg = true;
  });
  // the unread dot is per profile
  $effect(() => {
    void auth.id;
    if (auth.state === 'in') void loadNotes();
  });
  // ...or the profile's wallpaper (a photo picked in the Galleria), still
  // and dimmed so the page stays legible; a photo that isn't wide enough to
  // fill the screen shows whole over a blurred copy of itself
  // the signed-in profile's theme (START › Theme); Azoto on the profile screen
  applyProfileTheme();
  $effect(applyProfileTheme);
  const wall = $derived(auth.state === 'in' && !onLogin ? wallpaper() : null);
  const wallWhole = $derived(!!wall && (!wall.width || !wall.height || wall.width / wall.height < 1.2));
  let wallOk = $state(true);
  $effect(() => {
    void wall?.id;
    wallOk = true;
  });

  // Pages that don't set their own backdrop get the plain animated background.
  $effect(() => {
    void path;
    stage.backdrop = null;
    stage.ambient = false;
  });

  // Music belongs to the profile that started it (and its Jellyfin session).
  let musicOwner = '';
  $effect(() => {
    const id = auth.id;
    if (musicOwner && id !== musicOwner && music.active) stopMusic();
    musicOwner = id;
  });

  // On the player page: LB / RB previous / next song (held: not a burst of
  // skips), LT / RT seek.
  let lastSkip = 0;
  function musicPad(a: string): boolean {
    if (a === 'lt') seekBy(-15);
    else if (a === 'rt') seekBy(15);
    else if (a === 'lb' || a === 'rb') {
      if (performance.now() - lastSkip < 600) return true;
      lastSkip = performance.now();
      if (a === 'lb') prev();
      else next();
    } else return false;
    return true;
  }
  // The TV remote's own keys (see remote.rs): media keys drive the music
  // wherever it's playing, CH+ / CH- switch tabs, Home goes to the home screen.
  function remoteKey(a: string): boolean {
    if (a === 'home') {
      if (auth.state === 'in' && !onLogin && path !== '/') {
        modal.close?.();
        uiSound('back');
        goto('/');
      }
    } else if (a === 'chup' || a === 'chdown') (shoulder.tab ?? switchTab)(a === 'chup' ? 1 : -1);
    else if (!['play', 'stop', 'ff', 'rew', 'next', 'prev'].includes(a)) return false;
    else if (!music.active) return true;
    else if (a === 'play') togglePause();
    else if (a === 'stop') stopMusic();
    else if (a === 'ff' || a === 'rew') seekBy(a === 'ff' ? 15 : -15);
    else if (performance.now() - lastSkip >= 600) {
      lastSkip = performance.now();
      if (a === 'prev') prev();
      else next();
    }
    return true;
  }
  const song = $derived(music.active ? music.queue[music.index] : undefined);

  const href = (v: Item) => `/library/${v.Id}`;

  // The on-screen keyboard belongs to the page it was opened on.
  beforeNavigate(() => {
    osk.target = null;
  });

  // LB / RB: previous / next top-bar tab. Only on the tab pages themselves (not
  // on a film page, the picker, ...), no wrap-around, and spaced out because
  // the shoulder buttons auto-repeat while held.
  const tabs = $derived(['/tv', ...views.map(href), '/search']);
  let lastTab = 0;
  function switchTab(d: number) {
    const i = tabs.indexOf(path);
    const next = tabs[i + d];
    if (i < 0 || !next || modal.close || auth.state !== 'in') return;
    if (performance.now() - lastTab < 350) return;
    lastTab = performance.now();
    goto(next);
  }
</script>

<div class="app" class:osk-open={!!osk.target}>
  <div class="bg" aria-hidden="true"></div>
  {#if wall && wallOk}
    <div class="wall" aria-hidden="true">
      {#if wallWhole}<img class="blur" src={pic(wall.id, 'preview')} alt="" />{/if}
      <img class:whole={wallWhole} src={pic(wall.id, 'preview')} alt="" onerror={() => (wallOk = false)} />
    </div>
  {/if}
  <Backdrop url={stage.backdrop} ambient={stage.ambient} />

  {#snippet corner()}
    <div class="right">
      {#if song && path !== '/music'}
        <button class="mini" data-focusable data-scroll-top aria-label={t('Now playing')} onclick={() => goto('/music')}>
          {#if songCover(song, 4)}<img src={songCover(song, 4)} alt="" />{/if}
          <span class="eq" class:paused={music.paused}><i></i><i></i><i></i></span>
          <span class="t">{song.Name}</span>
        </button>
      {/if}
      <span class="clock">{clock}</span>
      <button
        class="avatar"
        data-focusable
        data-scroll-top
        aria-label="Menu"
        onclick={toggleControl}
        >{auth.name[0]?.toUpperCase() ?? '?'}{#if avatarImg}<img
            src={userImage(auth.id, 'current', 96)}
            alt=""
            onerror={() => (avatarImg = false)}
          />{/if}{#if notes.unread}<i class="unread" aria-label={t('new notifications')}></i>{/if}</button
      >
    </div>
  {/snippet}

  {#if auth.state === 'in' && bare}
    <header class="corner" class:light={path.startsWith('/mii')}>{@render corner()}</header>
  {:else if auth.state === 'in' && !onLogin}
    <!-- the TV app's header, styled like the other apps' (Giochi): title, then
         its tabs with the LB / RB hints; one line so the pages keep their room -->
    <header class="topbar">
      <h1>TV</h1>
      <nav>
        <kbd>{key('LB')}</kbd>
        <button class="tab" class:on={path === '/tv'} aria-current={path === '/tv' ? 'page' : undefined} data-focusable data-scroll-top onclick={() => goto('/tv')}>
          Home
        </button>
        {#each views as v}
          <button
            class="tab"
            class:on={path === href(v)}
            aria-current={path === href(v) ? 'page' : undefined}
            data-focusable
            data-scroll-top
            onclick={() => goto(href(v))}>{v.Name}</button
          >
        {/each}
        <button
          class="tab"
          class:on={path === '/search'}
          aria-current={path === '/search' ? 'page' : undefined}
          data-focusable
          data-scroll-top
          onclick={() => goto('/search')}>{t('Search')}</button
        >
        <kbd>{key('RB')}</kbd>
      </nav>
      {@render corner()}
    </header>
  {/if}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <main class="view" data-remember onfocusin={(e) => rememberFocus(path, e.target)}>
    {#if auth.state === 'loading'}
      <Splash />
    {:else if auth.state === 'in' || onLogin}
      {@render children()}
    {/if}
  </main>

  {#if osk.target}<Keyboard />{/if}
  {#if control.open}<ControlMenu />{/if}
  {#if control.notes}<Notifications />{/if}
  {#if control.phones}<PhonePair />{/if}
  {#if ask.who}{#key ask.who.id}<PhoneAsk />{/key}{/if}
  {#if sent.assets.length}
    <!-- photos a phone just sent ("Mostrale subito sulla TV", see phone.svelte.ts) -->
    <Viewer
      assets={sent.assets}
      show={sent.assets.filter((a) => !a.video).length > 1}
      title={t('From the phone')}
      onclose={() => (sent.assets = [])}
    />
  {/if}

  {#if game.loading}
    <!-- the loading screen until the game's window covers it (games.rs hide_when_shown);
         static on purpose (no spinner): the first start of a Windows game prepares its Wine prefix -->
    {@const g = findGame(game.id)}
    {@const hero = g ? heroUrl(g) : null}
    {@const logo = g ? artUrl(g, 'logo') : null}
    <div class="loading">
      {#if hero}<img class="hero" src={hero} alt="" />{/if}
      <div class="what">
        {#if logo}<img class="logo" src={logo} alt={game.name} />{:else}<h1>{game.name || t('Game')}</h1>{/if}
        <p>{game.name && logo ? `${game.name} · ` : ''}{t('Starting…')}</p>
      </div>
    </div>
  {/if}
  {#if toast.msg}<div class="toast glass">{toast.msg}</div>{/if}
  {#if saver.dark}<div class="dark"></div>{:else if saver.on}<Screensaver />{/if}
</div>

<style>
  .app { position: fixed; inset: 0; overflow: hidden; }

  /* ---- backdrop ----
     Static on purpose: the old drifting blobs repainted the whole screen (and
     the glass blur over it) every frame, which kept the server's CPU and GPU
     busy and its fans loud on every page without a film backdrop. */
  .bg {
    position: absolute; inset: 0;
    background: var(--wall);
  }

  /* ---- wallpaper ---- */
  .wall { position: absolute; inset: 0; overflow: hidden; background: var(--bg); }
  .wall img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .wall img.whole { object-fit: contain; }
  .wall img.blur { filter: blur(4rem) brightness(0.6); transform: scale(1.2); }
  .wall::after {
    content: ''; position: absolute; inset: 0;
    background:
      linear-gradient(180deg, rgb(var(--bg-rgb) / 0.45) 0%, rgb(var(--bg-rgb) / 0.2) 40%, rgb(var(--bg-rgb) / 0.7) 100%),
      rgb(var(--bg-rgb) / 0.4);
  }

  /* ---- top bar ---- */
  /* same title and tabs as the Giochi app (giochi/+page.svelte), level with the clock corner */
  .topbar {
    position: absolute; z-index: 10; top: 3rem; left: 6rem; right: 6rem; height: 5.6rem;
    display: flex; align-items: center; gap: 2.6rem;
  }
  /* smaller than the other apps' titles and in the logo's TV blue: on Home a film's
     title sits right under it, the two mustn't look alike */
  .topbar h1 { font-size: 2.6rem; font-weight: 800; letter-spacing: 0.04em; color: var(--accent); }
  .topbar .right { margin-left: auto; }
  /* no glass behind it any more: keep it readable over a bright film backdrop */
  .topbar h1, .topbar .tab:not(:focus), .topbar kbd, .topbar .clock { text-shadow: 0 0.1rem 0.25rem rgb(var(--shadow-rgb) / 0.7), 0 0 1rem rgb(var(--shadow-rgb) / 0.6); }
  /* the Mii app is white */
  .corner.light { --muted: rgba(40, 46, 58, 0.6); }
  .corner { position: absolute; z-index: 10; top: 3rem; right: 6rem; height: 5.6rem; display: flex; align-items: center; }
  nav { display: flex; align-items: center; gap: 0.4rem; }
  .tab {
    font-size: 1.6rem; padding: 0.9rem 2.2rem; border-radius: 2rem; color: var(--muted);
    transition: background 0.15s, color 0.15s;
  }
  .tab.on { color: var(--text); background: rgb(var(--fg-rgb) / 0.14); }
  .tab:focus { background: rgb(var(--fg-rgb) / 0.92); color: var(--focus-text); }
  nav kbd {
    font: inherit; font-size: 1.1rem; font-weight: 600; color: var(--muted); padding: 0.2rem 0.6rem;
    border: 1px solid rgb(var(--fg-rgb) / 0.3); border-radius: 0.5rem; margin: 0 0.6rem;
  }
  /* focus scale of the avatar is frame-driven: startLift in tween.ts */
  .right { display: flex; align-items: center; gap: 1.4rem; }
  .clock { font-size: 1.5rem; color: var(--muted); font-variant-numeric: tabular-nums; }
  .avatar {
    width: 3.8rem; height: 3.8rem; border-radius: 50%; font-weight: 700; font-size: 1.6rem;
    background: var(--avatar-grad);
  }
  .avatar:focus { box-shadow: 0 0 0 0.3rem var(--ring); }
  /* not overflow: hidden, the unread dot sits on the edge */
  .avatar { position: relative; }
  .avatar img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; border-radius: 50%; }
  .unread {
    position: absolute; top: -0.1rem; right: -0.1rem; width: 1.1rem; height: 1.1rem; border-radius: 50%;
    background: var(--azoto); box-shadow: 0 0 0 0.2rem var(--bg);
  }

  /* ---- mini player ---- */
  .mini {
    display: flex; align-items: center; gap: 0.9rem; height: 3.8rem; max-width: 22rem;
    padding: 0 1.4rem 0 0.4rem; border-radius: 1.9rem; background: rgb(var(--fg-rgb) / 0.1);
    font-size: 1.25rem; transition: background 0.15s, color 0.15s;
  }
  .mini img { width: 3rem; height: 3rem; border-radius: 50%; object-fit: cover; }
  .mini .t { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .mini:focus { background: rgb(var(--fg-rgb) / 0.92); color: var(--focus-text); }
  .eq { display: flex; align-items: flex-end; gap: 0.2rem; height: 1.4rem; flex: none; }
  /* still bars, not an animation: one would redraw the screen every frame for as long as music plays */
  .eq i { width: 0.3rem; height: 100%; background: currentColor; border-radius: 0.1rem; }
  .eq i:nth-child(1) { height: 55%; }
  .eq i:nth-child(3) { height: 75%; }
  .eq.paused i { height: 30%; }

  .toast {
    position: absolute; z-index: 30; left: 50%; bottom: 4rem; transform: translateX(-50%);
    padding: 1.2rem 2.6rem; border-radius: 2rem; font-size: 1.5rem;
  }
  /* screens off (idle.svelte.ts): black in case a frame still reaches them */
  .dark { position: fixed; inset: 0; z-index: 60; background: #000; }
  .loading { position: absolute; z-index: 29; inset: 0; display: grid; place-items: center; background: var(--bg); }
  .loading .hero { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; opacity: 0.35; }
  .loading .what { position: relative; display: grid; justify-items: center; gap: 2.4rem; }
  .loading .logo { max-width: 52rem; max-height: 20rem; object-fit: contain; filter: drop-shadow(0 0.4rem 2rem rgb(var(--shadow-rgb) / 0.6)); }
  .loading h1 { margin: 0; font-size: 4.4rem; }
  .loading p { margin: 0; font-size: 1.8rem; color: rgb(var(--fg-rgb) / 0.8); }

  /* ---- HUD ---- */
</style>
