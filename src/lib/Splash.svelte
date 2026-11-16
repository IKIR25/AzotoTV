<script lang="ts">
  import Logo from './Logo.svelte';

  /** Full-screen logo while AzotoTV starts (and while Jellyfin is still booting). */
  let { note = '' }: { note?: string } = $props();
</script>

<div class="splash">
  <Logo size={4.4} />
  <span class="dots" aria-hidden="true"><i></i><i></i><i></i></span>
  {#if note}<p>{note}</p>{/if}
</div>

<style>
  .splash {
    position: absolute; inset: 0; z-index: 5; display: grid; place-content: center; justify-items: center;
    gap: 2.6rem; background: var(--bg);
  }
  .dots { display: flex; gap: 0.9rem; height: 1rem; }
  /* opacity only, endless: no transition end to glitch on (see tween.ts) */
  .dots i { width: 0.8rem; height: 0.8rem; border-radius: 50%; background: var(--azoto); animation: pulse 1.2s ease-in-out infinite; }
  .dots i:nth-child(2) { animation-delay: 0.2s; }
  .dots i:nth-child(3) { animation-delay: 0.4s; }
  @keyframes pulse { 0%, 100% { opacity: 0.2; } 50% { opacity: 1; } }
  p { font-size: 1.4rem; color: var(--muted); }
</style>
