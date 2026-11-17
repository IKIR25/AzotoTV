<script lang="ts">
  import { t } from '$lib/i18n';
  // A request on its way: which step it is at, one bar from request to library,
  // speed and time left. Data from `dlProgress` (downloads.rs).
  import type { DlProgress } from './seerr';

  let { p }: { p: DlProgress } = $props();

  const LABEL: Record<DlProgress['stage'], string> = {
    none: '',
    unreleased: t('Not released yet'),
    search: t('Looking for a version'),
    wait: t('Waiting'),
    cloud: t('TorBox is fetching it'),
    download: t('Downloading it to the server'),
    unpack: t('Extracting the files'),
    import: t('Putting it in the library'),
    error: t('There\'s a problem'),
  };
  const STEPS = [t('Search|step'), 'TorBox', 'Download', t('Library')];
  const step = $derived(
    p.stage === 'cloud' ? 1
    : p.stage === 'download' || p.stage === 'unpack' ? 2
    : p.stage === 'import' ? 3
    : p.stage === 'error' ? (p.overall >= 0.97 ? 3 : p.overall >= 0.3 ? 2 : 0)
    : 0,
  );
  const pct = $derived(Math.round(p.overall * 100));

  function speed(b: number) {
    const mb = b / 1_000_000;
    return mb >= 1 ? `${mb.toFixed(1).replace('.', ',')} MB/s` : `${Math.round(b / 1000)} kB/s`;
  }
  function eta(s: number) {
    if (s < 60) return t('less than a minute');
    const m = Math.round(s / 60);
    if (m < 60) return t('about {m} min', { m });
    const h = Math.floor(m / 60);
    return m % 60 ? t('about {h} h {m} min', { h, m: m % 60 }) : t('about {h} h', { h });
  }
  const facts = $derived(
    [
      p.episodes ? t('{a} of {b} episodes', { a: p.episodes[0], b: p.episodes[1] }) : '',
      p.speed ? speed(p.speed) : '',
      p.eta != null && (p.stage === 'cloud' || p.stage === 'download') ? eta(p.eta) : '',
    ].filter(Boolean),
  );
</script>

<div class="dl" class:bad={p.stage === 'error'}>
  <div class="head">
    <b>{LABEL[p.stage]}</b>
    {#if p.stage !== 'unreleased'}<span>{pct}%</span>{/if}
  </div>
  <i class="bar"><b style="width:{Math.max(pct, 1)}%"></b></i>
  {#if facts.length}<div class="facts">{facts.join(' · ')}</div>{/if}
  {#if p.note}<div class="note">{p.note}</div>{/if}
  <ol class="steps">
    {#each STEPS as s, i (s)}
      <li class:done={i < step} class:now={i === step}>{s}</li>
    {/each}
  </ol>
</div>

<style>
  .dl {
    margin-top: 2.2rem; width: 62rem; padding: 1.6rem 2rem; border-radius: 1.4rem;
    background: rgba(10, 12, 24, 0.55); border: 1px solid var(--glass-edge);
  }
  .head { display: flex; justify-content: space-between; align-items: baseline; font-size: 1.7rem; }
  .head span { font-variant-numeric: tabular-nums; color: var(--muted); }
  .bar { display: block; height: 0.8rem; margin-top: 1rem; border-radius: 0.4rem; background: rgba(255, 255, 255, 0.14); overflow: hidden; }
  /* width is laid out, not composited: safe from the WebKitGTK transition glitch */
  .bar b { display: block; height: 100%; border-radius: 0.4rem; background: var(--accent); transition: width 1.2s linear; }
  .bad .bar b { background: #e0a040; }
  .facts { margin-top: 0.9rem; font-size: 1.35rem; color: var(--muted); font-variant-numeric: tabular-nums; }
  .note { margin-top: 0.7rem; font-size: 1.35rem; line-height: 1.4; }
  .bad .note { color: #f0c070; }
  .steps { display: flex; gap: 2.4rem; margin-top: 1.2rem; padding: 0; list-style: none; font-size: 1.3rem; color: rgba(244, 245, 250, 0.4); }
  .steps li::before { content: '○ '; }
  .steps li.done { color: var(--muted); }
  .steps li.done::before { content: '✓ '; }
  .steps li.now { color: #fff; font-weight: 600; }
  .steps li.now::before { content: '● '; }
</style>
