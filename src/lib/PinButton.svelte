<script lang="ts">
  import { t } from '$lib/i18n';
  import { isPinned, togglePin, type Pin } from './pins.svelte';

  // "＋ Home" on a film's / game's / playlist's page: a tile for it on the home, next to the apps.
  let { pin }: { pin: Pin } = $props();
  const on = $derived(isPinned(pin));
</script>

<button class="btn icon" data-focusable data-key="pin" data-scroll-top onclick={() => togglePin(pin)}>
  {on ? t('✓ On Home') : t('＋ Home')}
</button>
