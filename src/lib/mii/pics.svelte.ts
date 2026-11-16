// Pictures of Miis as reactive state, so pages and the home shelf update when
// one is drawn. Keyed by the Mii's look (its StoreData), so an edit redraws.
import { picture, type Shot } from './render';

export const faces = $state<Record<string, string>>({});
/** drawing failed (no FFLResHigh.dat, no WebGL): not tried again until the app restarts */
const failed = new Set<string>();

/** The face picture's URL if drawn, else starts drawing it and returns null. */
export function face(nx: string, shot: Shot = { size: 384 }): string | null {
  const key = `${shot.body ? 'b' : 'f'}${shot.size}:${nx}`;
  if (faces[key]) return faces[key];
  if (failed.has(key)) return null;
  picture(nx, shot)
    .then((url) => (faces[key] = url))
    .catch((e) => {
      failed.add(key);
      console.warn('[mii] draw', e);
    });
  return null;
}
