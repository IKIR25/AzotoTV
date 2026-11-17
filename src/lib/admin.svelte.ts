// Is the current profile a Jellyfin administrator? (Services, adding and
// changing games: what acts for every profile.) Asked once per profile.
import { api } from './jf';
import { auth } from './session.svelte';

export const admin = $state({ user: '', is: false });

export function checkAdmin() {
  const user = auth.id;
  if (admin.user === user) return;
  admin.user = user;
  admin.is = false;
  api.me().then(
    (u) => admin.user === user && (admin.is = !!u.Policy?.IsAdministrator),
    () => {},
  );
}
