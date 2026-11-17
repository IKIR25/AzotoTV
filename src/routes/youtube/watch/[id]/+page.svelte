<script lang="ts">
  import { t } from '$lib/i18n';
  import { tick } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { player } from '$lib/player.svelte';
  import { stage } from '$lib/stage.svelte';
  import { restoreFocus } from '$lib/nav';
  import Card from '$lib/Card.svelte';
  import Row from '$lib/Row.svelte';
  import PinButton from '$lib/PinButton.svelte';
  import {
    duration, isLater, isLiked, isNope, isSubscribed, known, resumeAt, thumb, toggleLater, toggleLike, toggleNope, toggleSub, videoPin, watch,
    yt, ytImage, videoTile, type Comment, type Video, type VideoPage,
  } from '$lib/youtube.svelte';

  // A video's page: its picture behind, title, channel, what it's about, and
  // Riproduci / Iscriviti / Mi piace / Guarda più tardi / Non mi interessa; related videos and
  // the comments (most popular first, a page at a time) below.
  const id = $derived(page.params.id ?? '');
  let info = $state<VideoPage | null>(null);
  let failed = $state('');
  const v = $derived<Video | null>(info?.video ?? known.get(id) ?? null);
  // the list's copy knows the length, the page doesn't
  const length = $derived(known.get(id)?.length || 0);
  const resume = $derived(v && !player.active ? resumeAt(v.id) : 0);
  let expanded = $state(false);
  let comments = $state<Comment[]>([]);
  let count = $state('');
  let more = $state<string | null>(null);
  let loadingMore = $state(false);
  let open = $state<Record<string, boolean>>({});

  $effect(() => {
    const cur = id;
    info = null;
    failed = '';
    expanded = false;
    comments = [];
    count = '';
    more = null;
    open = {};
    yt.video(cur).then(
      async (p) => {
        if (cur !== id) return;
        info = p;
        await tick();
        restoreFocus(page.url.pathname, '[data-key="play"]');
        if (p.comments) loadComments(cur, p.comments);
      },
      (e) => cur === id && (failed = String(e)),
    );
    // after the layout's own navigation effect, which clears the backdrop
    tick().then(() => {
      if (cur !== id) return;
      stage.backdrop = thumb(cur, 'big');
      document.querySelector<HTMLElement>('[data-key="play"]')?.focus({ preventScroll: true });
    });
  });

  const fmt = (s: number) => duration(Math.round(s));

  async function loadComments(cur: string, token: string) {
    loadingMore = true;
    try {
      const c = await yt.comments(token);
      if (cur !== id) return;
      comments = [...comments, ...c.comments.filter((x) => !comments.some((y) => y.id === x.id))];
      count ||= c.count;
      more = c.next;
    } catch {
      if (cur === id) more = null;
    } finally {
      loadingMore = false;
    }
  }

  async function loadMore() {
    if (!more || loadingMore) return;
    const first = comments.length;
    await loadComments(id, more);
    await tick();
    // carry on from the first new comment
    document.querySelector<HTMLElement>(`[data-key="cm-${comments[first]?.id}"]`)?.focus();
  }
</script>

<div class="shade" aria-hidden="true"></div>
<div class="scroller">
  {#if v}
    <section class="info">
      {#if info}
        <button class="owner" data-focusable data-key="channel" data-scroll-top onclick={() => goto(`/youtube/channel/${info!.owner.id}`)}>
          {#if info.owner.avatar}<img src={ytImage(info.owner.avatar)} alt="" />{/if}
          <span>{info.owner.name}<small>{info.owner.subs}</small></span>
        </button>
      {:else}
        <div class="owner static"><span>{v.channel}</span></div>
      {/if}
      <h1>{v.title}</h1>
      <div class="meta">
        {#if v.views}<span>{v.views}</span>{/if}
        {#if info?.date}<span>{info.date}</span>{:else if v.published}<span>{v.published}</span>{/if}
        {#if length}<span>{duration(length)}</span>{/if}
        {#if v.live}<span class="pill">{t('Live')}</span>{/if}
      </div>
      <div class="actions">
        <button class="btn primary" data-focusable data-key="play" data-scroll-top onclick={() => watch(v!)}>
          ▶ {resume ? t('Resume from {fmt}', { fmt: fmt(resume) }) : t('Play')}
        </button>
        {#if resume}
          <button class="btn icon" data-focusable data-key="restart" data-scroll-top onclick={() => watch(v!, { fromStart: true })}>
            {t('↺ From the start')}
          </button>
        {/if}
        {#if info}
          <button class="btn icon" class:on={isSubscribed(info.owner.id)} data-focusable data-key="sub" data-scroll-top onclick={() => toggleSub(info!.owner)}>
            {isSubscribed(info.owner.id) ? t('✓ Subscribed') : t('＋ Subscribe')}
          </button>
        {/if}
        <button class="btn icon" class:on={isLiked(v.id)} data-focusable data-key="like" data-scroll-top onclick={() => toggleLike({ ...v!, length })}>
          {isLiked(v.id) ? t('♥ Liked') : t('♡ Like')}
        </button>
        <button class="btn icon" class:on={isLater(v.id)} data-focusable data-key="later" data-scroll-top onclick={() => toggleLater({ ...v!, length })}>
          {isLater(v.id) ? t('✓ Later') : t('◷ Watch later')}
        </button>
        <button
          class="btn icon"
          class:on={isNope(v.id)}
          data-focusable
          data-key="nope"
          data-scroll-top
          onclick={() => toggleNope({ ...v!, length, channel: info?.owner.name || v!.channel, channel_id: info?.owner.id || v!.channel_id })}
        >
          {isNope(v.id) ? t('✓ Not interested') : t('⊘ Not interested')}
        </button>
        <PinButton pin={videoPin({ ...v, channel: info?.owner.name || v.channel })} />
      </div>
      {#if info?.description}
        <button class="desc" class:expanded data-focusable data-key="desc" onclick={() => (expanded = !expanded)}
          ><span>{info.description}</span></button
        >
      {/if}
      {#if failed}<p class="msg">{t('YouTube isn\'t answering: {failed}', { failed })}</p>{/if}
    </section>
    {#if info?.related.length}
      <Row title={t('More videos')}>
        {#each info.related as r (r.id)}<Card wide size={22} tile={videoTile(r, 'rel')} />{/each}
      </Row>
    {/if}
    {#if comments.length}
      <h2>{t('Comments')}{#if count}<span class="n">{count}</span>{/if}</h2>
      <div class="comments">
        {#each comments as c (c.id)}
          <button class="cm glass" class:open={open[c.id]} data-focusable data-key="cm-{c.id}" onclick={() => (open[c.id] = !open[c.id])}>
            {#if c.avatar}<img src={ytImage(c.avatar)} alt="" loading="lazy" />{:else}<span class="noav"></span>{/if}
            <span class="body">
              <span class="who">{c.author}{#if c.creator}<span class="tag">{t('Author')}</span>{/if}<small>{c.published}</small></span>
              <span class="txt">{c.text}</span>
              {#if c.likes || c.replies}
                <span class="stats">
                  {#if c.likes}<span>👍 {c.likes}</span>{/if}
                  {#if c.replies}<span>{c.replies === '1' ? t('1 reply') : `${c.replies} risposte`}</span>{/if}
                </span>
              {/if}
            </span>
          </button>
        {/each}
        {#if more}
          <button class="btn more" data-focusable data-key="more-comments" onclick={loadMore}>
            {loadingMore ? t('Loading…') : t('More comments')}
          </button>
        {/if}
      </div>
    {/if}
    <div class="end"></div>
  {:else if failed}
    <p class="msg top">{t('Video not available.')}</p>
  {/if}
</div>

<style>
  .shade {
    position: fixed; inset: 0; pointer-events: none;
    background: linear-gradient(90deg, rgba(7, 8, 15, 0.82) 0%, rgba(7, 8, 15, 0.6) 50%, rgba(7, 8, 15, 0.2) 85%),
      linear-gradient(0deg, rgba(7, 8, 15, 0.85) 0%, transparent 45%);
  }
  .info { position: relative; padding: 14rem 6rem 2.4rem; width: 92rem; min-height: 50rem; }
  .owner { display: inline-flex; align-items: center; gap: 1rem; padding: 0.4rem 1.2rem 0.4rem 0.4rem; border-radius: 3rem; text-align: left; }
  .owner.static { padding-left: 0; }
  .owner:focus { background: #fff; color: #10121c; }
  .owner img { width: 3.6rem; height: 3.6rem; border-radius: 50%; object-fit: cover; }
  .owner span { font-size: 1.5rem; font-weight: 600; }
  .owner small { display: block; font-size: 1.1rem; font-weight: 400; opacity: 0.7; }
  h1 {
    margin-top: 1rem; font-size: 3.4rem; font-weight: 700; letter-spacing: -0.01em; line-height: 1.15;
    display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden;
  }
  .meta { margin-top: 1.2rem; }
  /* wider than the text, up to the screen's margin: seven buttons on one line */
  .actions { display: flex; flex-wrap: wrap; gap: 1rem; margin-top: 2rem; width: max-content; max-width: 108rem; }
  .btn.on { background: rgba(255, 255, 255, 0.24); }
  .btn.on:focus { background: #fff; }
  /* A shows it whole */
  .desc {
    display: block; margin-top: 2rem; padding: 1rem 1.4rem; border-radius: 1.2rem; text-align: left;
    font-size: 1.35rem; line-height: 1.5; color: rgba(244, 245, 250, 0.82); max-width: 80rem; background: rgba(255, 255, 255, 0.06);
  }
  .desc span {
    display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; white-space: pre-line;
  }
  .desc.expanded span { display: block; }
  .desc:focus { background: rgba(255, 255, 255, 0.18); box-shadow: 0 0 0 0.25rem #fff; }
  h2 { position: relative; font-size: 1.9rem; font-weight: 600; margin: 1rem 6rem 1rem; }
  h2 .n { margin-left: 1rem; font-size: 1.4rem; font-weight: 400; color: var(--muted); }
  .comments { position: relative; display: grid; gap: 1rem; padding: 0 6rem; max-width: 100rem; }
  .cm { display: flex; gap: 1.4rem; padding: 1.2rem 1.4rem; border-radius: 1.4rem; text-align: left; transition: background 0.15s; }
  .cm:focus { background: rgba(255, 255, 255, 0.2); box-shadow: 0 0 0 0.25rem #fff; }
  .cm img, .noav { flex: 0 0 3.6rem; width: 3.6rem; height: 3.6rem; border-radius: 50%; object-fit: cover; background: rgba(255, 255, 255, 0.12); }
  .body { display: grid; gap: 0.4rem; min-width: 0; }
  .who { font-size: 1.25rem; font-weight: 600; }
  .who small { margin-left: 0.8rem; font-weight: 400; color: var(--muted); font-size: 1.1rem; }
  .tag { margin-left: 0.6rem; padding: 0.1rem 0.6rem; border-radius: 0.5rem; font-size: 1rem; background: rgba(255, 255, 255, 0.2); }
  /* A on a comment shows it whole */
  .txt {
    font-size: 1.4rem; line-height: 1.45; white-space: pre-line; color: rgba(244, 245, 250, 0.9);
    display: -webkit-box; -webkit-line-clamp: 4; line-clamp: 4; -webkit-box-orient: vertical; overflow: hidden;
  }
  .cm.open .txt { display: block; }
  .stats { display: flex; gap: 1.6rem; font-size: 1.1rem; color: var(--muted); }
  .more { justify-self: start; margin-top: 0.6rem; font-size: 1.3rem; }
  .msg { margin-top: 1.4rem; font-size: 1.4rem; color: var(--muted); }
  .msg.top { position: absolute; top: 12rem; left: 6rem; font-size: 1.8rem; }
  .end { height: 6rem; }
</style>
