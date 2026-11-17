<script lang="ts">
  import { lang, languages, setLanguage, t } from '$lib/i18n';
  import { key } from './input.svelte';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { invoke } from '@tauri-apps/api/core';
  import { auth, prefs, setTgAlerts, userImage } from './session.svelte';
  import { control, modal, showToast } from './stage.svelte';
  import { notes, onAirNow, openNote } from './notify.svelte';
  import { setSounds, sounds, uiSound } from './sounds.svelte';
  import { stopMusic } from './music.svelte';
  import { sleepNow } from './idle.svelte';
  import { tvLink } from './tvlink.svelte';

  // tvOS-style control menu (START): a TG that has just started (first, focused),
  // switch profile, notifications, pair a phone (and the TV, when it has forgotten AzotoTV), controllers, sounds, TG alerts and PIN of this profile,
  // leave to the desktop, reboot, standby. D-pad stays inside it; B or START closes it. Leaving /
  // rebooting need a second A within a few seconds. Nothing here turns the
  // machine off: it's the home server (Jellyfin, downloads…).
  const close = () => (control.open = false);

  let armed = $state('');
  let armTimer: ReturnType<typeof setTimeout>;
  let avatarImg = $state(true);

  function confirm(key: string, run: () => void) {
    if (armed !== key) {
      armed = key;
      clearTimeout(armTimer);
      armTimer = setTimeout(() => (armed = ''), 3500);
      return;
    }
    armed = '';
    run();
  }

  async function reboot() {
    try {
      await invoke('system_reboot');
    } catch (e) {
      showToast(t('Couldn\'t: {e}', { e }), 4000);
    }
  }

  /** Standby: music off, back to "who's watching" for whoever turns it on next, screens and TV off. */
  async function standby() {
    close();
    await stopMusic();
    await goto('/profiles');
    auth.locked = true; // whoever turns it on picks (and types their PIN) again
    sleepNow();
  }

  const actions = [
    { key: 'desktop', label: t('Exit to the desktop'), armed: t('Press again to exit'), run: () => invoke('system_exit') },
    { key: 'reboot', label: t('Restart'), armed: t('Press again to restart'), run: reboot },
    { key: 'poweroff', label: t('Turn off'), sub: t('The TV turns off, the server stays on'), run: standby },
  ];

  // pressing Language moves to the next one
  const nextLanguage = languages[(languages.findIndex((l) => l.id === lang) + 1) % languages.length];
  const tg = onAirNow();
  let tgBtn = $state<HTMLElement>();
  let first: HTMLElement;
  onMount(() => {
    const before = document.activeElement;
    modal.close = close;
    (tgBtn ?? first).focus({ preventScroll: true });
    return () => {
      clearTimeout(armTimer);
      if (modal.close === close) modal.close = null;
      // back to where the menu was opened from (unless we navigated away)
      if (before instanceof HTMLElement && before.isConnected) before.focus({ preventScroll: true });
    };
  });
</script>

<aside class="sheet glass" data-nav-trap>
  {#if tg}
    <button
      class="opt"
      data-focusable
      bind:this={tgBtn}
      onclick={() => {
        close();
        void openNote(tg);
      }}
    >
      <span class="ico" aria-hidden="true">
        <svg viewBox="0 0 24 24"><rect x="3" y="6" width="18" height="12.5" rx="2" /><path d="M8.5 2.5l3.5 3.5 3.5-3.5M8 21.5h8" /></svg>
      </span>
      <span class="txt">{t('Watch {title}', { title: tg.title })}<small>{tg.kicker}</small></span>
    </button>

    <div class="sep"></div>
  {/if}

  <button
    class="opt profile"
    data-focusable
    bind:this={first}
    onclick={() => {
      close();
      goto('/profiles');
    }}
  >
    <span class="avatar"
      >{auth.name[0]?.toUpperCase() ?? '?'}{#if avatarImg}<img
          src={userImage(auth.id, 'current', 96)}
          alt=""
          onerror={() => (avatarImg = false)}
        />{/if}</span
    >
    <span class="txt">{auth.name}<small>{t('Switch profile')}</small></span>
  </button>

  <button
    class="opt"
    data-focusable
    onclick={() => {
      close();
      control.notes = true;
    }}
  >
    <span class="ico" aria-hidden="true">
      <svg viewBox="0 0 24 24"><path d="M6 16V11a6 6 0 1 1 12 0v5l1.5 2h-15z" /><path d="M10 20.5a2 2 0 0 0 4 0" /></svg>
    </span>
    <span class="txt">{t('Notifications')}</span>
    {#if notes.unread}<span class="count">{notes.unread}</span>{/if}
  </button>

  <button
    class="opt"
    data-focusable
    onclick={() => {
      close();
      control.phones = true;
    }}
  >
    <span class="ico" aria-hidden="true">
      <svg viewBox="0 0 24 24"><rect x="7" y="2.5" width="10" height="19" rx="2.5" /><path d="M11 18.5h2" /></svg>
    </span>
    <span class="txt">{t('Connect a phone')}<small>{t('Remote, photos and videos from the phone')}</small></span>
  </button>

  <button
    class="opt"
    data-focusable
    onclick={() => {
      close();
      goto('/controllers');
    }}
  >
    <span class="ico" aria-hidden="true">
      <svg viewBox="0 0 24 24"
        ><path d="M7.5 8h9a5 5 0 0 1 4.8 6.4l-1.1 3.6a2 2 0 0 1-3.5.6L14.8 16H9.2l-1.9 2.6a2 2 0 0 1-3.5-.6l-1.1-3.6A5 5 0 0 1 7.5 8z" /><path
          d="M8 11v3M6.5 12.5h3M15.5 12h.01M17.5 13.5h.01"
        /></svg
      >
    </span>
    <span class="txt">Controller<small>{t('Who is connected, pair Joy-Cons and other controllers')}</small></span>
  </button>

  {#if tvLink.unpaired}
    <button
      class="opt"
      data-focusable
      onclick={() => {
        close();
        goto('/tvpair');
      }}
    >
      <span class="ico" aria-hidden="true">
        <svg viewBox="0 0 24 24"><rect x="3" y="4.5" width="18" height="12.5" rx="2" /><path d="M8 20.5h8M12 8.5v3.5M12 14.5v.01" /></svg>
      </span>
      <span class="txt">{t('Connect the TV')}<small>{t('The TV no longer obeys Turn off and turning on')}</small></span>
    </button>
  {/if}

  <button
    class="opt"
    data-focusable
    onclick={() => {
      setSounds(!sounds.on);
      uiSound('select'); // what they sound like, right away
    }}
  >
    <span class="ico" aria-hidden="true">
      <svg viewBox="0 0 24 24"
        ><path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4z" />{#if sounds.on}<path d="M15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11" />{:else}<path
            d="M16 9.5l5 5M21 9.5l-5 5"
          />{/if}</svg
      >
    </span>
    <span class="txt">{t('Interface sounds')}</span>
    <span class="state">{sounds.on ? t('Yes') : 'No'}</span>
  </button>

  {#if prefs.auto_reminders}
  <button
    class="opt"
    data-focusable
    onclick={() => setTgAlerts(!prefs.tg_alerts).catch((e) => showToast(t('Couldn\'t: {e}', { e }), 4000))}
  >
    <span class="ico" aria-hidden="true">
      <svg viewBox="0 0 24 24"><rect x="3" y="6" width="18" height="12.5" rx="2" /><path d="M8.5 2.5l3.5 3.5 3.5-3.5M8 21.5h8" /></svg>
    </span>
    <span class="txt">{t('Programme alerts')}</span>
    <span class="state">{prefs.tg_alerts ? t('Yes') : 'No'}</span>
  </button>
  {/if}

  <button class="opt" data-focusable onclick={() => setLanguage(nextLanguage.id).catch((e) => showToast(t('Couldn\'t: {e}', { e }), 4000))}>
    <span class="ico" aria-hidden="true">
      <svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" /><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18" /></svg>
    </span>
    <span class="txt">{t('Language')}</span>
    <span class="state">{languages.find((l) => l.id === lang)?.name ?? lang}</span>
  </button>

  <button
    class="opt"
    data-focusable
    onclick={() => {
      close();
      goto('/pin');
    }}
  >
    <span class="ico" aria-hidden="true">
      <svg viewBox="0 0 24 24"><rect x="5" y="10.5" width="14" height="10" rx="2" /><path d="M8.5 10.5V7.5a3.5 3.5 0 0 1 7 0v3" /></svg>
    </span>
    <span class="txt">{t('Profile PIN')}</span>
    <span class="state">{prefs.pin ? t('Yes') : 'No'}</span>
  </button>

  <div class="sep"></div>

  {#each actions as a (a.key)}
    <button class="opt" class:armed={armed === a.key} data-focusable onblur={() => armed === a.key && (armed = '')} onclick={() => (a.armed ? confirm(a.key, a.run) : a.run())}>
      <span class="ico" aria-hidden="true">
        {#if a.key === 'desktop'}
          <svg viewBox="0 0 24 24"><rect x="3" y="4" width="18" height="12" rx="2" /><path d="M8 20h8M12 16v4" /></svg>
        {:else if a.key === 'reboot'}
          <svg viewBox="0 0 24 24"><path d="M20 12a8 8 0 1 1-2.34-5.66" /><path d="M20 4v5h-5" /></svg>
        {:else}
          <svg viewBox="0 0 24 24"><path d="M12 3v9" /><path d="M6.3 6.8a8 8 0 1 0 11.4 0" /></svg>
        {/if}
      </span>
      <span class="txt">{armed === a.key ? a.armed : a.label}{#if a.sub}<small>{a.sub}</small>{/if}</span>
    </button>
  {/each}

  <div class="hint"><kbd>{key('A')}</kbd> {t('select ·')} <kbd>{key('B')}</kbd> {t('close')}</div>
</aside>

<style>
  .sheet {
    position: absolute; z-index: 22; top: 0; right: 0; bottom: 0; width: 40rem;
    border-radius: 2.4rem 0 0 2.4rem; background: rgba(16, 18, 32, 0.94);
    padding: 3rem 2rem;
  }
  .opt {
    display: flex; align-items: center; gap: 1.4rem; width: 100%; text-align: left;
    padding: 0.95rem 1.4rem; border-radius: 1.4rem; margin-bottom: 0.4rem;
  }
  .txt { font-size: 1.6rem; font-weight: 600; }
  .txt small { display: block; font-size: 1.15rem; font-weight: 400; color: var(--muted); margin-top: 0.15rem; }
  .avatar {
    position: relative; overflow: hidden; flex: none; display: grid; place-items: center;
    width: 5.2rem; height: 5.2rem; border-radius: 50%; font-weight: 700; font-size: 2rem;
    background: linear-gradient(135deg, #ff4fa3, #7c8cff); color: #fff;
  }
  .avatar img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .ico { width: 2.4rem; height: 2.4rem; flex: none; margin-left: 0.4rem; }
  svg { width: 100%; height: 100%; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
  .sep { height: 1px; background: var(--glass-edge); margin: 1.1rem 1.4rem; }
  /* plain state change, no transition (see tween.ts) */
  .opt:focus { background: #fff; color: #10121c; }
  .opt:focus small { color: rgba(16, 18, 28, 0.65); }
  .opt.armed:focus { background: #ff5d6c; color: #fff; }
  .count {
    margin-left: auto; min-width: 2.4rem; padding: 0.2rem 0.7rem; border-radius: 1.2rem; text-align: center;
    background: var(--azoto); color: #fff; font-size: 1.2rem; font-weight: 700;
  }
  .state { margin-left: auto; font-size: 1.3rem; font-weight: 600; color: var(--muted); }
  .opt:focus .state { color: rgba(16, 18, 28, 0.65); }
  .hint { position: absolute; bottom: 1.8rem; left: 3.4rem; font-size: 1.15rem; color: var(--muted); }
  kbd {
    font: inherit; font-size: 0.95rem; font-weight: 700; padding: 0.05rem 0.45rem;
    border: 1px solid var(--glass-edge); border-radius: 0.4rem; color: var(--text);
  }
</style>
