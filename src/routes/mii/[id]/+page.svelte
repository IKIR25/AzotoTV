<script lang="ts">
  import { MONTHS, t } from '$lib/i18n';
  import { onDestroy, tick, untrack } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { key } from '$lib/input.svelte';
  import { shoulder } from '$lib/stage.svelte';
  import { liveMii, type LiveMii } from '$lib/mii/render';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { auth, profiles } from '$lib/session.svelte';
  import { showToast } from '$lib/stage.svelte';
  import Picker from '$lib/Picker.svelte';
  import {
    byId, displayName, findNew, freeTomodachi, kinOf, lib, linkTomodachi, loadMiis, mainOf, mergePeople,
    removeMii, removePerson, rename, saveInfo, sendCopy, setBirthday, setMain, setTie, splitMii, tieLabel,
    tieOf, TIES, tomoLook, unlink, versionsOf, type Birthday, type Entry, type GameLink, type PersonEntry,
  } from '$lib/mii/library.svelte';
  import { face } from '$lib/mii/pics.svelte';
  import { nxId, unb64 } from '$lib/mii/miijs';
  import { favorite } from '$lib/mii/colors';

  // One person: their Mii drawn whole, names, birthday, family, and their
  // versions (other Miis of them, on other consoles or games). Everything
  // saves as you go; the Mii name and the birthday also go into the consoles.
  const DAYS = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
  const ORIGIN = { switch: t('from the Switch'), wii: t('from the Wii'), azoto: t('made in AzotoTV'), miitopia: t('from Miitopia'), tomodachi: t('from Tomodachi Life') };

  const me = $derived(byId(page.params.id ?? ''));

  $effect(() => {
    if (auth.id && lib.loaded !== auth.id) loadMiis(auth.id);
  });
  // the games' Miis (Tomodachi Life), for "Aggiungi una versione"
  $effect(() => {
    if (lib.loaded === auth.id) findNew().catch(() => {});
  });

  /** the version on show: the one last focused in "Versioni", else the main one */
  let shownId = $state<string | null>(null);
  const shown = $derived(me ? (versionsOf(me).find((m) => m.id === shownId) ?? mainOf(me)) : undefined);
  /** a linked Tomodachi Life Mii on show instead (its look made Switch-like) */
  let shownLink = $state<string | null>(null);
  const linkNx = $derived(shownLink && me ? tomoLook(shownLink, mainOf(me)?.look.general.favoriteColor) : undefined);

  // ---- the Mii, live: the right stick turns it, LB / RB by 45° (remote: CH+ / CH-)
  let box = $state<HTMLElement>();
  let live: LiveMii | null = null;
  let liveOn = $state(false);
  /** redrawn only when the look changes (not the name) */
  const lookKey = $derived(linkNx ? `tomodachi:${linkNx}` : shown ? JSON.stringify({ ...shown.look, meta: null }) : '');

  $effect(() => {
    const el = box;
    if (!lookKey || !el) return;
    const nx = untrack(() => linkNx ?? shown!.nx);
    let gone = false;
    let mine: LiveMii | null = null;
    liveMii(nx, el)
      .then((l) => {
        if (gone) return l.stop();
        mine = live = l;
        liveOn = true;
      })
      .catch((e) => console.warn('[mii] live', e));
    return () => {
      gone = true;
      mine?.stop();
      if (live === mine) live = null;
      liveOn = false;
    };
  });

  $effect(() => {
    const un = listen<number>('pad-turn', (e) => live?.turn(e.payload));
    shoulder.tab = (d) => live?.spin(d * 45);
    return () => {
      void un.then((f) => f());
      live?.turn(0);
      shoulder.tab = null;
    };
  });

  // ---- names: typed with the on-screen keyboard (or the phone), saved after a pause
  const FIELDS = [
    { key: 'first', label: t('Name') },
    { key: 'short', label: t('Short name') },
    { key: 'surname', label: t('Surname') },
    { key: 'nickname', label: 'Nickname' },
  ] as const;
  type Field = (typeof FIELDS)[number]['key'];

  let pending: (() => Promise<void>) | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  function later(job: () => Promise<void>) {
    clearTimeout(timer);
    pending = job;
    timer = setTimeout(flush, 700);
  }
  function flush() {
    clearTimeout(timer);
    const job = pending;
    pending = null;
    job?.().catch((e) => showToast(t('Not saved: {e}', { e })));
  }
  onDestroy(flush);

  function setName(p: PersonEntry, k: Field, v: string) {
    p.names[k] = v.trim() || undefined;
    later(() => saveInfo(p));
  }
  function setMiiName(m: Entry, v: string) {
    const name = v.trim();
    if (name) later(() => rename(m, name));
  }

  // ---- side sheets (birthday, relations, versions, send a copy)
  type Sheet = {
    title: string;
    options: { key: number; main: string; sub?: string }[];
    value: number | null;
    onpick: (k: number) => void;
  };
  let sheet = $state<Sheet | null>(null);
  let opener: HTMLElement | null = null;

  async function openSheet(s: Sheet) {
    if (!sheet) opener = document.activeElement as HTMLElement | null;
    sheet = null;
    await tick(); // one sheet at a time owns B (modal.close)
    sheet = s;
  }
  /** Closes the sheet; focus goes to `key` if given (a chip just added), else back where it was. */
  async function closeSheet(key?: string, fallback = 'add-kin') {
    sheet = null;
    await tick();
    const el =
      (key && document.querySelector<HTMLElement>(`[data-key="${CSS.escape(key)}"]`)) ||
      (opener?.isConnected ? opener : document.querySelector<HTMLElement>(`[data-key="${fallback}"]`));
    el?.focus({ preventScroll: true });
  }

  const YEAR = new Date().getFullYear();
  function pickBirthday(p: PersonEntry) {
    const was = p.birthday;
    openSheet({
      title: t('Birthday: the month'),
      options: [{ key: 0, main: t('No birthday') }, ...MONTHS.map((n, i) => ({ key: i + 1, main: n[0].toUpperCase() + n.slice(1) }))],
      value: was?.month ?? 0,
      onpick: (month) => {
        if (!month) return void setBirthday(p, null).then(() => closeSheet());
        openSheet({
          title: t('Birthday: the day of {a}', { a: MONTHS[month - 1] }),
          options: Array.from({ length: DAYS[month - 1] }, (_, i) => ({ key: i + 1, main: `${i + 1} ${MONTHS[month - 1]}` })),
          value: was?.month === month ? was.day : null,
          onpick: (day) =>
            openSheet({
              title: t('Year of birth'),
              options: [
                { key: 0, main: t('I don\'t know'), sub: t('Only the birthday') },
                ...Array.from({ length: YEAR - 1899 }, (_, i) => ({ key: YEAR - i, main: String(YEAR - i) })),
              ],
              value: was?.year ?? 0,
              onpick: async (year) => {
                await setBirthday(p, { month, day, year: year || undefined });
                closeSheet();
              },
            }),
        });
      },
    });
  }

  function pickTie(p: PersonEntry, other: PersonEntry) {
    const now = tieOf(p, other);
    openSheet({
      title: t('{name} is…', { name: displayName(other) }),
      options: [
        ...TIES.map((tie, i) => ({ key: i, main: tieLabel(other, tie), sub: t('of {displayName}', { displayName: displayName(p) }) })),
        { key: -1, main: t('No relationship') },
      ],
      value: now ? TIES.indexOf(now) : -1,
      onpick: async (k) => {
        await setTie(p, other, k < 0 ? null : TIES[k]);
        closeSheet(k < 0 ? undefined : `kin-${other.id}`);
      },
    });
  }

  function addKin(p: PersonEntry) {
    const others = lib.people.filter((o) => o.id !== p.id && !tieOf(p, o));
    if (!others.length) return showToast(t('You have already linked every Mii'));
    openSheet({
      title: t('Who are you adding?'),
      options: others.map((o, i) => ({ key: i, main: displayName(o), sub: o.names.nickname })),
      value: null,
      onpick: (i) => pickTie(p, others[i]),
    });
  }

  // ---- versions: other Miis of the same person
  const TOMO = 1000; // keys from here: Tomodachi Life Miis

  function addVersion(p: PersonEntry) {
    const others = lib.people.filter((o) => o.id !== p.id);
    const tomo = freeTomodachi();
    const base = shown;
    openSheet({
      title: t('Another Mii of {displayName}', { displayName: displayName(p) }),
      options: [
        { key: -1, main: t('Create a new version'), sub: t('Starts from a copy of this one, to change in the editor') },
        ...tomo.map((m, i) => ({ key: TOMO + i, main: m.name || m.miiName, sub: 'Tomodachi Life' })),
        ...others.map((o, i) => ({ key: i, main: displayName(o), sub: t('Another Mii of this library: it becomes a version of this one') })),
      ],
      value: null,
      onpick: async (k) => {
        if (k === -1) {
          sheet = null;
          return void goto(`/mii/editor?da=${base?.id}&persona=${p.id}`);
        }
        try {
          if (k >= TOMO) {
            const m = tomo[k - TOMO];
            const filled = await linkTomodachi(p, m);
            showToast(filled ? t('Linked: name and birthday taken from Tomodachi Life') : t('Linked to {name} of Tomodachi Life', { name: m.name || m.miiName }));
            closeSheet(`link-${m.id}`, 'add-version');
          } else {
            const o = others[k];
            const moved = versionsOf(o);
            await mergePeople(p, o);
            showToast(t('{displayName} is now a version of {displayName2}', { displayName: displayName(o), displayName2: displayName(p) }));
            closeSheet(moved[0] && `ver-${moved[0].id}`, 'add-version');
          }
        } catch (e) {
          showToast(t('Didn\'t work: {e}', { e }));
          closeSheet();
        }
      },
    });
  }

  function versionMenu(p: PersonEntry, m: Entry) {
    const all = versionsOf(p);
    const isMain = all[0]?.id === m.id;
    const opts = [
      !isMain && { key: 1, main: t('Use as the main one'), sub: t('The one you see first') },
      all.length > 1 && { key: 2, main: t('Separate'), sub: t('Becomes a Mii of its own, with no names or family') },
      all.length > 1 && { key: 3, main: t('Delete this version'), sub: t('It goes from the consoles too') },
    ].filter(Boolean) as Sheet['options'];
    if (!opts.length) return showToast(t('It\'s the only version: to remove it, delete the Mii'));
    openSheet({
      title: `${m.look.meta.name} · ${ORIGIN[m.from]}`,
      options: opts,
      value: null,
      onpick: async (k) => {
        if (k === 1) await setMain(p, m);
        if (k === 2) {
          const q = await splitMii(m);
          showToast(t('{name} is now a Mii of its own', { name: m.look.meta.name }));
          shownId = null;
          closeSheet(undefined, 'add-version');
          return void q;
        }
        if (k === 3) {
          await removeMii(m);
          shownId = null;
          return closeSheet(undefined, 'add-version');
        }
        closeSheet(`ver-${m.id}`);
      },
    });
  }

  function linkMenu(p: PersonEntry, l: GameLink) {
    openSheet({
      title: t('{name} · Tomodachi Life', { name: l.name }),
      options: [{ key: 1, main: t('Unlink'), sub: t('Names and birthday stay') }],
      value: null,
      onpick: async () => {
        if (shownLink === l.id) shownLink = null;
        await unlink(p, l);
        closeSheet(undefined, 'add-version');
      },
    });
  }

  async function pickProfile(p: PersonEntry) {
    const list = (await profiles().catch(() => ({ profiles: [] }))).profiles.filter((x) => x.id !== auth.id);
    if (!list.length) return showToast(t('There are no other profiles'));
    openSheet({
      title: t('Send a copy of {displayName} to…', { displayName: displayName(p) }),
      options: list.map((x, i) => ({ key: i, main: x.name, sub: t('Gets a Mii of their own, without the family ties') })),
      value: null,
      onpick: async (i) => {
        try {
          await sendCopy(p, list[i].id);
          showToast(t('{displayName} sent to {b}', { displayName: displayName(p), b: list[i].name }));
        } catch (e) {
          showToast(t('Couldn\'t send it: {e}', { e }));
        }
        closeSheet();
      },
    });
  }

  // ---- delete: a second press confirms
  let confirming = $state(false);
  async function del(p: PersonEntry) {
    if (!confirming) return void (confirming = true);
    pending = null;
    const name = displayName(p);
    await removePerson(p);
    showToast(`${name} eliminato`);
    goto('/mii', { replaceState: true });
  }

  // land on the first name field when the page opens
  let landed = false;
  $effect(() => {
    if (!me || !shown || landed) return; // (the page shows once both are loaded)
    landed = true;
    tick().then(() => document.querySelector<HTMLElement>('[data-key="name-first"]')?.focus({ preventScroll: true }));
  });

  function age(b: Birthday) {
    if (!b.year) return null;
    const now = new Date();
    const had = now.getMonth() + 1 > b.month || (now.getMonth() + 1 === b.month && now.getDate() >= b.day);
    return now.getFullYear() - b.year - (had ? 0 : 1);
  }
  function bday(p: PersonEntry) {
    const b = p.birthday;
    if (!b) return t('Not set');
    const a = age(b);
    return `${b.day} ${MONTHS[b.month - 1]}${b.year ? ` ${b.year}` : ''}${a !== null && a >= 0 ? ` · ${a === 1 ? '1 anno' : `${a} anni`}` : ''}`;
  }
</script>

{#if me && shown}
  {@const p = me}
  {@const m = shown}
  {@const versions = versionsOf(p)}
  {@const fav = favorite(m.look.general.favoriteColor)}
  {@const body = face(m.nx, { body: true, size: 768 })}
  <div class="page scroller" style="--fav: {fav.hex}">
    <aside class="stage">
      {#if body && !liveOn}<img src={body} alt="" />{/if}
      <div class="live" bind:this={box}></div>
      {#if liveOn}
        <p class="hint">{t('Right stick or')} <kbd>{key('LB')}</kbd> <kbd>{key('RB')}</kbd> {t('to turn it')}</p>
      {/if}
    </aside>

    <section class="info">
      <header>
        <h1>{displayName(p)}</h1>
        <p>
          <span class="swatch"></span>{fav.name} · {ORIGIN[m.from]}{versions.length > 1 ? t(' · {n} versions', { n: versions.length }) : ''}
        </p>
      </header>

      <h2>{t('Names')}</h2>
      <div class="fields">
        <label class="wide">
          {t('Name in games')}
          <input
            value={m.look.meta.name}
            maxlength="10"
            data-focusable
            data-key="name-mii"
            oninput={(e) => setMiiName(m, e.currentTarget.value)}
          />
          <small>
            {t('What Wii and Switch show, 10 letters at most')}{versions.length > 1 ? t(' (of this version: each has its own)') : ''}
          </small>
        </label>
        {#each FIELDS as f (f.key)}
          <label>
            {f.label}
            <input
              value={p.names[f.key] ?? ''}
              data-focusable
              data-key="name-{f.key}"
              oninput={(e) => setName(p, f.key, e.currentTarget.value)}
            />
          </label>
        {/each}
      </div>

      <h2>{t('Birthday')}</h2>
      <button class="row" data-focusable data-key="birthday" onclick={() => pickBirthday(p)}>
        <span>{bday(p)}</span>
        <small>{t('Change')}</small>
      </button>

      <h2>{t('Family and friends')}</h2>
      <div class="kin">
        {#each kinOf(p) as k (k.person.id)}
          {@const km = mainOf(k.person)}
          {@const pic = km ? face(km.nx) : null}
          <button
            class="chip"
            class:guess={k.inferred}
            data-focusable
            data-key="kin-{k.person.id}"
            style="--c: {favorite(km?.look.general.favoriteColor ?? 0).hex}"
            onclick={() => pickTie(p, k.person)}
          >
            <span class="pic">{#if pic}<img src={pic} alt="" />{/if}</span>
            <span class="txt">
              <strong>{displayName(k.person)}</strong>
              <small>{k.label}{k.inferred ? t(' · as a result') : ''}</small>
            </span>
          </button>
        {/each}
        <button class="chip add" data-focusable data-key="add-kin" onclick={() => addKin(p)}>
          <span class="pic">+</span>
          <span class="txt"><strong>{t('Add')}</strong><small>{t('a relative or a friend')}</small></span>
        </button>
      </div>

      <h2>{t('Versions')}</h2>
      <p class="note">{t('Other Miis of the same person: for the consoles they stay separate Miis.')}</p>
      <div class="kin">
        {#each versions as v, i (v.id)}
          {@const pic = face(v.nx)}
          <button
            class="chip"
            class:on={v.id === m.id && !shownLink}
            data-focusable
            data-key="ver-{v.id}"
            style="--c: {favorite(v.look.general.favoriteColor).hex}"
            onfocus={() => ((shownId = v.id), (shownLink = null))}
            onclick={() => versionMenu(p, v)}
          >
            <span class="pic">{#if pic}<img src={pic} alt="" />{/if}</span>
            <span class="txt">
              <strong>{v.look.meta.name}</strong>
              <small>{ORIGIN[v.from]}{i === 0 && versions.length > 1 ? t(' · main') : ''}</small>
            </span>
          </button>
        {/each}
        <!-- a link whose Mii was added from the game is that version already -->
        {#each p.links.filter((l) => !versions.some((v) => v.from === 'tomodachi' && nxId(unb64(v.nx)) === l.id)) as l (l.id)}
          {@const lnx = tomoLook(l.id, mainOf(p)?.look.general.favoriteColor)}
          {@const pic = lnx ? face(lnx) : null}
          <button
            class="chip game"
            class:on={shownLink === l.id}
            data-focusable
            data-key="link-{l.id}"
            onfocus={() => (shownLink = lnx ? l.id : null)}
            onclick={() => linkMenu(p, l)}
          >
            <span class="pic" class:drawn={pic}>{#if pic}<img src={pic} alt="" />{:else}TL{/if}</span>
            <span class="txt"><strong>{l.name}</strong><small>Tomodachi Life{lnx ? t(' · with the closest Switch parts') : ''}</small></span>
          </button>
        {/each}
        <button class="chip add" data-focusable data-key="add-version" onclick={() => addVersion(p)}>
          <span class="pic">+</span>
          <span class="txt"><strong>{t('Add a version')}</strong><small>{t('from this library or from Tomodachi Life')}</small></span>
        </button>
      </div>

      <div class="actions">
        <button class="btn" data-focusable data-key="edit" onclick={() => goto(`/mii/editor?mii=${m.id}`)}>
          {versions.length > 1 ? t('Edit this version') : t('Edit the Mii')}
        </button>
        <button class="btn" data-focusable data-key="send" onclick={() => pickProfile(p)}>{t('Send a copy to another profile')}</button>
        <button
          class="btn danger"
          class:armed={confirming}
          data-focusable
          data-key="delete"
          onclick={() => del(p)}
          onblur={() => (confirming = false)}
        >
          {confirming
            ? versions.length > 1
              ? t('Press again to delete {name} and its {n} versions', { name: displayName(p), n: versions.length })
              : t('Press again to delete {name}', { name: displayName(p) })
            : t('Delete')}
        </button>
      </div>
    </section>
  </div>

  {#if sheet}
    <Picker light title={sheet.title} options={sheet.options} value={sheet.value} onpick={sheet.onpick} onclose={() => closeSheet()} />
  {/if}
{:else if lib.loaded === auth.id}
  <div class="page"><p class="gone">{t('This Mii no longer exists.')}</p></div>
{/if}

<style>
  .page {
    --ink: #2b2f38;
    --soft: #7b818d;
    --line: #e3e6eb;
    --tint: #3aa6c9;
    position: absolute;
    inset: 0;
    background: linear-gradient(#ffffff, #f1f3f6);
    color: var(--ink);
  }

  /* the Mii stands on the left, turned with the right stick or LB / RB */
  .stage { position: fixed; left: 0; top: 0; bottom: 0; width: 46rem; display: grid; place-items: center; }
  .stage img { position: relative; width: 44rem; height: 44rem; object-fit: contain; }
  .live { position: absolute; inset: 0; }
  .hint { position: absolute; bottom: 2.4rem; left: 0; right: 0; text-align: center; font-size: 1.1rem; color: var(--soft); }
  .hint kbd {
    font: inherit; font-size: 0.95rem; font-weight: 700; padding: 0.05rem 0.45rem;
    border: 1px solid var(--line); border-radius: 0.4rem; color: var(--ink);
  }

  .info { margin-left: 46rem; padding: 4.2rem 6rem 8rem 1rem; max-width: 80rem; }
  h1 { font-size: 3.6rem; font-weight: 700; letter-spacing: -0.02em; }
  header p { margin-top: 0.5rem; font-size: 1.35rem; color: var(--soft); display: flex; align-items: center; gap: 0.6rem; }
  .swatch { width: 1.2rem; height: 1.2rem; border-radius: 50%; background: var(--fav); }
  h2 { margin: 2.8rem 0 1rem; font-size: 1.25rem; font-weight: 700; color: var(--soft); }

  .fields { display: grid; grid-template-columns: 1fr 1fr; gap: 1.2rem 1.6rem; }
  label { display: grid; gap: 0.45rem; font-size: 1.2rem; color: var(--soft); }
  label.wide { grid-column: 1 / -1; }
  label small { font-size: 1.05rem; }
  input {
    font: inherit; font-size: 1.6rem; color: var(--ink); padding: 0.95rem 1.3rem; border-radius: 1.2rem;
    background: #fff; border: 0.2rem solid var(--line); outline: none;
  }
  input::placeholder { color: #c3c8d0; }
  input:focus { border-color: var(--tint); box-shadow: 0 0 0 0.3rem rgba(58, 166, 201, 0.2); }

  .row {
    width: 100%; display: flex; justify-content: space-between; align-items: center;
    padding: 1.1rem 1.4rem; border-radius: 1.2rem; background: #fff; border: 0.2rem solid var(--line);
    font-size: 1.6rem; text-align: left;
  }
  .row small { font-size: 1.2rem; color: var(--soft); }
  .row:focus { border-color: var(--tint); box-shadow: 0 0 0 0.3rem rgba(58, 166, 201, 0.2); }

  .kin { display: flex; flex-wrap: wrap; gap: 1rem; }
  .chip {
    display: flex; align-items: center; gap: 1rem; padding: 0.6rem 1.4rem 0.6rem 0.6rem; border-radius: 4rem;
    background: #fff; border: 0.2rem solid var(--line); text-align: left;
  }
  .chip.guess { border-style: dashed; background: rgba(255, 255, 255, 0.6); }
  .chip:focus { border-color: var(--c, var(--tint)); border-style: solid; box-shadow: 0 0 0 0.3rem color-mix(in srgb, var(--c, var(--tint)) 30%, transparent); }
  .chip .pic {
    width: 4.4rem; height: 4.4rem; border-radius: 50%; overflow: hidden; display: grid; place-items: center;
    background: color-mix(in srgb, var(--c, var(--tint)) 14%, #f4f6f8); font-size: 2.2rem; color: var(--soft);
  }
  .chip .pic img { width: 100%; height: 100%; object-fit: cover; }
  /* the version on show */
  .chip.on { border-color: color-mix(in srgb, var(--c, var(--tint)) 55%, var(--line)); }
  /* a game's Mii AzotoTV can't draw yet */
  .chip.game .pic { font-size: 1.3rem; font-weight: 700; color: #fff; background: #e8732c; }
  .chip.game .pic.drawn { background: #f6e3d6; }
  .note { margin: -0.4rem 0 1rem; font-size: 1.1rem; color: var(--soft); }
  .chip .txt { display: grid; }
  .chip strong { font-size: 1.4rem; font-weight: 600; }
  .chip small { font-size: 1.1rem; color: var(--soft); }

  .actions { margin-top: 3.4rem; display: flex; gap: 1.2rem; flex-wrap: wrap; }
  .btn {
    font-size: 1.35rem; font-weight: 600; padding: 1rem 2rem; border-radius: 2rem;
    background: #eef1f4; color: var(--ink);
  }
  .btn:focus { background: var(--tint); color: #fff; }
  .btn.danger { color: #c0392b; }
  .btn.danger:focus, .btn.danger.armed { background: #d64534; color: #fff; }

  .gone { padding: 6rem; font-size: 1.6rem; color: var(--soft); }
</style>
