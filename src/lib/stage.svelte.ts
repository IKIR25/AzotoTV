// What the persistent layer behind the UI should show.
export const stage = $state<{ backdrop: string | null; ambient: boolean }>({ backdrop: null, ambient: false });
/** Bumped when playback ends so pages re-fetch resume points / watched state. */
export const refresh = $state({ n: 0 });
export const toast = $state<{ msg: string }>({ msg: '' });
/** The START menu (profile, desktop, power), its "Notifiche" sheet and "Collega un telefono". */
export const control = $state({ open: false, notes: false, phones: false });
/** Set by an open overlay (e.g. the track picker): B / Esc closes it instead of going back. */
export const modal: { close: (() => void) | null } = { close: null };
/** Set by a page with tabs of its own (Giochi): LB / RB go to it (-1 / 1) instead of the top bar. */
export const shoulder: { tab: ((d: number) => void) | null } = { tab: null };
let timer: ReturnType<typeof setTimeout>;
export function showToast(msg: string, ms = 2600) {
  toast.msg = msg;
  clearTimeout(timer);
  timer = setTimeout(() => (toast.msg = ''), ms);
}
