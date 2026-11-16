<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { findGame, launch, lobby } from '$lib/games.svelte';
  import { key } from '$lib/input.svelte';

  // START › Controller, and the screen before every game for more players:
  // players 1-4 in the order their controllers were connected (the order the
  // game gets them in, see controllers.rs). While it's open Bluetooth looks
  // for controllers in pairing mode and pairs them, a controller in use lights
  // its card up, and phones see "La TV cerca controller".

  interface Pad {
    node: string;
    name: string;
    kind: 'joycons' | 'joycon-l' | 'joycon-r' | 'procon' | 'phone' | 'xbox' | 'playstation' | 'pad';
    player: number | null;
    battery: number | null;
    charging: boolean;
    wireless: boolean;
    asleep: boolean;
  }
  interface Overview {
    pads: Pad[];
    waiting: number;
    bluetooth: { state: string; scanning: boolean; news: string[] };
  }

  let info = $state<Overview | null>(null);
  let lit = $state<Record<string, boolean>>({});
  const timers: Record<string, ReturnType<typeof setTimeout>> = {};

  const pending = lobby.pending;
  const g = pending ? findGame(pending.id) : null;
  const title = g
    ? [g.name, pending?.chapter != null ? g.chapters[pending.chapter]?.name : null, pending?.world?.name].filter(Boolean).join(' · ')
    : '';

  const slots = $derived(
    [1, 2, 3, 4].map((n) => info?.pads.find((p) => p.player === n) ?? null),
  );
  const extra = $derived(info?.pads.filter((p) => p.player === null) ?? []);
  const news = $derived(info?.bluetooth.news.at(-1) ?? '');

  let started = false;

  async function play() {
    if (!pending) return;
    started = true;
    lobby.pending = null;
    history.back();
    await new Promise((r) => setTimeout(r, 80));
    await launch(pending.id, pending.chapter, pending.world, true);
  }

  const load = async () => {
    try {
      info = await invoke<Overview>('controllers_list');
    } catch {
      /* keeps the last one */
    }
  };

  onMount(() => {
    invoke('controllers_watch', { on: true }).catch(() => {});
    void load();
    const poll = setInterval(load, 1000);
    const un = listen<string>('controller-input', (e) => {
      const n = e.payload;
      lit[n] = true;
      clearTimeout(timers[n]);
      timers[n] = setTimeout(() => (lit[n] = false), 600);
    });
    void tick().then(() => document.querySelector<HTMLElement>('.wrap [data-focusable]')?.focus({ preventScroll: true }));
    return () => {
      clearInterval(poll);
      void un.then((f) => f());
      Object.values(timers).forEach(clearTimeout);
      invoke('controllers_watch', { on: false }).catch(() => {});
      // left with B: the game doesn't start
      if (!started && lobby.pending === pending) lobby.pending = null;
    };
  });

  const KIND: Record<Pad['kind'], string> = {
    joycons: 'Joy-Con in coppia',
    'joycon-l': 'Joy-Con da solo',
    'joycon-r': 'Joy-Con da solo',
    procon: 'Nintendo',
    phone: 'Telefono',
    xbox: 'Xbox',
    playstation: 'PlayStation',
    pad: 'Controller',
  };
  const bars = (b: number) => Math.max(1, Math.round(b / 25));
</script>

{#snippet icon(kind: Pad['kind'])}
  <svg viewBox="0 0 24 24" aria-hidden="true">
    {#if kind === 'joycons'}
      <path d="M11 4H8.5A3.5 3.5 0 0 0 5 7.5v9A3.5 3.5 0 0 0 8.5 20H11z" /><path d="M13 4h2.5A3.5 3.5 0 0 1 19 7.5v9a3.5 3.5 0 0 1-3.5 3.5H13z" /><path
        d="M8 9v.01M16 14.5v.01"
      />
    {:else if kind === 'joycon-l' || kind === 'joycon-r'}
      <path d="M4 8.5A3.5 3.5 0 0 1 7.5 5h9A3.5 3.5 0 0 1 20 8.5V12H4z" /><path d="M4 12h16v3.5a3.5 3.5 0 0 1-3.5 3.5h-9A3.5 3.5 0 0 1 4 15.5z" /><path
        d="M8 9v.01M16 9v.01"
      />
    {:else if kind === 'phone'}
      <rect x="7" y="2.5" width="10" height="19" rx="2.5" /><path d="M11 18.5h2" />
    {:else}
      <path d="M7.5 8h9a5 5 0 0 1 4.8 6.4l-1.1 3.6a2 2 0 0 1-3.5.6L14.8 16H9.2l-1.9 2.6a2 2 0 0 1-3.5-.6l-1.1-3.6A5 5 0 0 1 7.5 8z" /><path
        d="M8 11v3M6.5 12.5h3M15.5 12h.01M17.5 13.5h.01"
      />
    {/if}
  </svg>
{/snippet}

<div class="wrap">
  <header>
    <h1>Controller</h1>
    {#if g}<p class="game">{title} · fino a {g.players} giocatori</p>{/if}
  </header>

  <div class="slots">
    {#each slots as p, i (i)}
      <div class="slot glass" class:empty={!p} class:lit={p && lit[p.node]} class:asleep={p?.asleep}>
        <span class="num">{i + 1}</span>
        {#if p}
          <span class="ico">{@render icon(p.kind)}</span>
          <span class="name">{p.name}</span>
          <span class="sub">
            {#if p.asleep}Spenti: premi un tasto{:else}{KIND[p.kind]}{/if}
          </span>
          {#if p.battery !== null && !p.asleep}
            <span class="batt" class:low={p.battery <= 25} title="{p.battery}%">
              {#each [1, 2, 3, 4] as b (b)}<i class:on={b <= bars(p.battery)}></i>{/each}
              {#if p.charging}<small>in carica</small>{/if}
            </span>
          {/if}
        {:else}
          <span class="ico plus"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 6v12M6 12h12" /></svg></span>
          <span class="name">Libero</span>
        {/if}
      </div>
    {/each}
  </div>

  {#if extra.length}
    <p class="extra">Collegati in più (giocano con il giocatore 4): {extra.map((p) => p.name).join(', ')}</p>
  {/if}

  <div class="help glass">
    {#if info && info.waiting > 0}
      <p class="joy">
        <b>{info.waiting === 1 ? 'Un Joy-Con aspetta' : `${info.waiting} Joy-Con aspettano`}:</b> premi <b>L</b> e <b>R</b> insieme (uno su ogni
        Joy-Con) per usarli come un solo controller, oppure <b>SL</b> e <b>SR</b> su un Joy-Con per usarlo da solo, in orizzontale.
      </p>
    {/if}
    {#if info?.bluetooth.scanning}
      <p>
        <span class="dot"></span>Per collegare un controller Bluetooth nuovo tieni premuto il suo tasto di associazione (sui Joy-Con
        quello piccolo sul lato, finché le luci scorrono).
      </p>
    {:else if info && (info.bluetooth.state === 'off' || info.bluetooth.state === 'none')}
      <p>Bluetooth non disponibile: si possono usare i controller col cavo e i telefoni.</p>
    {/if}
    <p>Dal telefono: app AzotoTV › Usa come controller.</p>
    {#if news}<p class="news">{news}</p>{/if}
  </div>

  <div class="actions">
    {#if pending}
      <button class="btn primary" data-focusable onclick={play}>Gioca</button>
      <button class="btn" data-focusable onclick={() => history.back()}>Annulla</button>
    {:else}
      <button class="btn primary" data-focusable onclick={() => history.back()}>Fatto</button>
    {/if}
    <span class="hint">Premi un tasto su un controller per vedere qual è · {key('B')} per tornare indietro</span>
  </div>
</div>

<style>
  .wrap { position: absolute; inset: 0; display: grid; align-content: center; justify-items: center; gap: 2.4rem; padding: 0 6rem; }
  header { text-align: center; display: grid; gap: 0.6rem; }
  h1 { font-size: 3.2rem; font-weight: 600; }
  .game { font-size: 1.6rem; color: var(--muted); }
  .slots { display: grid; grid-template-columns: repeat(4, 22rem); gap: 2rem; }
  .slot {
    position: relative; height: 26rem; border-radius: 2.2rem; padding: 2.4rem 1.6rem 2rem;
    display: grid; grid-template-rows: auto 1fr auto auto auto; justify-items: center; align-items: center; gap: 0.4rem;
    text-align: center; transition: transform 0.15s, box-shadow 0.15s, border-color 0.15s;
  }
  .slot.empty { background: transparent; border-style: dashed; color: var(--muted); }
  .slot.lit { transform: scale(1.05); border-color: var(--accent); box-shadow: 0 0 0 0.3rem var(--accent), 0 0 3rem rgba(124, 140, 255, 0.5); }
  .slot.asleep { opacity: 0.55; }
  .num {
    width: 3.4rem; height: 3.4rem; border-radius: 50%; display: grid; place-items: center;
    font-size: 1.7rem; font-weight: 700; background: var(--accent); color: #fff;
  }
  .empty .num { background: rgba(255, 255, 255, 0.1); color: var(--muted); }
  .ico svg { width: 7.5rem; height: 7.5rem; fill: none; stroke: currentColor; stroke-width: 1.4; stroke-linecap: round; stroke-linejoin: round; }
  .ico.plus svg { width: 4rem; height: 4rem; opacity: 0.6; }
  .name { font-size: 1.6rem; font-weight: 600; max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sub { font-size: 1.25rem; color: var(--muted); min-height: 1.6rem; }
  .batt { display: flex; align-items: center; gap: 0.3rem; height: 1.8rem; }
  .batt i { width: 1.4rem; height: 0.8rem; border-radius: 0.2rem; background: rgba(255, 255, 255, 0.15); }
  .batt i.on { background: #7be08f; }
  .batt.low i.on { background: #ff8a6a; }
  .batt small { margin-left: 0.5rem; font-size: 1.1rem; color: var(--muted); }
  .extra { font-size: 1.3rem; color: var(--muted); }
  .help { width: 94rem; border-radius: 1.8rem; padding: 1.6rem 2.4rem; display: grid; gap: 0.7rem; font-size: 1.35rem; line-height: 1.45; color: var(--muted); }
  .help b { color: var(--text, #f4f5fa); }
  .joy { color: #f4f5fa; background: rgba(255, 60, 60, 0.14); border-radius: 1rem; padding: 0.8rem 1.2rem; }
  .news { color: #9fe3ae; }
  .dot {
    display: inline-block; width: 0.8rem; height: 0.8rem; border-radius: 50%; background: #3fb6ff; margin-right: 0.8rem;
    vertical-align: 0.1rem; animation: blink 1.4s ease-in-out infinite;
  }
  @keyframes blink { 50% { opacity: 0.25; } }
  .actions { display: flex; align-items: center; gap: 1.2rem; }
  .hint { margin-left: 1.4rem; font-size: 1.25rem; color: var(--muted); }
</style>
