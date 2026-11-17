<script lang="ts">
  import { t } from '$lib/i18n';
  import { api } from '$lib/jf';
  import { auth } from '$lib/session.svelte';
  import Library from '$lib/Library.svelte';

  // The Musica app: the music library on a page of its own (no TV top bar);
  // songs play in the background player, "In riproduzione" is /music.
  let id = $state('');
  let missing = $state(false);
  $effect(() => {
    void auth.id;
    api.musicView().then(
      (v) => ((id = v?.Id ?? ''), (missing = !v)),
      () => (missing = true),
    );
  });
</script>

{#if id}
  {#key id}<Library {id} app />{/key}
{:else if missing}
  <p class="msg">{t('No music library on this server.')}</p>
{/if}

<style>
  .msg { position: absolute; top: 12rem; left: 6rem; font-size: 1.8rem; color: var(--muted); }
</style>
