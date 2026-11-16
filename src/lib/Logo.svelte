<script lang="ts">
  /** AZOTO in red, TV in the accent (azoto is nitrogen: the app began on an Acer Nitro laptop). `size` in rem. */
  let { size = 1.5 }: { size?: number } = $props();
</script>

<span class="logo" style="font-size: {size}rem"><span class="azoto">AZOTO</span><b>TV</b></span>

<style>
  .logo { font-weight: 300; letter-spacing: 0.28em; white-space: nowrap; }
  .azoto { color: var(--azoto); font-weight: 500; }
  b { font-weight: 800; color: var(--accent); }
</style>
