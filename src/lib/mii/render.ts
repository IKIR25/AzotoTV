// Draws Miis the way the Wii U does: FFL.js (Nintendo's FFL compiled to
// WebAssembly, $lib/vendor/ffl) with the Wii U's own models and textures
// (FFLResHigh.dat, served by mii.rs) and miijs's bodies. One FFL instance and
// one WebGL context for the whole app; pictures are cached per Mii.
//
// FFL.js (AGPL) and the bodies are not in the repo: tools/fetch-mii-renderer.sh
// puts them in place before a build. Without them the app still builds, and
// the Mii app shows its Miis without drawing them (canDraw() is false).
import * as THREE from 'three';
import { GLTFLoader, type GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js';
import * as SkeletonUtils from 'three/examples/jsm/utils/SkeletonUtils.js';
import { invoke } from '@tauri-apps/api/core';
import { decode, miijs, unb64 } from './miijs';

// whatever of FFL.js is there at build time (nothing: {})
const lib = import.meta.glob(
  '../vendor/ffl/{ffl,materials/FFLShaderMaterial,helpers/BodyUtilities,helpers/SkeletonScalingExtensions,helpers/ModelScaleDesc}.js',
  { eager: true },
) as Record<string, any>;
const { FFL, CharModel, pantsColors } = lib['../vendor/ffl/ffl.js'] ?? {};
const FFLShaderMaterial = lib['../vendor/ffl/materials/FFLShaderMaterial.js']?.default;
const Body = lib['../vendor/ffl/helpers/BodyUtilities.js'];
const { addSkeletonScalingExtensions } = lib['../vendor/ffl/helpers/SkeletonScalingExtensions.js'] ?? {};
const { detectModelDesc } = lib['../vendor/ffl/helpers/ModelScaleDesc.js'] ?? {};

/** Switch-only glasses (types 9+) drawn as the closest Wii U ones (miijs's table). */
const NX_GLASSES = [0, 8, 3, 4, 1, 5, 14, 13, 11];

type Engine = { ffl: any; renderer: THREE.WebGLRenderer; bodies: (GLTF | null)[] };
let engine: Promise<Engine> | null = null;

function loadScript(src: string) {
  return new Promise<void>((ok, fail) => {
    if ((globalThis as any).ModuleFFL) return ok();
    const s = document.createElement('script');
    s.src = src;
    s.onload = () => ok();
    s.onerror = () => fail(new Error(`${src} didn't load`));
    document.head.append(s);
  });
}

async function start(): Promise<Engine> {
  if (!FFL || !FFLShaderMaterial || !Body || !addSkeletonScalingExtensions || !detectModelDesc) {
    throw new Error('FFL.js is not in this build (tools/fetch-mii-renderer.sh)');
  }
  const [, res, male, female] = await Promise.all([
    loadScript('/mii/ffl-emscripten.js'),
    invoke<ArrayBuffer>('mii_ffl_resource'),
    new GLTFLoader().loadAsync('/mii/miiMaleBody.glb').catch(() => null),
    new GLTFLoader().loadAsync('/mii/miiFemaleBody.glb').catch(() => null),
  ]);
  const ffl = await FFL.initWithResource(new Uint8Array(res), (globalThis as any).ModuleFFL);
  addSkeletonScalingExtensions(THREE.Skeleton);
  const renderer = new THREE.WebGLRenderer({ alpha: true, antialias: true, preserveDrawingBuffer: true });
  ffl.setRenderer(renderer);
  return { ffl, renderer, bodies: [male, female] };
}

function getEngine() {
  engine ??= start().catch((e) => {
    engine = null; // try again next time (the file may be put in place later)
    throw e;
  });
  return engine;
}

/** What FFL reads: miijs's Studio encoding, with Switch-only glasses mapped. */
async function studioData(nx: string): Promise<Uint8Array> {
  const look = (await decode(unb64(nx))).toJSON();
  const g = look.glasses?.type;
  if (Number.isInteger(g) && g > 8) look.glasses.type = NX_GLASSES[g - 9] ?? 0;
  return new Uint8Array(miijs.encodeMii(look, miijs.MiiFormats.MNMS));
}

function bodyFor(e: Engine, gender: number) {
  const gltf = e.bodies[gender] ?? e.bodies[0];
  if (!gltf) return null;
  const model = SkeletonUtils.clone(gltf.scene);
  const mixer = new THREE.AnimationMixer(model);
  const clip = gltf.animations.find((a) => a.name === 'Wait') ?? gltf.animations[0];
  if (clip) mixer.clipAction(clip).play();
  mixer.update(0);
  return { model, mixer, animations: gltf.animations, scaleDesc: detectModelDesc(model) };
}

export type Shot = { body?: boolean; size?: number };

async function draw(nx: string, { body = false, size = 512 }: Shot): Promise<Blob> {
  const e = await getEngine();
  const { renderer, ffl } = e;
  const scene = new THREE.Scene();
  const head = new CharModel(ffl, await studioData(nx), 0, FFLShaderMaterial, renderer);
  head.meshes.traverse((m: THREE.Object3D) => (m.frustumCulled = false));
  const b = bodyFor(e, head.charInfo.gender);
  try {
    if (b) {
      Body.prepareBodyForCharModel(
        b,
        FFLShaderMaterial,
        head.favoriteColor,
        head.getBodyScale(),
        pantsColors[0].clone().convertLinearToSRGB(),
      );
      Body.attachHeadToBody(b, head.meshes);
      scene.add(b.model);
    } else scene.add(head.meshes);
    let camera: THREE.PerspectiveCamera;
    if (body && b) camera = Body.getWholeBodyCamera(1, head.charInfo.height);
    else {
      camera = Body.getFaceCamera();
      if (b) Body.adjustCameraForBodyHead(camera, b);
    }
    // sized here, after the awaits: a live Mii draws in between (its own size)
    renderer.setSize(size, size, false);
    renderer.setClearColor(0x000000, 0);
    renderer.render(scene, camera);
    // copied off right away (same task): the canvas may be on screen showing
    // a live Mii, which is drawn back before the browser shows a frame
    const snap = document.createElement('canvas');
    snap.width = snap.height = size;
    snap.getContext('2d')!.drawImage(renderer.domElement, 0, 0, size, size);
    view?.redraw();
    return await new Promise<Blob>((ok, fail) =>
      snap.toBlob((blob) => (blob ? ok(blob) : fail(new Error('no picture'))), 'image/png'),
    );
  } finally {
    if (b) Body.disposeModel(b.model);
    head.dispose();
  }
}

const cache = new Map<string, Promise<string>>();
let queue: Promise<unknown> = Promise.resolve();

/** A picture of the Mii (blob URL, transparent background), drawn one at a time. */
export function picture(nx: string, shot: Shot = {}): Promise<string> {
  const key = `${shot.body ? 'b' : 'f'}${shot.size ?? 512}:${nx}`;
  let p = cache.get(key);
  if (!p) {
    p = queue.then(() => draw(nx, shot)).then((blob) => URL.createObjectURL(blob));
    queue = p.catch(() => {});
    p.catch(() => cache.delete(key));
    cache.set(key, p);
  }
  return p;
}

/** Whether Miis can be drawn here (FFLResHigh.dat in place, WebGL working). */
export async function canDraw(): Promise<boolean> {
  try {
    await getEngine();
    return true;
  } catch (e) {
    console.warn('[mii] renderer:', e);
    return false;
  }
}

// ---- the live Mii (its page): turned with the right stick ---------------------------

/**
 * A Mii drawn live in `box` (it borrows the shared canvas): it plays its idle
 * animation for a while, then holds still, so an open page doesn't keep the
 * GPU busy (see idle.svelte.ts). `turn(speed)` turns it (-1..1, 0 = stop);
 * `spin(deg)` turns it by a step.
 */
export type LiveMii = {
  turn(speed: number): void;
  spin(deg: number): void;
  /** turned by this much at once (a finger dragging it) */
  rotate(deg: number): void;
  /** the Mii changed (the editor): drawn anew, still turned the same way */
  update(nx: string): Promise<void>;
  stop(): void;
};

let view: { redraw(): void } | null = null;

/** seconds of animation after the last turn */
const AWAKE = 6;
const TURN_SPEED = Math.PI; // rad/s at full tilt

type Model = { root: THREE.Object3D; head: any; body: ReturnType<typeof bodyFor>; height: number };

async function buildModel(e: Engine, nx: string): Promise<Model> {
  const head = new CharModel(e.ffl, await studioData(nx), 0, FFLShaderMaterial, e.renderer);
  head.meshes.traverse((m: THREE.Object3D) => (m.frustumCulled = false));
  const b = bodyFor(e, head.charInfo.gender);
  let root: THREE.Object3D = head.meshes;
  if (b) {
    Body.prepareBodyForCharModel(b, FFLShaderMaterial, head.favoriteColor, head.getBodyScale(), pantsColors[0].clone().convertLinearToSRGB());
    Body.attachHeadToBody(b, head.meshes);
    root = b.model;
  }
  return { root, head, body: b, height: head.charInfo.height };
}

function disposeModel(m: Model) {
  if (m.body) Body.disposeModel(m.body.model);
  m.head.dispose();
}

/** `tallest`: framed for the tallest Mii (the editor shows height changes), else for this one. */
export async function liveMii(nx: string, box: HTMLElement, { tallest = false } = {}): Promise<LiveMii> {
  const e = await getEngine();
  const { renderer } = e;
  let model = await buildModel(e, nx);
  const scene = new THREE.Scene();
  scene.add(model.root);

  const canvas = renderer.domElement;
  canvas.style.width = canvas.style.height = '100%';
  canvas.style.display = 'block';
  box.append(canvas);
  const w = box.clientWidth || 600;
  const h = box.clientHeight || 600;
  const px = Math.min(2, devicePixelRatio || 1);
  const camera = model.body ? Body.getWholeBodyCamera(w / h, tallest ? 127 : model.height) : Body.getFaceCamera();

  let angle = 0;
  let target: number | null = null; // a step in progress (spin)
  let speed = 0;
  let awakeUntil = performance.now() / 1000 + AWAKE;
  let raf = 0;
  let last = 0;
  let gone = false;

  function render() {
    renderer.setSize(Math.round(w * px), Math.round(h * px), false);
    renderer.setClearColor(0x000000, 0);
    model.root.rotation.y = angle;
    renderer.render(scene, camera);
  }
  function frame(ms: number) {
    raf = 0;
    if (gone) return;
    const t = ms / 1000;
    const dt = last ? Math.min(0.1, t - last) : 0;
    last = t;
    if (speed) {
      angle += speed * TURN_SPEED * dt;
      target = null;
    } else if (target !== null) {
      const d = target - angle;
      angle += Math.abs(d) < 0.01 ? d : d * Math.min(1, dt * 8);
      if (angle === target) target = null;
    }
    model.body?.mixer.update(dt);
    render();
    if (speed || target !== null || t < awakeUntil) raf = requestAnimationFrame(frame);
    else last = 0; // still: no more frames until the next turn
  }
  function wake() {
    awakeUntil = performance.now() / 1000 + AWAKE;
    if (!raf && !gone) raf = requestAnimationFrame(frame);
  }
  view = { redraw: render };
  wake();

  // updates one at a time, the latest wins
  let next: string | null = null;
  let updating: Promise<void> | null = null;
  async function apply() {
    while (next !== null && !gone) {
      const nx = next;
      next = null;
      const fresh = await buildModel(e, nx);
      if (gone) return disposeModel(fresh);
      scene.remove(model.root);
      disposeModel(model);
      model = fresh;
      scene.add(model.root);
      model.body?.mixer.update(0);
      render();
    }
  }

  return {
    turn(s: number) {
      speed = Math.max(-1, Math.min(1, s));
      wake();
    },
    rotate(deg: number) {
      angle += (deg * Math.PI) / 180;
      target = null;
      wake();
      if (!speed) render();
    },
    spin(deg: number) {
      target = (target ?? angle) + (deg * Math.PI) / 180;
      wake();
    },
    update(nx: string) {
      next = nx;
      updating ??= apply().finally(() => (updating = null));
      return updating;
    },
    stop() {
      gone = true;
      if (raf) cancelAnimationFrame(raf);
      if (view?.redraw === render) view = null;
      canvas.remove();
      disposeModel(model);
    },
  };
}
