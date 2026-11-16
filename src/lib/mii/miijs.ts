// The Mii formats, through the vendored miijs (see $lib/vendor/miijs/README.md).
// A Mii's appearance is kept as Switch StoreData (0x44 bytes): the Switch
// has every part and colour of the older consoles, so nothing is lost; the
// Wii version (RFLCharData, 74 bytes) is made from it.
// @ts-ignore vendored JS, no types
import * as miijs from '$lib/vendor/miijs/miijs.browser.esm.js';

const { Mii: MiiClass, MiiFormats } = miijs as any;

/** What miijs decodes a Mii to (only the parts AzotoTV reads are typed). */
export type Look = {
  meta: { name: string; miiId: string; type?: string; creatorName?: string; systemId?: string };
  general: {
    gender: number;
    favoriteColor: number;
    height: number;
    weight: number;
    birthMonth?: number;
    birthday?: number;
  };
  [part: string]: any;
};

export interface Mii {
  toJSON(): Look;
  encode(format: string): Uint8Array;
  set(path: string | object, value?: unknown): void;
  toInstructions(console: string): Promise<unknown>;
}

export const NX_LEN = 0x44;
export const RVL_LEN = 74;

export function b64(bytes: Uint8Array): string {
  let s = '';
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s);
}

export function unb64(s: string): Uint8Array {
  const t = atob(s);
  const out = new Uint8Array(t.length);
  for (let i = 0; i < t.length; i++) out[i] = t.charCodeAt(i);
  return out;
}

export const hex = (b: Uint8Array) => Array.from(b, (x) => x.toString(16).padStart(2, '0')).join('');

export async function decode(bytes: Uint8Array): Promise<Mii> {
  return MiiClass.create(bytes);
}

export function toNx(m: Mii): Uint8Array<ArrayBuffer> {
  const nx = Uint8Array.from(m.encode(MiiFormats.NFSD));
  // a Mii from the Wii has no Switch create ID: give it one
  if (nx.subarray(0x30, 0x40).every((b) => b === 0)) freshNxId(nx);
  return nx;
}

/** A new Switch create ID: a random UUID (v4), as the Switch makes them. */
export function freshNxId(nx: Uint8Array<ArrayBuffer>) {
  crypto.getRandomValues(nx.subarray(0x30, 0x40));
  nx[0x36] = (nx[0x36] & 0x0f) | 0x40;
  nx[0x38] = (nx[0x38] & 0x3f) | 0x80;
}

/**
 * The Wii version. `before` is the Mii's previous Wii version: its Mii ID and
 * console ID (bytes 0x18-0x1F) are kept, because Wii games remember a Mii by
 * them (Mario Kart Wii licences, Wii Sports records) and miijs makes a new
 * ID from the clock every time.
 */
export function toRvl(m: Mii, before?: Uint8Array): Uint8Array {
  const out = new Uint8Array(m.encode(MiiFormats.RCD)).slice(0, RVL_LEN);
  if (before?.length === RVL_LEN) out.set(before.subarray(0x18, 0x20), 0x18);
  else freshRvlId(out);
  return out;
}

const WII_EPOCH = Date.UTC(2006, 0, 1);
let lastStamp = 0;

/**
 * A new Wii Mii ID: 3 flag bits (kept from miijs: an ordinary Mii) and the
 * time in 4-second steps since 2006, as the Wii makes it. miijs uses the
 * clock as is, so Miis made in the same second (an import) got the same ID;
 * here each one gets the next step.
 */
function freshRvlId(out: Uint8Array) {
  let t = Math.floor((Date.now() - WII_EPOCH) / 4000);
  if (t <= lastStamp) t = lastStamp + 1;
  lastStamp = t;
  const id = ((out[0x18] & 0xe0) << 24) | (t & 0x1fffffff);
  new DataView(out.buffer, out.byteOffset).setUint32(0x18, id >>> 0);
}

/** Switch create ID (identifies a Mii across Switch games). */
export const nxId = (nx: Uint8Array) => hex(nx.subarray(0x30, 0x40));
/** Wii Mii ID (without the console ID). */
export const rvlId = (rvl: Uint8Array) => hex(rvl.subarray(0x18, 0x1c));

export { miijs };
