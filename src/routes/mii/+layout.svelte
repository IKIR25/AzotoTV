<script lang="ts">
  import { auth } from '$lib/session.svelte';
  import { music } from '$lib/music.svelte';
  import { player } from '$lib/player.svelte';
  import { saver } from '$lib/idle.svelte';
  import { loadTrack, miiMusic, setMusic } from '$lib/mii/music.svelte';

  // Every Mii page has the Mii maker's music.
  let { children } = $props();

  $effect(() => {
    if (auth.id) loadTrack(auth.id);
  });
  $effect(() => {
    const quiet = (music.active && !music.paused) || player.active || saver.dark;
    setMusic(quiet ? null : miiMusic.track);
  });
  $effect(() => () => setMusic(null));
</script>

{@render children()}
