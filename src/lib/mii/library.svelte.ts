// The active profile's Miis (mii.rs keeps them in `players/<id>/miis.json`).
//
// A person (Mamma) is what only AzotoTV knows: real names, birthday, family.
// A person has one or more Miis: their versions, each its own Mii for the
// consoles (its own look and identity), like Mamma's Mii on the Switch and
// the one Tomodachi Life made of her. A Mii is kept as Switch StoreData (the
// look, with the Mii name games show) plus its Wii version. Games whose Miis
// AzotoTV can't draw yet are linked to a person instead (`links`).
// Each profile has its own; sending a person to another profile gives them an
// independent copy.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { showToast } from '../stage.svelte';
import { b64, decode, freshNxId, nxId, rvlId, toNx, toRvl, unb64, type Look, type Mii } from './miijs';
import { toCharInfo } from './tomodachi';
import { t, tn } from '$lib/i18n';

export type RelKind = 'parent' | 'partner' | 'sibling' | 'friend';

/**
 * Kept once, on one of the two people; the other side is worked out.
 * parent: this person is a parent of `to`. sibling: `older` = this one is the
 * older one. partner: `married`. friend: `best`.
 */
export type Relation = { to: string; kind: RelKind; older?: boolean; married?: boolean; best?: boolean };

export type Names = { first?: string; short?: string; surname?: string; nickname?: string };
export type Birthday = { month: number; day: number; year?: number };
export type Origin = 'switch' | 'wii' | 'azoto' | 'miitopia' | 'tomodachi';

/** A game's Mii AzotoTV can't draw yet, linked to a person. */
export type GameLink = { game: 'tomodachi'; id: string; name: string };

export type PersonEntry = {
  id: string;
  names: Names;
  birthday?: Birthday;
  relations: Relation[];
  /** the Mii shown for them (else their first) */
  main?: string;
  links: GameLink[];
  created: number;
  updated: number;
};

export type MiiEntry = {
  id: string;
  person: string;
  /** Switch StoreData, base64: the appearance and the Mii name */
  nx: string;
  /** Wii RFLCharData, base64, made from `nx` (keeps its Wii Mii ID) */
  rvl?: string;
  /** where it was first found */
  from: Origin;
  created: number;
  updated: number;
};

/** A Mii with its decoded look. */
export type Entry = MiiEntry & { look: Look };

export const lib = $state<{ people: PersonEntry[]; miis: Entry[]; loaded: string; error: string }>({
  people: [],
  miis: [],
  loaded: '',
  error: '',
});

const newId = () => crypto.randomUUID().replaceAll('-', '');

async function withLook(e: MiiEntry): Promise<Entry> {
  const m = await decode(unb64(e.nx));
  return { ...e, look: m.toJSON() };
}

const strip = ({ look: _, ...e }: Entry): MiiEntry => e;

// ---- reading ----------------------------------------------------------------------

export const byId = (id: string) => lib.people.find((p) => p.id === id);
export const miisOf = (p: PersonEntry) => lib.miis.filter((m) => m.person === p.id);
export const mainOf = (p: PersonEntry): Entry | undefined => {
  const all = miisOf(p);
  return all.find((m) => m.id === p.main) ?? all[0];
};
/** The person's Miis, the main one first. */
export const versionsOf = (p: PersonEntry) => {
  const main = mainOf(p);
  return main ? [main, ...miisOf(p).filter((m) => m !== main)] : [];
};

/**
 * The name to show: the short name, else the Mii name (what the Miis are
 * called everywhere), else the first name.
 */
export const displayName = (p: PersonEntry) =>
  p.names.short || mainOf(p)?.look.meta.name || p.names.first || p.links[0]?.name || '?';

export const isFemale = (p: PersonEntry) => mainOf(p)?.look.general.gender === 1;

// ---- loading (and moving a v1 library, one Mii = one person, to people) --------

type RawMii = MiiEntry & { names?: Names; birthday?: Birthday; relations?: Relation[] };

export async function loadMiis(user: string) {
  try {
    const raw = await invoke<{ people?: PersonEntry[]; miis: RawMii[] }>('mii_library');
    const people = (raw.people ?? []).map((p) => ({ ...p, names: p.names ?? {}, relations: p.relations ?? [], links: p.links ?? [] }));
    const moved: PersonEntry[] = [];
    const miis: MiiEntry[] = raw.miis.map(({ names, birthday, relations, ...m }) => {
      if (m.person && people.some((p) => p.id === m.person)) return m;
      // a v1 Mii: it becomes a person with the same id (relations stay valid)
      const p: PersonEntry = {
        id: m.id,
        names: names ?? {},
        birthday,
        relations: relations ?? [],
        links: [],
        created: m.created,
        updated: m.updated,
      };
      people.push(p);
      moved.push(p);
      return { ...m, person: p.id };
    });
    lib.people = people;
    lib.miis = await Promise.all(miis.map(withLook));
    lib.loaded = user;
    lib.error = '';
    if (moved.length) {
      await invoke('mii_people_put', { people: moved, to: null });
      await invoke('mii_put', { miis: miis.filter((m) => moved.some((p) => p.id === m.person)), to: null });
    }
  } catch (e) {
    lib.error = String(e);
    return;
  }
  listenForGames();
  await mergeInbox().catch((e) => console.warn('[mii] inbox', e));
}

// ---- saving -----------------------------------------------------------------------

export async function savePeople(people: PersonEntry[], to?: string) {
  const now = Date.now();
  const out: PersonEntry[] = people.map((p) => ({ ...$state.snapshot(p), updated: now }));
  await invoke('mii_people_put', { people: out, to: to ?? null });
  if (to) return;
  for (const p of out) {
    const i = lib.people.findIndex((x) => x.id === p.id);
    if (i >= 0) lib.people[i] = p;
    else lib.people.push(p);
  }
}

export async function saveMiis(entries: Entry[], to?: string) {
  await invoke('mii_put', { miis: entries.map((e) => strip($state.snapshot(e) as Entry)), to: to ?? null });
  if (to) return;
  for (const e of entries) {
    const i = lib.miis.findIndex((m) => m.id === e.id);
    if (i >= 0) lib.miis[i] = e;
    else lib.miis.push(e);
  }
}

/** The Mii's encodings for every console (the Wii one keeps its Mii ID, and takes the birthday). */
async function encoded(e: MiiEntry, m: Mii, birthday?: Birthday): Promise<Entry> {
  const before = e.rvl ? unb64(e.rvl) : undefined;
  // the Switch keeps no birthday; the Wii does
  m.set('general.birthMonth', birthday?.month ?? 0);
  m.set('general.birthday', birthday?.day ?? 0);
  const nx = toNx(m);
  return { ...e, nx: b64(nx), rvl: b64(toRvl(m, before)), updated: Date.now(), look: (await decode(nx)).toJSON() };
}

/** A changed look (an edited Mii): re-encodes it for every console. */
export async function saveLook(e: Entry, m: Mii) {
  await saveMiis([await encoded(e, m, byId(e.person)?.birthday)]);
}

export const saveInfo = (p: PersonEntry) => savePeople([p]);

/** The Mii name games show (10 characters at most). */
export async function rename(e: Entry, name: string) {
  const m = await decode(unb64(e.nx));
  m.set('meta.name', name.slice(0, 10));
  await saveLook(e, m);
}

/** The birthday also goes into each Mii's Wii version. */
export async function setBirthday(p: PersonEntry, birthday: Birthday | null) {
  p.birthday = birthday ?? undefined;
  await savePeople([p]);
  const miis = await Promise.all(miisOf(p).map(async (e) => encoded(e, await decode(unb64(e.nx)), p.birthday)));
  if (miis.length) await saveMiis(miis);
}

/** A new Mii (made in the editor) as another version of `p`. */
export async function addVersion(p: PersonEntry, m: Mii) {
  const now = Date.now();
  const e: MiiEntry = { id: newId(), person: p.id, nx: '', from: 'azoto', created: now, updated: now };
  const done = await encoded(e, m, p.birthday);
  await saveMiis([done]);
  return done;
}

/** A new Mii (made in the editor) as a new person. */
export async function newPerson(m: Mii) {
  const now = Date.now();
  const p: PersonEntry = { id: newId(), names: {}, relations: [], links: [], created: now, updated: now };
  await savePeople([p]);
  await addVersion(p, m);
  return p;
}

/** A person and all their Miis; others' relations to them go too. */
export async function removePerson(p: PersonEntry) {
  await invoke('mii_person_remove', { id: p.id });
  lib.people = lib.people.filter((x) => x.id !== p.id);
  lib.miis = lib.miis.filter((m) => m.person !== p.id);
  const touched = lib.people.filter((x) => x.relations.some((r) => r.to === p.id));
  for (const x of touched) x.relations = x.relations.filter((r) => r.to !== p.id);
  if (touched.length) await savePeople(touched);
}

/** One version of a person (not their last one: that's removePerson). */
export async function removeMii(e: Entry) {
  await invoke('mii_remove', { id: e.id });
  lib.miis = lib.miis.filter((m) => m.id !== e.id);
  const p = byId(e.person);
  if (p?.main === e.id) {
    p.main = undefined;
    await savePeople([p]);
  }
}

export async function setMain(p: PersonEntry, e: Entry) {
  p.main = e.id;
  await savePeople([p]);
}

const sameDay = (a: Birthday, b: Birthday) => a.month === b.month && a.day === b.day;

/**
 * Two people are the same person: `from`'s Miis, links and family move to
 * `into`, and what `into` doesn't know yet (names, birthday) comes from `from`.
 */
export async function mergePeople(into: PersonEntry, from: PersonEntry) {
  const moved = miisOf(from).map((m) => ({ ...m, person: into.id }));
  for (const k of ['first', 'short', 'surname', 'nickname'] as const) into.names[k] ||= from.names[k];
  if (!into.birthday) into.birthday = from.birthday;
  else if (from.birthday && !into.birthday.year && sameDay(into.birthday, from.birthday))
    into.birthday = { ...into.birthday, year: from.birthday.year };
  for (const l of from.links) if (!into.links.some((x) => x.id === l.id)) into.links.push(l);
  for (const r of from.relations)
    if (r.to !== into.id && !into.relations.some((x) => x.to === r.to)) into.relations.push(r);
  const touched: PersonEntry[] = [];
  for (const x of lib.people) {
    if (x.id === from.id || x.id === into.id) continue;
    if (x.relations.some((r) => r.to === from.id)) {
      x.relations = x.relations
        .map((r) => (r.to === from.id ? { ...r, to: into.id } : r))
        .filter((r, i, all) => all.findIndex((y) => y.to === r.to) === i);
      touched.push(x);
    }
  }
  into.relations = into.relations.filter((r) => r.to !== from.id);
  // the Miis first: removing `from` takes the Miis still pointing at it along
  if (moved.length) await saveMiis(moved);
  await savePeople([into, ...touched]);
  await invoke('mii_person_remove', { id: from.id });
  lib.people = lib.people.filter((x) => x.id !== from.id);
}

/** A version becomes a person of its own (no names or family yet). */
export async function splitMii(e: Entry) {
  const old = byId(e.person);
  const now = Date.now();
  const p: PersonEntry = { id: newId(), names: {}, relations: [], links: [], created: now, updated: now };
  await savePeople([p]);
  await saveMiis([{ ...e, person: p.id }]);
  if (old?.main === e.id) {
    old.main = undefined;
    await savePeople([old]);
  }
  return p;
}

/**
 * Gives `profile` its own copy of a person: new ids (each Mii's Switch create
 * ID included, Wii IDs made fresh), no family or game links (they belong to
 * this profile).
 */
export async function sendCopy(p: PersonEntry, profile: string) {
  const now = Date.now();
  const id = newId();
  const miis: MiiEntry[] = [];
  let main: string | undefined;
  for (const e of versionsOf(p)) {
    const nx = toNx(await decode(unb64(e.nx)));
    freshNxId(nx);
    const copy = await decode(nx);
    const mid = newId();
    main ??= mid;
    miis.push({ id: mid, person: id, nx: b64(toNx(copy)), rvl: b64(toRvl(copy)), from: 'azoto', created: now, updated: now });
  }
  const person: PersonEntry = { ...$state.snapshot(p), id, relations: [], links: [], main, created: now, updated: now };
  await invoke('mii_people_put', { people: [person], to: profile });
  await invoke('mii_put', { miis, to: profile });
}

// ---- importing from the consoles and games ----------------------------------------

export type Found = { console: Origin; raw: Uint8Array; look: Look };
/** A Mii in a Tomodachi Life: Living the Dream save (mii.rs); `ex` = its CharInfoEx. */
export type TomodachiMii = { id: string; ex: string; miiName: string; name: string; call: string; day: number; month: number; year: number };

/** What games know that the library can't show as Miis yet (the person page offers it). */
export const games = $state<{ tomodachi: TomodachiMii[] }>({ tomodachi: [] });

/** Tomodachi Life Miis drawn as Switch Miis (StoreData, base64) by
 * `id:favourite colour`: their new parts become the closest Switch ones
 * (tomodachi.ts), and the game has no favourite colour, so the person's is used. */
const tomoLooks = $state<Record<string, string>>({});
const tomoPending = new Set<string>();

/** A Tomodachi Life Mii's look, if made (else starts making it: reactive). */
export function tomoLook(id: string, favorite = 0): string | undefined {
  const key = `${id}:${favorite}`;
  if (tomoLooks[key] || tomoPending.has(key)) return tomoLooks[key];
  const t = games.tomodachi.find((x) => x.id === id);
  if (!t) return undefined;
  tomoPending.add(key);
  decode(toCharInfo(unb64(t.ex), t.name || t.miiName, favorite))
    .then((m) => (tomoLooks[key] = b64(toNx(m))))
    .catch((e) => console.warn('[mii] tomodachi look', t.name, e));
  return undefined;
}

type Sources = { switch: string[]; wii: string[]; miitopia?: string[]; tomodachi?: TomodachiMii[] };

/** Miis on Eden, this profile's Wii and its Miitopia and Tomodachi Life saves that the library doesn't have yet. */
export async function findNew(): Promise<Found[]> {
  const src = await invoke<Sources>('mii_sources');
  games.tomodachi = src.tomodachi ?? [];
  const haveNx = new Set(lib.miis.map((m) => nxId(unb64(m.nx))));
  const haveRvl = new Set(lib.miis.flatMap((m) => (m.rvl ? [rvlId(unb64(m.rvl))] : [])));
  const out: Found[] = [];
  const add = async (console: Origin, s: string, have: Set<string>, id: string) => {
    if (have.has(id)) return;
    have.add(id); // a Mii in two places is offered once
    const raw = unb64(s);
    out.push({ console, raw, look: (await decode(raw)).toJSON() });
  };
  for (const s of src.switch) await add('switch', s, haveNx, nxId(unb64(s)));
  for (const s of src.wii) await add('wii', s, haveRvl, rvlId(unb64(s)));
  // Miitopia keeps CharInfo: the create ID comes first
  for (const s of src.miitopia ?? []) await add('miitopia', s, haveNx, hexOf(unb64(s).subarray(0, 16)));
  // Tomodachi Life: the Miis made in the game (a library Mii there has its
  // create ID; one linked to a person is already theirs), drawn with the
  // closest Switch parts. Once added, linkKnownTomodachi() links them by
  // create ID and brings their name and birthday.
  for (const t of freeTomodachi()) {
    try {
      await add('tomodachi', b64(toCharInfo(unb64(t.ex), t.name || t.miiName)), haveNx, t.id);
    } catch (e) {
      console.warn('[mii] tomodachi', t.name, e);
    }
  }
  return out;
}

const hexOf = (b: Uint8Array) => Array.from(b, (x) => x.toString(16).padStart(2, '0')).join('');

export async function importFound(found: Found[]) {
  const now = Date.now();
  const people: PersonEntry[] = [];
  const miis: Entry[] = [];
  for (const f of found) {
    const m = await decode(f.raw);
    const g = m.toJSON().general;
    const p: PersonEntry = {
      id: newId(),
      names: {},
      birthday: g.birthMonth && g.birthday ? { month: g.birthMonth, day: g.birthday } : undefined,
      relations: [],
      links: [],
      created: now,
      updated: now,
    };
    const nx = f.console === 'switch' ? f.raw : toNx(m);
    people.push(p);
    miis.push({
      id: newId(),
      person: p.id,
      nx: b64(nx),
      // a Wii Mii keeps its own bytes (ID, creator, birthday) until it's edited
      rvl: f.console === 'wii' ? b64(f.raw) : b64(toRvl(m)),
      from: f.console,
      created: now,
      updated: now,
      look: (await decode(nx)).toJSON(),
    });
  }
  await savePeople(people);
  await saveMiis(miis);
}

/** A Tomodachi Life Mii's birthday, if it has a real one. */
function tomoBirthday(t: TomodachiMii): Birthday | undefined {
  if (t.month < 1 || t.month > 12 || t.day < 1 || t.day > 31) return undefined;
  return { month: t.month, day: t.day, year: t.year > 1900 ? t.year : undefined };
}

/**
 * Links a Tomodachi Life Mii to a person, and fills in what the person
 * doesn't have yet: the first name and the birthday (with the year). True if
 * anything was filled in.
 */
export async function linkTomodachi(p: PersonEntry, t: TomodachiMii) {
  let filled = false;
  if (!p.links.some((l) => l.id === t.id)) p.links.push({ game: 'tomodachi', id: t.id, name: t.name || t.miiName });
  if (!p.names.first && t.name) {
    p.names.first = t.name;
    filled = true;
  }
  const bd = tomoBirthday(t);
  if (bd && (!p.birthday || (sameDay(p.birthday, bd) && !p.birthday.year && bd.year))) {
    await setBirthday(p, bd);
    filled = true;
  } else await savePeople([p]);
  return filled;
}

export async function unlink(p: PersonEntry, l: GameLink) {
  p.links = p.links.filter((x) => x.id !== l.id);
  await savePeople([p]);
}

/** Tomodachi Life Miis that are one of the library's Miis (same create ID): linked, with names and birthdays. */
export async function linkKnownTomodachi() {
  const done: string[] = [];
  for (const t of games.tomodachi) {
    const e = lib.miis.find((m) => nxId(unb64(m.nx)) === t.id);
    const p = e && byId(e.person);
    if (!p || p.links.some((l) => l.id === t.id)) continue;
    if (await linkTomodachi(p, t)) done.push(displayName(p));
  }
  if (done.length) showToast(t('From Tomodachi Life: name and birthday of {a}', { a: done.join(', ') }), 4000);
}

/** The Tomodachi Life Miis no person is linked to yet. */
export const freeTomodachi = () => games.tomodachi.filter((t) => !lib.people.some((p) => p.links.some((l) => l.id === t.id)));

// ---- relations ------------------------------------------------------------------

/** Sets how `a` relates to `b` (null removes it), replacing what was there. */
export async function relate(a: PersonEntry, b: PersonEntry, rel: Omit<Relation, 'to'> | null) {
  a.relations = a.relations.filter((r) => r.to !== b.id);
  b.relations = b.relations.filter((r) => r.to !== a.id);
  if (rel) a.relations.push({ ...rel, to: b.id });
  await savePeople([a, b]);
}

export type Kin = { person: PersonEntry; label: string; inferred: boolean };

const g = (p: PersonEntry, male: string, female: string) => (isFemale(p) ? female : male);

/** Who's older: the stored flag, else unknown. */
function siblingLabel(other: PersonEntry, olderIsOther: boolean | undefined) {
  const base = g(other, t('Brother'), t('Sister'));
  if (olderIsOther === undefined) return base;
  return `${base} ${olderIsOther ? 'maggiore' : 'minore'}`;
}

/** Everyone `me` is related to, with what they are to `me` ("Mamma", "Fratello minore"…). */
export function kinOf(me: PersonEntry): Kin[] {
  const all = lib.people;
  const get = (id: string) => all.find((m) => m.id === id);
  const parentsOf = (m: PersonEntry) => all.filter((p) => p.relations.some((r) => r.kind === 'parent' && r.to === m.id));
  const childrenOf = (m: PersonEntry) => m.relations.filter((r) => r.kind === 'parent').flatMap((r) => get(r.to) ?? []);
  /** explicit siblings (with who's older) and those sharing a parent */
  const siblingsOf = (m: PersonEntry) => {
    const out = new Map<string, { p: PersonEntry; olderIsOther?: boolean; inferred: boolean }>();
    for (const r of m.relations)
      if (r.kind === 'sibling') {
        const o = get(r.to);
        if (o) out.set(o.id, { p: o, olderIsOther: r.older === undefined ? undefined : !r.older, inferred: false });
      }
    for (const o of all)
      for (const r of o.relations)
        if (r.kind === 'sibling' && r.to === m.id) out.set(o.id, { p: o, olderIsOther: r.older, inferred: false });
    for (const p of parentsOf(m))
      for (const c of childrenOf(p)) if (c.id !== m.id && !out.has(c.id)) out.set(c.id, { p: c, inferred: true });
    return [...out.values()];
  };
  /** everyone linked to `m` either way */
  const linked = (m: PersonEntry) => {
    const out: { p: PersonEntry; r: Relation }[] = [];
    for (const r of m.relations) {
      const o = get(r.to);
      if (o) out.push({ p: o, r });
    }
    for (const o of all) for (const r of o.relations) if (r.to === m.id) out.push({ p: o, r });
    return out;
  };

  const out = new Map<string, Kin>();
  const add = (p: PersonEntry, label: string, inferred: boolean) => {
    if (p.id !== me.id && !out.has(p.id)) out.set(p.id, { person: p, label, inferred });
  };

  for (const p of parentsOf(me)) add(p, g(p, t('Dad'), t('Mom')), false);
  for (const c of childrenOf(me)) add(c, g(c, t('Son'), t('Daughter')), false);
  for (const { p, r } of linked(me)) {
    if (r.kind === 'partner') add(p, r.married ? g(p, t('Husband'), t('Wife')) : g(p, t('Boyfriend'), t('Girlfriend')), false);
    if (r.kind === 'friend') add(p, r.best ? g(p, t('Best friend|m'), t('Best friend|f')) : g(p, t('Friend|m'), t('Friend|f')), false);
  }
  for (const s of siblingsOf(me)) add(s.p, siblingLabel(s.p, s.olderIsOther), s.inferred);
  // a parent's partner who isn't a parent yet
  for (const p of parentsOf(me)) for (const k of linked(p)) if (k.r.kind === 'partner') add(k.p, g(k.p, t('Dad'), t('Mom')), true);
  for (const p of parentsOf(me)) {
    for (const gp of parentsOf(p)) add(gp, g(gp, t('Grandpa'), t('Grandma')), true);
    for (const u of siblingsOf(p)) {
      add(u.p, g(u.p, t('Uncle'), t('Aunt')), true);
      for (const c of childrenOf(u.p)) add(c, g(c, t('Cousin|m'), t('Cousin|f')), true);
    }
  }
  // a partner's children
  for (const k of linked(me)) if (k.r.kind === 'partner') for (const c of childrenOf(k.p)) add(c, g(c, t('Son'), t('Daughter')), true);
  for (const c of childrenOf(me)) for (const gc of childrenOf(c)) add(gc, t('Grandchild'), true);
  for (const s of siblingsOf(me)) for (const n of childrenOf(s.p)) add(n, t('Grandchild'), true);
  return [...out.values()];
}

// ---- editing a relation from one person's page ------------------------------------

/** What another person is to this one, as chosen on its page. */
export type Tie = 'parent' | 'child' | 'older' | 'younger' | 'sibling' | 'spouse' | 'partner' | 'best' | 'friend';

export const TIES: Tie[] = ['parent', 'child', 'older', 'younger', 'sibling', 'spouse', 'partner', 'best', 'friend'];

/** `other`'s tie to `me` as set by hand (inferred ones aren't). */
export function tieOf(me: PersonEntry, other: PersonEntry): Tie | null {
  const theirs = other.relations.find((r) => r.to === me.id);
  const mine = me.relations.find((r) => r.to === other.id);
  const r = theirs ?? mine;
  if (!r) return null;
  const fromOther = r === theirs;
  if (r.kind === 'parent') return fromOther ? 'parent' : 'child';
  if (r.kind === 'sibling') {
    if (r.older === undefined) return 'sibling';
    // `older` means the one holding the relation is the older one
    return r.older === fromOther ? 'older' : 'younger';
  }
  if (r.kind === 'partner') return r.married ? 'spouse' : 'partner';
  return r.best ? 'best' : 'friend';
}

export async function setTie(me: PersonEntry, other: PersonEntry, tie: Tie | null) {
  switch (tie) {
    case null: return relate(me, other, null);
    case 'parent': return relate(other, me, { kind: 'parent' });
    case 'child': return relate(me, other, { kind: 'parent' });
    case 'older': return relate(other, me, { kind: 'sibling', older: true });
    case 'younger': return relate(other, me, { kind: 'sibling', older: false });
    case 'sibling': return relate(other, me, { kind: 'sibling' });
    case 'spouse': return relate(me, other, { kind: 'partner', married: true });
    case 'partner': return relate(me, other, { kind: 'partner' });
    case 'best': return relate(me, other, { kind: 'friend', best: true });
    case 'friend': return relate(me, other, { kind: 'friend' });
  }
}

/** "Mamma", "Fratello maggiore"…: what `other` would be, by their gender. */
export function tieLabel(other: PersonEntry, tie: Tie): string {
  const [m, f] = {
    parent: [t('Dad'), t('Mom')],
    child: [t('Son'), t('Daughter')],
    older: [t('Older brother'), t('Older sister')],
    younger: [t('Younger brother'), t('Younger sister')],
    sibling: [t('Brother'), t('Sister')],
    spouse: [t('Husband'), t('Wife')],
    partner: [t('Boyfriend'), t('Girlfriend')],
    best: [t('Best friend|m'), t('Best friend|f')],
    friend: [t('Friend|m'), t('Friend|f')],
  }[tie];
  return g(other, m, f);
}

// ---- what games changed (mii.rs puts the consoles' databases in the inbox) --------

type InboxDb = { name: string; console: 'switch' | 'wii'; miis: string[] };

let listening = false;
function listenForGames() {
  if (listening) return;
  listening = true;
  // mii.rs has read the console's Miis back by the time this arrives
  listen('game-ended', () => {
    if (lib.loaded) loadMiis(lib.loaded);
  });
}

const sameBytes = (a: Uint8Array, b: Uint8Array) => a.length === b.length && a.every((x, i) => x === b[i]);

/** What a Mii looks like, apart from its name, ids and birthday. */
function appearance(l: Look) {
  const { meta: _, general, ...parts } = l;
  const { birthMonth: _m, birthday: _d, ...g } = general;
  return JSON.stringify({ g, parts });
}

let merging: Promise<void> | null = null;

/** Brings in Miis made or edited in games. Miis deleted there stay here. */
export function mergeInbox() {
  merging ??= doMerge().finally(() => (merging = null));
  return merging;
}

async function doMerge() {
  const dbs = await invoke<InboxDb[]>('mii_inbox');
  let added = 0;
  let changed = 0;
  for (const db of dbs) {
    for (const s of db.miis) {
      const raw = unb64(s);
      if (db.console === 'switch') {
        const e = lib.miis.find((m) => nxId(unb64(m.nx)) === nxId(raw));
        if (!e) {
          await importFound([{ console: 'switch', raw, look: (await decode(raw)).toJSON() }]);
          added++;
        } else if (!sameBytes(unb64(e.nx).subarray(0, 0x40), raw.subarray(0, 0x40))) {
          await saveLook(e, await decode(raw)); // edited in a Switch game or the Mii editor
          changed++;
        }
        continue;
      }
      const e = lib.miis.find((m) => m.rvl && rvlId(unb64(m.rvl)) === rvlId(raw));
      if (!e) {
        await importFound([{ console: 'wii', raw, look: (await decode(raw)).toJSON() }]);
        added++;
        continue;
      }
      // the console ID is AzotoTV's to set (mii.rs), not a change
      const mine = unb64(e.rvl!).slice();
      mine.set(raw.subarray(0x1c, 0x20), 0x1c);
      if (sameBytes(mine, raw)) continue;
      const wii = await decode(raw);
      const was = (await decode(mine)).toJSON();
      const now = wii.toJSON();
      // a new look from the Wii replaces the Switch one (keeping its Switch
      // identity); otherwise the Switch version only takes the new name
      const base = appearance(now) !== appearance(was) ? wii : await decode(unb64(e.nx));
      if (base !== wii) base.set('meta.name', now.meta.name);
      const nx = toNx(base);
      nx.set(unb64(e.nx).subarray(0x30, 0x40), 0x30);
      await saveMiis([{ ...e, nx: b64(nx), rvl: b64(raw), updated: Date.now(), look: (await decode(nx)).toJSON() }]);
      // a birthday set on the Wii is the person's (a known year stays if it's the same day)
      const p = byId(e.person);
      const g = now.general;
      if (p && g.birthMonth && g.birthday) {
        const bd = { month: g.birthMonth, day: g.birthday };
        if (!p.birthday || !sameDay(p.birthday, bd)) {
          p.birthday = bd;
          await savePeople([p]);
        }
      }
      changed++;
    }
    await invoke('mii_inbox_done', { names: [db.name] });
  }
  const said = [added && tn(added, '{n} new|one Mii', '{n} new|Miis'), changed && tn(changed, '{n} changed|one Mii', '{n} changed|Miis')];
  if (added || changed) showToast(t('Miis from the games: {a}', { a: said.filter(Boolean).join(', ') }));
}
