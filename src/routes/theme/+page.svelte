<script lang="ts">
  import { t } from '$lib/i18n';
  import { onMount, tick } from 'svelte';
  import { auth } from '$lib/session.svelte';
  import { uiSound } from '$lib/sounds.svelte';
  import { current, setTheme, themes, tokenStyle, type Theme } from '$lib/themes/index.svelte';

  // START › Theme: one card per theme, each drawn in its own colours (a tiny
  // home: wallpaper, a menu with the focus on it, two tiles). A puts it on at
  // once, for this profile only. No confirmation: picking another is as quick.
  // An automatic theme shows its day and night halves.
  onMount(async () => {
    await tick();
    const on = document.querySelector<HTMLElement>(`[data-key="theme-${current.pick}"]`) ?? document.querySelector<HTMLElement>('.cards [data-focusable]');
    on?.focus({ preventScroll: true });
  });

  function pick(id: string) {
    setTheme(id);
    uiSound('select');
  }
</script>

{#snippet mini(th: Theme, time: string)}
  <div class="mini" style={tokenStyle(th)} aria-hidden="true">
    <span class="clock">{time}</span><span class="me"></span>
    <span class="title"></span><span class="sub"></span>
    <span class="menu">
      <i class="focus"></i><i></i><i></i>
    </span>
    <span class="tiles"><i class="lit"></i><i></i></span>
  </div>
{/snippet}

<div class="wrap">
  <header>
    <h1>{t('Theme')}</h1>
    <p>{t('How AzotoTV looks for {name}. Other profiles keep theirs.', { name: auth.name })}</p>
  </header>

  <div class="cards">
    {#each themes as th (th.id)}
      <button class="card" class:on={current.pick === th.id} data-focusable data-key="theme-{th.id}" onclick={() => pick(th.id)}>
        <div class="look">
          {#if th.auto}
            {@render mini(themes.find((x) => x.id === th.auto?.night) ?? th, '21:30')}
            <div class="half">{@render mini(themes.find((x) => x.id === th.auto?.day) ?? th, '10:30')}</div>
          {:else}
            {@render mini(th, '21:30')}
          {/if}
        </div>
        <span class="name">{t(th.name)}</span>
        <span class="note">{current.pick === th.id ? t('In use') : t(th.note)}</span>
      </button>
    {/each}
  </div>
</div>

<style>
  .wrap { position: absolute; inset: 0; padding: 12rem 6rem 0; }
  header { margin-bottom: 3.2rem; }
  h1 { font-size: 3.2rem; font-weight: 600; }
  header p { font-size: 1.5rem; color: var(--muted); margin-top: 0.6rem; }

  .cards { display: flex; gap: 2.4rem; flex-wrap: wrap; }
  .card { width: 25.2rem; display: grid; gap: 0.4rem; text-align: left; border-radius: 1.6rem; }
  .card:focus .look { box-shadow: 0 0 0 0.4rem var(--ring), 0 1.6rem 3rem rgb(var(--shadow-rgb) / 0.55); }
  .name { font-size: 1.8rem; font-weight: 600; margin-top: 1rem; }
  .note { font-size: 1.3rem; color: var(--muted); }
  .card.on .note { color: var(--ok); }

  .look { position: relative; border-radius: 1.6rem; box-shadow: 0 0.8rem 1.6rem rgb(var(--shadow-rgb) / 0.4); }
  .half { position: absolute; inset: 0; clip-path: polygon(0 0, 100% 0, 0 100%); }

  /* the tiny home, in the card's own theme */
  .mini {
    position: relative; aspect-ratio: 16 / 9; border-radius: 1.6rem; overflow: hidden;
    background: var(--wall); color: var(--text); font-family: var(--font);
  }
  .mini i, .mini span { position: absolute; display: block; }
  .clock { top: 0.8rem; right: 3.2rem; font-size: 0.9rem; font-style: normal; color: var(--muted); }
  .me { top: 0.8rem; right: 1.2rem; width: 1.4rem; height: 1.4rem; border-radius: 50%; background: var(--avatar-grad); }
  .title { top: 3.2rem; left: 1.2rem; width: 9rem; height: 1rem; border-radius: 0.3rem; background: var(--text); }
  .sub { top: 4.8rem; left: 1.2rem; width: 6.4rem; height: 0.6rem; border-radius: 0.3rem; background: var(--muted); }
  .menu {
    top: 3.2rem; right: 1.2rem; width: 9rem; padding: 0.4rem; border-radius: 0.8rem;
    background: var(--glass-bg); border: 1px solid var(--glass-edge); display: grid !important; gap: 0.3rem;
  }
  .menu i { position: static !important; height: 1.2rem; border-radius: 0.4rem; background: rgb(var(--fg-rgb) / 0.1); }
  .menu i.focus { background: var(--focus-bg); }
  .tiles { left: 1.2rem; bottom: 1.2rem; display: flex !important; gap: 0.8rem; }
  .tiles i { position: static !important; width: 4.8rem; height: 3rem; border-radius: 0.6rem; background: var(--placeholder-grad); }
  .tiles i.lit { background: var(--accent); box-shadow: 0 0 0 0.2rem var(--ring); }
</style>
