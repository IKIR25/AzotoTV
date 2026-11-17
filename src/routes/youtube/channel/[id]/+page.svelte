<script lang="ts">
  import { t } from '$lib/i18n';
  import { tick } from 'svelte';
  import { page } from '$app/state';
  import { refresh, stage } from '$lib/stage.svelte';
  import { restoreFocus } from '$lib/nav';
  import Card from '$lib/Card.svelte';
  import PinButton from '$lib/PinButton.svelte';
  import { channelPin, isSubscribed, toggleSub, videoTile, yt, ytImage, type ChannelPage } from '$lib/youtube.svelte';

  // A channel: banner, avatar, Iscriviti, and its latest videos (no Shorts).
  const id = $derived(page.params.id ?? '');
  let ch = $state<ChannelPage | null>(null);
  let failed = $state(false);

  $effect(() => {
    const cur = id;
    void refresh.n;
    ch = null;
    failed = false;
    stage.backdrop = null;
    yt.channel(cur).then(
      async (c) => {
        if (cur !== id) return;
        ch = c;
        await tick();
        restoreFocus(page.url.pathname, '[data-key="sub"]');
      },
      () => cur === id && (failed = true),
    );
  });
</script>

<div class="scroller">
  {#if ch}
    {#if ch.banner}<div class="banner" style="background-image:url('{ytImage(ch.banner)}')"></div>{/if}
    <header class:nobanner={!ch.banner}>
      {#if ch.channel.avatar}<img class="avatar" src={ytImage(ch.channel.avatar)} alt="" />{/if}
      <div class="who">
        <h1>{ch.channel.name}</h1>
        <div class="meta">
          {#if ch.channel.subs}<span>{ch.channel.subs}</span>{/if}
          <span>{ch.videos.length ? t('Latest videos') : t('No videos')}</span>
        </div>
      </div>
      <button class="btn primary" class:on={isSubscribed(id)} data-focusable data-key="sub" data-scroll-top onclick={() => toggleSub(ch!.channel)}>
        {isSubscribed(id) ? t('✓ Subscribed') : t('＋ Subscribe')}
      </button>
      <PinButton pin={channelPin(ch.channel)} />
    </header>
    <div class="grid">
      {#each ch.videos as v (v.id)}<Card wide size={24} tile={videoTile(v, 'cv')} />{/each}
    </div>
    <div class="end"></div>
  {:else if failed}
    <p class="msg">{t('Channel not available.')}</p>
  {/if}
</div>

<style>
  .banner { height: 15rem; background-size: cover; background-position: center; mask-image: linear-gradient(180deg, #000 55%, transparent); }
  header { display: flex; align-items: center; gap: 2.4rem; padding: 0 6rem 1.4rem; margin-top: -4rem; position: relative; }
  header.nobanner { margin-top: 0; padding-top: 6rem; }
  .avatar { width: 10rem; height: 10rem; border-radius: 50%; object-fit: cover; box-shadow: 0 0.6rem 2rem rgba(0, 0, 0, 0.5); }
  .who { flex: 1; min-width: 0; }
  h1 { font-size: 3.4rem; font-weight: 700; letter-spacing: -0.01em; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .meta { margin-top: 0.4rem; }
  .btn.primary { font-size: 1.4rem; padding: 0.9rem 2rem; }
  .btn.on { background: rgba(255, 255, 255, 0.2); }
  .btn.on:focus { background: #fff; }
  .grid { display: grid; grid-template-columns: repeat(4, 24rem); gap: 2.4rem 1.4rem; padding: 1.4rem 6rem 2rem; }
  .msg { position: absolute; top: 12rem; left: 6rem; font-size: 1.8rem; color: var(--muted); }
  .end { height: 8rem; }
</style>
