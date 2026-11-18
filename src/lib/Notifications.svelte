<script lang="ts">
  import { locale, t } from '$lib/i18n';
  import { key } from './input.svelte';
  import { onMount } from 'svelte';
  import { control, modal } from './stage.svelte';
  import { ago, clearNotes, markSeen, notes, openNote, opens, toggleReminder, type Notification } from './notify.svelte';

  // "Notifiche" (from the START menu): live TV reminders still to come, then
  // the notifications, newest first; the ones this profile hadn't seen yet
  // have a dot. A opens what a notification is about / removes a reminder.
  const close = () => (control.notes = false);

  // captured before marking them seen, so the dots stay while the list is open
  const unread = notes.unread;
  let now = Date.now();
  let armed = $state(false);
  let armTimer: ReturnType<typeof setTimeout>;

  function open(n: Notification) {
    if (!opens(n)) return;
    close();
    void openNote(n);
  }

  function clearAll() {
    if (!armed) {
      armed = true;
      clearTimeout(armTimer);
      armTimer = setTimeout(() => (armed = false), 3500);
      return;
    }
    armed = false;
    void clearNotes();
  }

  const at = (ms: number) => new Date(ms).toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' });
  const day = (ms: number) => {
    const d = new Date(ms);
    const today = new Date();
    return d.toDateString() === today.toDateString() ? t('Today') : d.toLocaleDateString(locale, { weekday: 'long', day: 'numeric', month: 'short' });
  };

  let list: HTMLElement;
  onMount(() => {
    const before = document.activeElement;
    modal.close = close;
    list.querySelector<HTMLElement>('[data-focusable]')?.focus({ preventScroll: true });
    void markSeen();
    return () => {
      clearTimeout(armTimer);
      if (modal.close === close) modal.close = null;
      if (before instanceof HTMLElement && before.isConnected) before.focus({ preventScroll: true });
    };
  });
</script>

<aside class="sheet glass" data-nav-trap>
  <h2>{t('Notifications')}</h2>
  <div class="list scroller" bind:this={list}>
    {#if notes.reminders.length}
      <h3>{t('Reminders')}</h3>
      {#each notes.reminders as r (r.program_id)}
        <button class="row" data-focusable onclick={() => toggleReminder(r)}>
          <span class="pic logo">{#if r.image}<img src={`jf://localhost${r.image}`} alt="" />{/if}</span>
          <span class="txt">
            <small class="k">{t('{day} at {at} · {channel}', { day: day(r.start), at: at(r.start), channel: r.channel })}</small>
            <b>{r.name}</b>
            <small>{r.episode ?? t('{a} remove the reminder', { a: key('A') })}</small>
          </span>
          <span class="bell" aria-hidden="true">
            <svg viewBox="0 0 24 24"><path d="M6 16V11a6 6 0 1 1 12 0v5l1.5 2h-15z" /><path d="M10 20.5a2 2 0 0 0 4 0" /></svg>
          </span>
        </button>
      {/each}
      <h3>{t('Recent')}</h3>
    {/if}

    {#each notes.items as n, i (n.id)}
      <button class="row" class:dead={!opens(n)} data-focusable onclick={() => open(n)}>
        <span class="pic" class:logo={n.logo}>{#if n.image}<img src={`jf://localhost${n.image}`} alt="" />{/if}</span>
        <span class="txt">
          <small class="k">{n.kicker}</small>
          <b>{n.title}</b>
          {#if n.body}<small>{n.body}</small>{/if}
          <small class="t">{ago(n.time, now)}</small>
        </span>
        {#if i < unread}<span class="dot" aria-label={t('new')}></span>{/if}
      </button>
    {:else}
      <p class="empty">{t('No notifications. Films and episodes that were just added and Live TV reminders show up here.')}</p>
    {/each}

    {#if notes.items.length}
      <button class="row clear" class:armed data-focusable onblur={() => (armed = false)} onclick={clearAll}>
        <span class="txt"><b>{armed ? t('Press again to clear') : t('Clear notifications')}</b></span>
      </button>
    {/if}
  </div>
  <div class="hint"><kbd data-btn="A">{key('A')}</kbd> {t('open ·')} <kbd data-btn="B">{key('B')}</kbd> {t('close')}</div>
</aside>

<style>
  .sheet {
    position: absolute; z-index: 23; top: 0; right: 0; bottom: 0; width: 54rem;
    border-radius: 2.4rem 0 0 2.4rem; background: rgb(var(--panel-rgb) / 0.96);
  }
  h2 { position: absolute; top: 4rem; left: 3.4rem; font-size: 2.4rem; font-weight: 700; }
  h3 {
    font-size: 1.15rem; font-weight: 600; color: var(--muted); text-transform: uppercase; letter-spacing: 0.12em;
    margin: 1rem 1.4rem 0.6rem;
  }
  .list { top: 9.5rem; bottom: 5rem; padding: 0.6rem 2rem 2rem; }
  .row {
    display: flex; align-items: center; gap: 1.4rem; width: 100%; text-align: left;
    padding: 1rem 1.4rem; border-radius: 1.4rem; margin-bottom: 0.4rem;
  }
  .pic {
    flex: none; width: 5.2rem; height: 7.8rem; border-radius: 0.8rem; overflow: hidden;
    background: rgb(var(--fg-rgb) / 0.08); display: grid; place-items: center;
  }
  .pic img { width: 100%; height: 100%; object-fit: cover; }
  .pic.logo { width: 7.8rem; height: 4.4rem; }
  .pic.logo img { width: auto; height: auto; max-width: 85%; max-height: 80%; object-fit: contain; }
  .txt { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 0.2rem; }
  .txt b { font-size: 1.55rem; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .txt small { font-size: 1.15rem; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .txt .k { color: var(--accent-soft); font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; font-size: 1rem; }
  .txt .t { font-size: 1.05rem; }
  .dot { flex: none; width: 1rem; height: 1rem; border-radius: 50%; background: var(--azoto); }
  .bell { flex: none; width: 2.2rem; height: 2.2rem; color: var(--accent-soft); }
  svg { width: 100%; height: 100%; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
  .clear { justify-content: center; margin-top: 1rem; }
  .clear b { font-size: 1.35rem; text-align: center; color: var(--muted); }
  /* plain state change, no transition (see tween.ts) */
  .row:focus { background: var(--focus-bg); color: var(--focus-text); }
  .row:focus small, .row.clear:focus b { color: rgb(var(--focus-text-rgb) / 0.65); }
  .row:focus .k, .row:focus .bell { color: var(--accent-on-focus); }
  .row.dead:focus { background: rgb(var(--fg-rgb) / 0.82); }
  .row.clear.armed:focus { background: var(--danger); }
  .row.clear.armed:focus b { color: var(--on-accent); }
  .empty { font-size: 1.5rem; line-height: 1.5; color: var(--muted); padding: 1rem 1.4rem; }
  .hint { position: absolute; bottom: 1.8rem; left: 3.4rem; font-size: 1.15rem; color: var(--muted); }
  kbd {
    font: inherit; font-size: 0.95rem; font-weight: 700; padding: 0.05rem 0.45rem;
    border: 1px solid var(--glass-edge); border-radius: 0.4rem; color: var(--text);
  }
</style>
