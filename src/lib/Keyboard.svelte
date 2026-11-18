<script lang="ts">
  import { t } from '$lib/i18n';
  import { onMount } from 'svelte';
  import { input, key } from './input.svelte';
  import { backspace, closeKeyboard, cycleShift, osk, toggleMode, type } from './osk.svelte';

  type Act = 'shift' | 'bksp' | 'mode' | 'space' | 'done';
  type Key = { ch: string; w?: number } | { act: Act; w: number };

  // Both layouts have the same shape, so switching keeps focus on the same spot.
  const chars = (s: string): Key[] => [...s].map((ch) => ({ ch }));
  const bottom = (mode: Key): Key[] => [mode, { ch: '-' }, { act: 'space', w: 4 }, { ch: '.' }, { act: 'done', w: 2.5 }];
  const LAYOUTS: Record<'abc' | 'sym', Key[][]> = {
    abc: [
      chars('1234567890'),
      chars('qwertyuiop'),
      chars("asdfghjkl'"),
      [{ act: 'shift', w: 1.5 }, ...chars('zxcvbnm'), { act: 'bksp', w: 1.5 }],
      bottom({ act: 'mode', w: 1.5 }),
    ],
    sym: [
      chars('1234567890'),
      chars('àèéìòù!?@#'),
      chars('&*():;/_+='),
      [{ act: 'shift', w: 1.5 }, ...chars('"%€$,<>'), { act: 'bksp', w: 1.5 }],
      bottom({ act: 'mode', w: 1.5 }),
    ],
  };
  const rows = $derived(LAYOUTS[osk.mode]);

  const upper = $derived(osk.shift !== 'off');
  function label(k: Key) {
    if ('ch' in k) return upper ? k.ch.toUpperCase() : k.ch;
    return {
      shift: osk.shift === 'lock' ? '⇪' : '⇧',
      bksp: '⌫',
      mode: osk.mode === 'abc' ? '&123' : 'ABC',
      space: 'spazio',
      done: t('Done'),
    }[k.act];
  }
  function press(k: Key) {
    if ('ch' in k) return type(k.ch);
    if (k.act === 'shift') cycleShift();
    else if (k.act === 'bksp') backspace();
    else if (k.act === 'mode') toggleMode();
    else if (k.act === 'space') type(' ');
    else closeKeyboard(true);
  }

  const field = osk.target!;
  const name =
    field.closest('label')?.firstChild?.textContent?.trim() || field.placeholder || field.getAttribute('aria-label') || '';
  const secret = field.type === 'password';
  // `data-osk="side"` on the input docks the keyboard on the left, so the page
  // can show live results on the right (search).
  const side = field.dataset.osk === 'side';
  const shown = $derived(secret ? '•'.repeat(osk.value.length) : osk.value);

  let box: HTMLElement;
  // the caret blinks from a timer, not a CSS animation: WebKit runs those at
  // full frame rate, ~10% of a CPU core just for a blinking line
  let caretOn = $state(true);
  onMount(() => {
    const blink = setInterval(() => (caretOn = !caretOn), 500);
    // start on the first letter
    box.querySelectorAll<HTMLElement>('.key')[10]?.focus({ preventScroll: true });

    // A physical keyboard still types straight into the field.
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') closeKeyboard();
      else if (e.key === 'Backspace') backspace();
      else if (e.key.length === 1 && !e.ctrlKey && !e.altKey && !e.metaKey) type(e.key);
      else return; // arrows / Enter keep their usual meaning
      e.preventDefault();
      e.stopImmediatePropagation();
    };
    window.addEventListener('keydown', onKey, true);
    return () => {
      clearInterval(blink);
      window.removeEventListener('keydown', onKey, true);
    };
  });
</script>

<div class="osk glass" class:side data-nav-trap bind:this={box}>
  <div class="field">
    {#if name}<span class="name">{name}</span>{/if}
    <div class="val">{shown.slice(0, osk.caret)}<i class="caret" class:off={!caretOn}></i>{shown.slice(osk.caret)}</div>
  </div>

  {#each rows as row}
    <div class="krow">
      {#each row as k}
        <button
          class="key"
          class:act={'act' in k}
          class:on={'act' in k && k.act === 'shift' && osk.shift !== 'off'}
          data-focusable
          style="--kw:{k.w ?? 1}"
          onclick={() => press(k)}>{label(k)}</button
        >
      {/each}
    </div>
  {/each}

  <div class="legend">
    {#if side}<span><kbd>▶</kbd> {t('to the results')}</span>{/if}
    {#if input.remote}
      <span><kbd>OK</kbd> {t('type')}</span>
      <span><kbd>{t('Back')}</kbd> {t('erase')}</span>
      <span>{t('space, caps and Done are keys up here')}</span>
    {:else}
      <span><kbd data-btn="A">{key('A')}</kbd> {t('type')}</span>
      <span><kbd data-btn="B">{key('B')}</kbd> {t('erase')}</span>
      <span><kbd data-btn="Y">{key('Y')}</kbd> {t('space')}</span>
      <span><kbd data-btn="X">{key('X')}</kbd> {t('caps')}</span>
      <span><kbd data-btn="LB">{key('LB')}</kbd><kbd data-btn="RB">{key('RB')}</kbd> {t('cursor')}</span>
      <span><kbd data-btn="START">{key('START')}</kbd> {t('done')}</span>
    {/if}
  </div>
</div>

<style>
  .osk {
    --u: 5.4rem; --g: 0.6rem;
    position: absolute; z-index: 25; left: 50%; bottom: 3rem; transform: translateX(-50%);
    padding: 2rem 2.2rem 1.4rem; border-radius: 2.4rem; display: grid; gap: var(--g);
    background: rgb(var(--panel-rgb) / 0.95);
  }
  .osk.side { --u: 4.6rem; left: 4rem; top: 10rem; bottom: auto; transform: none; }
  .side .key { height: 4.4rem; }
  .side .legend { max-width: 50rem; justify-self: center; }
  .field {
    margin-bottom: 0.8rem; padding: 0.9rem 1.4rem; border-radius: 1.2rem;
    background: rgb(var(--fg-rgb) / 0.08); border: 0.2rem solid rgb(var(--fg-rgb) / 0.5);
  }
  .name { display: block; font-size: 1.1rem; color: var(--muted); margin-bottom: 0.2rem; }
  .val { font-size: 1.9rem; min-height: 2.6rem; white-space: pre; overflow: hidden; }
  .caret {
    display: inline-block; width: 0.18rem; height: 2.2rem; margin: 0 0.05rem; vertical-align: -0.35rem;
    background: var(--accent);
  }
  .caret.off { visibility: hidden; }
  .krow { display: flex; gap: var(--g); }
  .key {
    width: calc(var(--u) * var(--kw) + var(--g) * (var(--kw) - 1)); height: 4.8rem;
    border-radius: 1rem; font-size: 1.8rem; background: rgb(var(--fg-rgb) / 0.1);
  }
  .key.act { font-size: 1.4rem; font-weight: 600; background: rgb(var(--fg-rgb) / 0.18); }
  .key.on { background: var(--accent); }
  /* no transitions here: plain state changes only (see tween.ts) */
  .key:focus { background: var(--focus-bg); color: var(--focus-text); }
  .legend {
    display: flex; flex-wrap: wrap; justify-content: center; gap: 0.4rem 1.6rem;
    margin-top: 0.8rem; font-size: 1.1rem; color: var(--muted);
  }
  kbd {
    font: inherit; font-size: 0.95rem; font-weight: 700; padding: 0.05rem 0.45rem; margin-right: 0.3rem;
    border: 1px solid var(--glass-edge); border-radius: 0.4rem; color: var(--text);
  }
</style>
