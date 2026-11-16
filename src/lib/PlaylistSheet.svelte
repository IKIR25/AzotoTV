<script lang="ts">
  import { key } from './input.svelte';
  import { onMount, tick } from 'svelte';
  import { api, type Item } from './jf';
  import { modal, showToast } from './stage.svelte';
  import { openKeyboard, osk } from './osk.svelte';

  // Side sheet: "which playlist?" — this profile's playlists, and a new one
  // (A on the name field opens the keyboard; "Fatto" creates it). D-pad stays
  // inside (data-nav-trap, given up while the keyboard is open); B closes.
  let {
    title = 'Aggiungi a una playlist',
    onpick,
    onclose,
  }: {
    title?: string;
    /** a playlist of this profile, new or existing */
    onpick: (id: string, name: string) => void;
    onclose: () => void;
  } = $props();

  let lists = $state<Item[] | null>(null);
  let name = $state('');
  let input: HTMLInputElement;
  let box: HTMLElement;

  onMount(() => {
    modal.close = onclose;
    api.playlists().then(
      async (p) => {
        lists = p;
        await tick();
        // straight to typing a name when there's nothing to pick
        if (!p.length) {
          input.focus({ preventScroll: true });
          openKeyboard(input);
        } else box.querySelector<HTMLElement>('.opt')?.focus({ preventScroll: true });
      },
      () => (lists = []),
    );
    input.focus({ preventScroll: true });
    return () => {
      if (modal.close === onclose) modal.close = null;
    };
  });

  async function create(e: SubmitEvent) {
    e.preventDefault();
    const n = name.trim();
    if (!n) return;
    try {
      const id = await api.createPlaylist(n);
      onpick(id, n);
    } catch (err) {
      showToast(`Non riuscito: ${err}`, 4000);
    }
  }
</script>

<aside class="sheet glass" data-nav-trap={osk.target ? undefined : true} bind:this={box}>
  <h2>{title}</h2>
  <div class="list scroller">
    <form onsubmit={create}>
      <input
        bind:this={input}
        bind:value={name}
        data-focusable
        data-osk-submit
        placeholder="＋ Nuova playlist…"
        aria-label="Nome della nuova playlist"
        autocomplete="off"
      />
    </form>
    {#each lists ?? [] as p (p.Id)}
      <button class="opt" data-focusable onclick={() => onpick(p.Id, p.Name)}>
        <span class="txt">{p.Name}{#if p.ChildCount}<small>{p.ChildCount} brani</small>{/if}</span>
      </button>
    {/each}
  </div>
  <div class="hint"><kbd>{key('A')}</kbd> scegli · <kbd>{key('B')}</kbd> chiudi</div>
</aside>

<style>
  .sheet {
    position: absolute; z-index: 22; top: 0; right: 0; bottom: 0; width: 48rem;
    border-radius: 2.4rem 0 0 2.4rem; background: rgba(16, 18, 32, 0.94);
  }
  h2 { position: absolute; top: 4rem; left: 3.4rem; font-size: 2.4rem; font-weight: 700; }
  .list { top: 9.5rem; bottom: 5rem; padding: 0.6rem 2rem 2rem; }
  input {
    width: 100%; font: inherit; font-size: 1.6rem; color: var(--text); padding: 1.1rem 1.4rem; margin-bottom: 1rem;
    border-radius: 1.2rem; border: 0.2rem solid transparent; background: rgba(255, 255, 255, 0.1); outline: none;
  }
  input:focus { border-color: #fff; background: rgba(255, 255, 255, 0.16); }
  input::placeholder { color: var(--muted); }
  .opt { display: flex; width: 100%; text-align: left; padding: 1.1rem 1.4rem; border-radius: 1.2rem; margin-bottom: 0.4rem; }
  .txt { font-size: 1.6rem; font-weight: 600; }
  .txt small { display: block; font-size: 1.15rem; font-weight: 400; color: var(--muted); margin-top: 0.15rem; }
  .opt:focus { background: #fff; color: #10121c; }
  .opt:focus small { color: rgba(16, 18, 28, 0.65); }
  .hint { position: absolute; bottom: 1.8rem; left: 3.4rem; font-size: 1.15rem; color: var(--muted); }
  kbd {
    font: inherit; font-size: 0.95rem; font-weight: 700; padding: 0.05rem 0.45rem;
    border: 1px solid var(--glass-edge); border-radius: 0.4rem; color: var(--text);
  }
</style>
