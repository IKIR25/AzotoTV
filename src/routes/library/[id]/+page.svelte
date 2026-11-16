<script lang="ts">
  import { page } from '$app/state';
  import Library from '$lib/Library.svelte';
</script>

<Library id={page.params.id ?? ''} />
