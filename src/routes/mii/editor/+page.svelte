<script lang="ts">
  import { t } from '$lib/i18n';
  import { onDestroy, tick, untrack } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { listen } from '@tauri-apps/api/event';
  import { auth } from '$lib/session.svelte';
  import { key } from '$lib/input.svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { modal, shoulder, showToast } from '$lib/stage.svelte';
  import { touchScreen } from '$lib/phone.svelte';
  import { openKeyboard } from '$lib/osk.svelte';
  import Picker from '$lib/Picker.svelte';
  import { liveMii, type LiveMii } from '$lib/mii/render';
  import { face } from '$lib/mii/pics.svelte';
  import { addVersion, byId, lib, loadMiis, newPerson, saveLook } from '$lib/mii/library.svelte';
  import { b64, decode, freshNxId, miijs, type Look } from '$lib/mii/miijs';
  import { PALETTES, TABS, getAt, setAt, type Field } from '$lib/mii/parts';

  // The Mii editor, like the consoles' Mii makers: the Mii live on the left
  // (the right stick turns it), the parts on the right in tabs (LB / RB).
  // ?mii=<id>: edit that Mii. ?da=<id>&persona=<id>: a new version of that
  // person, starting from a copy. Nothing: a brand-new Mii.
  // Touch works too: the iPhone app's "Schermo touch" shows this page on the
  // phone (phonescreen.rs) and its fingers come back as a touchscreen.
  const params = page.url.searchParams;
  const editing = $derived(lib.miis.find((m) => m.id === params.get('mii')));
  const from = $derived(lib.miis.find((m) => m.id === params.get('da')));
  const person = $derived(byId(params.get('persona') ?? '') ?? (editing && byId(editing.person)));

  /** a plain Mii to start from (and to draw the part pictures on): the
   * Switch's base Mii (nn::mii BaseMii, as in Eden's raw_data.cpp) */
  const TEMPLATE: Look = {
    general: { height: 64, weight: 64, gender: 0, favoriteColor: 0 },
    meta: { type: 'Default', name: 'Mii', miiId: '00000000000000000000000000000000' },
    face: { type: 0, feature: 0, color: 0, makeup: 0 },
    hair: { type: 33, flipped: false, color: 1 },
    eyebrows: { color: 1, type: 6, squash: 3, rotation: 6, size: 4, yPosition: 7, distanceApart: 2 },
    eyes: { color: 8, type: 2, yPosition: 12, rotation: 4, squash: 3, size: 4, distanceApart: 2 },
    nose: { type: 1, yPosition: 9, size: 4 },
    mouth: { color: 19, type: 23, squash: 3, yPosition: 13, size: 4 },
    beard: { color: 8, type: 0, mustache: { type: 0, yPosition: 10, size: 4 } },
    glasses: { color: 8, size: 4, yPosition: 10, type: 0 },
    mole: { on: false, xPosition: 2, yPosition: 20, size: 4 },
  } as Look;
  /** where the Switch's base woman differs from the man */
  const WOMAN: Record<string, [number, number]> = {
    'hair.type': [33, 12],
    'eyes.type': [2, 4],
    'eyes.rotation': [4, 3],
    'eyebrows.type': [6, 0],
  };
  /** the base Mii of this gender */
  function base(gender: number): Look {
    const l = structuredClone(TEMPLATE);
    l.general.gender = gender;
    for (const [path, v] of Object.entries(WOMAN)) setAt(l, path, v[gender ? 1 : 0]);
    return l;
  }

  let look = $state<Look | null>(null);
  let start = ''; // what was loaded, to know if anything changed
  const dirty = $derived(look ? JSON.stringify(look) !== start : false);

  $effect(() => {
    if (auth.id && lib.loaded !== auth.id) loadMiis(auth.id);
  });
  $effect(() => {
    if (look || (lib.loaded !== auth.id && (params.get('mii') || params.get('da')))) return;
    const src = editing ?? from;
    const l = src ? structuredClone($state.snapshot(src.look)) : base(0);
    start = JSON.stringify(l);
    look = l;
  });

  const encode = (l: Look) => new Uint8Array(miijs.encodeMii($state.snapshot(l), miijs.MiiFormats.NFSD));
  const nx = $derived(look ? b64(encode(look)) : '');

  // ---- tabs (LB / RB, the remote's CH- / CH+)
  let tab = $state(0);
  const current = $derived(TABS[tab]);
  async function goTab(i: number) {
    tab = (i + TABS.length) % TABS.length;
    await tick();
    const el =
      document.querySelector<HTMLElement>('.panel .sel[data-focusable]') ?? document.querySelector<HTMLElement>('.panel [data-focusable]');
    el?.focus({ preventScroll: true });
    document.querySelector('.panel')?.scrollTo({ top: 0 });
  }

  // ---- the Mii live
  let box = $state<HTMLElement>();
  let live: LiveMii | null = null;
  $effect(() => {
    const el = box;
    if (!el || !look) return;
    const first = untrack(() => nx);
    let gone = false;
    let mine: LiveMii | null = null;
    liveMii(first, el, { tallest: true })
      .then((l) => {
        if (gone) return l.stop();
        mine = live = l;
        if (nx !== first) l.update(nx);
      })
      .catch((e) => console.warn('[mii] editor live', e));
    return () => {
      gone = true;
      mine?.stop();
      if (live === mine) live = null;
    };
  });
  // every change redrawn (a short pause first: sliders send many)
  let redraw: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const n = nx;
    clearTimeout(redraw);
    redraw = setTimeout(() => live?.update(n), 120);
  });

  $effect(() => {
    const un = listen<number>('pad-turn', (e) => live?.turn(e.payload));
    shoulder.tab = (d) => goTab(tab + d);
    return () => {
      void un.then((f) => f());
      shoulder.tab = null;
    };
  });

  // ---- changing values
  function set(path: string, v: unknown) {
    if (!look) return;
    // a brand-new Mii starts over from the other base, like on the Switch:
    // only the parts still as they came
    if (path === 'general.gender' && !editing && !from && v !== look.general.gender)
      for (const [p, both] of Object.entries(WOMAN)) {
        if (getAt(look, p) === both[look.general.gender ? 1 : 0]) setAt(look, p, both[v ? 1 : 0]);
      }
    setAt(look, path, v);
  }
  const value = (path: string) => (look ? getAt(look, path) : undefined);

  /** Listens to nav.ts's `navmove` (cancel it to take the move). */
  function navmove(node: HTMLElement, handler: (e: Event) => void) {
    let h = handler;
    const on = (e: Event) => h(e);
    node.addEventListener('navmove', on);
    return {
      update: (next: (e: Event) => void) => (h = next),
      destroy: () => node.removeEventListener('navmove', on),
    };
  }

  /** A slider takes left / right while focused (nav.ts `navmove`). */
  function slide(f: Field & { kind: 'slider' }) {
    return (e: Event) => {
      const dir = (e as CustomEvent<string>).detail;
      if (dir !== 'left' && dir !== 'right') return;
      e.preventDefault();
      const v = Math.max(f.min, Math.min(f.max, (value(f.path) as number) + (dir === 'right' ? 1 : -1)));
      set(f.path, v);
    };
  }

  // ---- the pictures of each option: the Mii template with this tab's parts
  // and the option (so changes in other tabs don't redraw them all)
  const GROUPS: Record<string, string[]> = {
    face: ['face', 'hair'],
    hair: ['hair'],
    eyebrows: ['eyebrows'],
    eyes: ['eyes'],
    nose: ['nose'],
    mouth: ['mouth'],
    beard: ['beard'],
    glasses: ['glasses'],
  };
  function optionNx(path: string, v: number) {
    const cur = $state.snapshot(look) as any;
    const l = base(cur.general.gender) as any;
    l.face.color = cur.face.color;
    for (const g of GROUPS[current.key] ?? []) l[g] = structuredClone(cur[g]);
    setAt(l, path, v);
    return b64(new Uint8Array(miijs.encodeMii(l, miijs.MiiFormats.NFSD)));
  }

  // ---- what the Wii keeps: a value that doesn't come back the same from the
  // Wii's format is marked "Solo Switch"
  const wiiCache = new Map<string, boolean>();
  let notOnWii = $state<Record<string, boolean>>({});
  async function wiiKeeps(path: string, v: unknown): Promise<boolean> {
    const k = `${path}=${v}`;
    const hit = wiiCache.get(k);
    if (hit !== undefined) return hit;
    const l = structuredClone($state.snapshot(look)) as any;
    setAt(l, path, v);
    let ok = false;
    try {
      const rcd = new Uint8Array(miijs.encodeMii(l, miijs.MiiFormats.RCD)).slice(0, 74);
      const back = getAt((await decode(rcd)).toJSON(), path);
      // a setting the Wii doesn't have (proportions) is only lost if it isn't the default
      ok = back === v || (back === undefined && v === getAt(base(l.general.gender), path));
    } catch {}
    wiiCache.set(k, ok);
    return ok;
  }
  // the current tab's options, worked out once each
  $effect(() => {
    const t = current;
    if (!look) return;
    (async () => {
      const out: Record<string, boolean> = {};
      for (const f of t.fields) {
        if (f.kind === 'parts') for (const v of f.grid.flat()) out[`${f.path}=${v}`] = !(await wiiKeeps(f.path, v));
        if (f.kind === 'color' && f.palette !== 'favorite')
          for (let v = 0; v < PALETTES[f.palette].length; v++) out[`${f.path}=${v}`] = !(await wiiKeeps(f.path, v));
        if (f.kind === 'slider') out[f.path] = !(await wiiKeeps(f.path, f.min)) && !(await wiiKeeps(f.path, f.max));
      }
      notOnWii = { ...notOnWii, ...out };
    })();
  });

  /** what the Mii loses on the Wii right now */
  let wiiLoses = $state<string[]>([]);
  $effect(() => {
    if (!look) return;
    const snap = JSON.stringify($state.snapshot(look));
    void snap;
    (async () => {
      const lost: string[] = [];
      for (const t of TABS)
        for (const f of t.fields) {
          if (f.kind === 'toggle' || f.path === 'general.favoriteColor') continue;
          if (!(await wiiKeeps(f.path, value(f.path)))) lost.push(`${t.name}: ${f.label.toLowerCase()}`);
        }
      wiiLoses = lost;
    })();
  });

  // ---- touch (the phone's touch screen) and the mouse
  /** Drags on `node`: `move(dx, x)` with the pixels moved since the last call
   * and the finger's x. Touches don't scroll the page meanwhile. */
  function drag(node: HTMLElement, move: (dx: number, x: number, start: boolean) => void) {
    let h = move;
    let lastX: number | null = null;
    const at = (x: number, start: boolean) => {
      h(lastX === null ? 0 : x - lastX, x, start);
      lastX = x;
    };
    const tstart = (e: TouchEvent) => {
      e.preventDefault();
      at(e.touches[0].clientX, true);
    };
    const tmove = (e: TouchEvent) => {
      e.preventDefault();
      at(e.touches[0].clientX, false);
    };
    const end = () => (lastX = null);
    const mdown = (e: MouseEvent) => {
      if (e.button !== 0) return;
      at(e.clientX, true);
      const mm = (e: MouseEvent) => at(e.clientX, false);
      const mu = () => {
        end();
        removeEventListener('mousemove', mm);
        removeEventListener('mouseup', mu);
      };
      addEventListener('mousemove', mm);
      addEventListener('mouseup', mu);
    };
    node.addEventListener('touchstart', tstart, { passive: false });
    node.addEventListener('touchmove', tmove, { passive: false });
    node.addEventListener('touchend', end);
    node.addEventListener('touchcancel', end);
    node.addEventListener('mousedown', mdown);
    return {
      update: (next: typeof move) => (h = next),
      destroy() {
        node.removeEventListener('touchstart', tstart);
        node.removeEventListener('touchmove', tmove);
        node.removeEventListener('touchend', end);
        node.removeEventListener('touchcancel', end);
        node.removeEventListener('mousedown', mdown);
      },
    };
  }
  /** a finger across the Mii turns it (half a turn across its box) */
  const turnBy = (dx: number) => box && live?.rotate((dx / box.clientWidth) * 180);
  /** a finger on a slider puts it there */
  function slideTo(f: Field & { kind: 'slider' }) {
    return (_dx: number, x: number, start: boolean) => {
      const track = document.querySelector<HTMLElement>(`[data-key="ed-${f.path}"] .track`);
      if (!track) return;
      if (start) track.closest<HTMLElement>('[data-focusable]')?.focus({ preventScroll: true });
      const r = track.getBoundingClientRect();
      const t = Math.max(0, Math.min(1, (x - r.left) / r.width));
      set(f.path, Math.round(f.min + t * (f.max - f.min)));
    };
  }
  /** what's tapped takes the focus, so the pad goes on from there */
  function tapped(e: MouseEvent) {
    (e.target as Element).closest<HTMLElement>('[data-focusable]')?.focus({ preventScroll: true });
  }
  function leave() {
    if (dirty) askLeave();
    else history.back();
  }

  // the phone may be this page's touch screen while it's open
  touchScreen.name = t('Mii editor');
  onDestroy(() => {
    touchScreen.name = '';
    invoke('phone_screen_stop').catch(() => {});
  });

  // ---- name
  function setName(v: string) {
    const n = v.trim().slice(0, 10);
    if (n) set('meta.name', n);
  }

  // ---- saving and leaving
  let saving = $state(false);
  async function save() {
    if (!look || saving) return;
    saving = true;
    try {
      const bytes = encode(look);
      if (editing) {
        await saveLook(editing, await decode(bytes));
        showToast(t('Mii saved'));
      } else {
        freshNxId(bytes); // a Mii of its own, even when it started as a copy
        const m = await decode(bytes);
        if (person) await addVersion(person, m);
        else {
          const p = await newPerson(m);
          start = JSON.stringify(look);
          return void goto(`/mii/${p.id}`, { replaceState: true });
        }
        showToast(person ? t('New version of {name}', { name: look.meta.name }) : t('Mii created'));
      }
      start = JSON.stringify(look);
      history.back();
    } catch (e) {
      showToast(t('Not saved: {e}', { e }));
    } finally {
      saving = false;
    }
  }

  // B with changes: ask first
  type Sheet = { title: string; options: { key: number; main: string; sub?: string }[]; onpick: (k: number) => void };
  let sheet = $state<Sheet | null>(null);
  function askLeave() {
    sheet = {
      title: t('Save the changes?'),
      options: [
        { key: 1, main: t('Save') },
        { key: 2, main: t('Exit without saving') },
        { key: 3, main: t('Keep editing') },
      ],
      onpick: async (k) => {
        sheet = null;
        await tick();
        if (k === 1) save();
        if (k === 2) {
          start = JSON.stringify(look);
          history.back();
        }
      },
    };
  }
  // the layout's B closes `modal.close`: ours while there are changes (the
  // sheet takes it over while it's open)
  $effect(() => {
    if (sheet) return;
    modal.close = dirty ? askLeave : null;
  });
  onDestroy(() => {
    if (modal.close === askLeave) modal.close = null;
  });

  // land on the first option of the first tab
  let landed = false;
  $effect(() => {
    if (!look || landed) return;
    landed = true;
    goTab(0);
  });

  const title = $derived(editing ? t('Edit Mii') : person ? t('New version') : t('New Mii'));
</script>

{#if look}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="editor" onclickcapture={tapped}>
    <aside class="stage">
      <div class="live" bind:this={box} use:drag={turnBy}></div>
      {#if wiiLoses.length}
        <div class="wii" title={wiiLoses.join(', ')}>
          <strong>{t('On Wii it looks a little different')}</strong>
          <span>{wiiLoses.slice(0, 3).join(' · ')}{wiiLoses.length > 3 ? t(' and more ({length})', { length: wiiLoses.length }) : ''}</span>
        </div>
      {/if}
      <p class="hint">{t('Right stick (or a finger) to turn it')}</p>
    </aside>

    <section class="side">
      <header>
        <h1>{title}</h1>
        <nav class="tabs">
          <kbd>{key('LB')}</kbd>
          {#each TABS as t, i (t.key)}
            <button class="tab" class:on={i === tab} onclick={() => goTab(i)}>{t.name}</button>
          {/each}
          <kbd>{key('RB')}</kbd>
        </nav>
      </header>

      <div class="panel scroller">
        {#if current.key === 'profile'}
          <label class="name">
            {t('Name in games')}
            <input
              value={look.meta.name}
              maxlength="10"
              data-focusable
              data-key="ed-name"
              oninput={(e) => setName(e.currentTarget.value)}
              onkeydown={(e) => e.key === 'Enter' && openKeyboard(e.currentTarget)}
              onclick={(e) => openKeyboard(e.currentTarget)}
            />
          </label>
        {/if}
        {#each current.fields as f (f.path)}
          <section class="field">
            <h2>
              {f.label}
              {#if f.kind === 'slider' && notOnWii[f.path]}<em>{t('Switch only')}</em>{/if}
            </h2>
            {#if f.kind === 'parts'}
              <div class="parts" class:zoom={f.zoom} style="--cols: {f.grid[0].length}">
                {#each f.grid.flat() as v (v)}
                  {@const pic = face(optionNx(f.path, v), { size: 160 })}
                  <button
                    class="part"
                    class:sel={value(f.path) === v}
                    data-focusable
                    data-key="ed-{f.path}-{v}"
                    onclick={() => set(f.path, v)}
                  >
                    {#if pic}<img src={pic} alt="" />{:else}<span class="num">{v + 1}</span>{/if}
                    {#if notOnWii[`${f.path}=${v}`]}<i>Switch</i>{/if}
                  </button>
                {/each}
              </div>
            {:else if f.kind === 'color'}
              <div class="colors" style="--cols: {f.grid[0].length}">
                {#each f.grid.flat() as v (v)}
                  {@const c = PALETTES[f.palette][v]}
                  <button
                    class="swatch"
                    class:sel={value(f.path) === v}
                    class:switch={notOnWii[`${f.path}=${v}`]}
                    style="--c: {c}"
                    data-focusable
                    data-key="ed-{f.path}-{v}"
                    aria-label={t('Color {a}', { a: v + 1 })}
                    onclick={() => set(f.path, v)}
                  ></button>
                {/each}
              </div>
            {:else if f.kind === 'slider'}
              {@const v = value(f.path) as number}
              <button class="slider" data-focusable data-key="ed-{f.path}" use:navmove={slide(f)} use:drag={slideTo(f)}>
                <span class="end">{f.low ?? '−'}</span>
                <span class="track"><span class="fill" style="width: {((v - f.min) / (f.max - f.min)) * 100}%"></span></span>
                <span class="end">{f.high ?? '+'}</span>
                <span class="val">{v - f.min + 1}</span>
              </button>
            {:else if f.kind === 'toggle'}
              <div class="toggle">
                {#each f.options as o, i (o)}
                  {@const v = f.values ? f.values[i] : i}
                  <button class="opt" class:sel={value(f.path) === v} data-focusable data-key="ed-{f.path}-{i}" onclick={() => set(f.path, v)}>{o}</button>
                {/each}
              </div>
            {/if}
          </section>
        {/each}

        <div class="actions">
          <button class="btn primary" data-focusable data-key="ed-save" disabled={saving} onclick={save}>
            {saving ? t('Saving…') : editing ? t('Save') : t('Create')}
          </button>
          <button class="btn" data-focusable data-key="ed-leave" onclick={leave}>{t('Exit')}</button>
          <span class="tip">o {key('B')}{dirty ? t(' (I\'ll ask whether to save)') : ''}</span>
        </div>
      </div>
    </section>
  </div>

  {#if sheet}
    <Picker light title={sheet.title} options={sheet.options} value={null} onpick={sheet.onpick} onclose={() => (sheet = null)} />
  {/if}
{/if}

<style>
  .editor {
    --ink: #2b2f38;
    --soft: #7b818d;
    --line: #e3e6eb;
    --tint: #3aa6c9;
    position: absolute;
    inset: 0;
    background: linear-gradient(#ffffff, #f1f3f6);
    color: var(--ink);
  }
  .stage { position: absolute; left: 0; top: 0; bottom: 0; width: 40rem; }
  .live { position: absolute; inset: 2rem 0 5rem; touch-action: none; }
  .hint { position: absolute; bottom: 2rem; left: 0; right: 0; text-align: center; font-size: 1.1rem; color: var(--soft); }
  .wii {
    position: absolute; left: 3rem; right: 3rem; bottom: 4.6rem; padding: 0.9rem 1.2rem; border-radius: 1.2rem;
    background: #fff; border: 1px solid var(--line); display: grid; gap: 0.2rem; font-size: 1.05rem; color: var(--soft);
  }
  .wii strong { color: var(--ink); font-size: 1.15rem; }

  .side { position: absolute; left: 40rem; right: 0; top: 0; bottom: 0; display: flex; flex-direction: column; }
  header { padding: 3.6rem 4rem 1rem 1rem; }
  h1 { font-size: 2.8rem; font-weight: 700; letter-spacing: -0.02em; }
  .tabs { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem; margin-top: 1.2rem; }
  .tab { font-size: 1.2rem; font-weight: 600; padding: 0.45rem 1rem; border-radius: 2rem; color: var(--soft); background: transparent; }
  .tab.on { background: var(--tint); color: #fff; }
  kbd {
    font: inherit; font-size: 0.95rem; font-weight: 700; padding: 0.05rem 0.45rem;
    border: 1px solid var(--line); border-radius: 0.4rem; color: var(--ink);
  }

  .panel { position: relative; flex: 1; padding: 0.6rem 4rem 6rem 1rem; }
  .field { margin-bottom: 2rem; }
  h2 { font-size: 1.2rem; font-weight: 700; color: var(--soft); margin-bottom: 0.8rem; display: flex; gap: 0.8rem; align-items: center; }
  h2 em, .part i {
    font-style: normal; font-size: 0.85rem; font-weight: 700; padding: 0.1rem 0.5rem; border-radius: 1rem;
    background: #fdecdf; color: #c4601b;
  }

  .name { display: grid; gap: 0.45rem; font-size: 1.2rem; color: var(--soft); margin-bottom: 2rem; max-width: 40rem; }
  .name input {
    font: inherit; font-size: 1.6rem; color: var(--ink); padding: 0.95rem 1.3rem; border-radius: 1.2rem;
    background: #fff; border: 0.2rem solid var(--line); outline: none;
  }
  .name input:focus { border-color: var(--tint); box-shadow: 0 0 0 0.3rem rgba(58, 166, 201, 0.2); }

  .parts { display: grid; grid-template-columns: repeat(var(--cols), 8.4rem); gap: 0.8rem; }
  .part {
    position: relative; width: 8.4rem; height: 8.4rem; border-radius: 1.4rem; overflow: hidden;
    background: #fff; border: 0.2rem solid var(--line); display: grid; place-items: center;
  }
  .part img { width: 100%; height: 100%; object-fit: cover; }
  /* small parts: the face, closer */
  .zoom .part img { transform: scale(1.7) translateY(8%); }
  .part .num { font-size: 1.3rem; color: #c3c8d0; }
  .part i { position: absolute; left: 0.4rem; bottom: 0.4rem; }
  .part.sel { border-color: var(--tint); background: #eef8fb; }
  .part:focus { border-color: var(--tint); box-shadow: 0 0 0 0.35rem rgba(58, 166, 201, 0.35); }

  .colors { display: grid; grid-template-columns: repeat(var(--cols), 3.4rem); gap: 0.55rem; }
  .swatch { width: 3.4rem; height: 3.4rem; border-radius: 50%; background: var(--c); border: 0.2rem solid rgba(0, 0, 0, 0.08); position: relative; }
  /* not on the Wii: a small notch */
  .swatch.switch::after {
    content: ''; position: absolute; right: -0.15rem; top: -0.15rem; width: 0.9rem; height: 0.9rem;
    border-radius: 50%; background: #e8732c; border: 0.15rem solid #fff;
  }
  .swatch.sel { box-shadow: 0 0 0 0.25rem #fff, 0 0 0 0.45rem var(--ink); }
  .swatch:focus { box-shadow: 0 0 0 0.25rem #fff, 0 0 0 0.5rem var(--tint); }

  .slider {
    display: flex; align-items: center; gap: 1rem; width: 100%; max-width: 50rem; padding: 0.9rem 1.2rem;
    border-radius: 1.2rem; background: #fff; border: 0.2rem solid var(--line); touch-action: none;
  }
  .slider .end { font-size: 1.1rem; color: var(--soft); min-width: 5rem; }
  .slider .end:nth-child(3) { text-align: right; }
  .track { flex: 1; height: 0.8rem; border-radius: 1rem; background: #e9ecf0; overflow: hidden; }
  .fill { display: block; height: 100%; background: var(--tint); }
  .val { font-size: 1.3rem; font-weight: 700; min-width: 2.6rem; text-align: right; }
  .slider:focus { border-color: var(--tint); box-shadow: 0 0 0 0.3rem rgba(58, 166, 201, 0.2); }

  .toggle { display: flex; gap: 0.8rem; }
  .opt { font-size: 1.35rem; font-weight: 600; padding: 0.9rem 2rem; border-radius: 2rem; background: #eef1f4; color: var(--ink); border: 0.2rem solid transparent; }
  .opt.sel { border-color: var(--tint); background: #eef8fb; }
  .opt:focus { background: var(--tint); color: #fff; }

  .actions { display: flex; align-items: center; gap: 1.6rem; margin-top: 1rem; }
  .btn { font-size: 1.4rem; font-weight: 600; padding: 1rem 2.6rem; border-radius: 2rem; background: #eef1f4; color: var(--ink); }
  .btn.primary { background: var(--tint); color: #fff; }
  .btn:focus { box-shadow: 0 0 0 0.35rem rgba(58, 166, 201, 0.35); }
  .tip { font-size: 1.1rem; color: var(--soft); }
</style>
