// Rendu de référence du post-process de Sillage, sur la scène de `crates/cf-render/examples/postfx_demo`.
//
// La chaîne est celle du constructeur de `PostFX` (`engine/postfx.ts`) : cible HDR demi-flottante, `RenderPass`,
// `UnrealBloomPass(résolution / 2, 0.3, 0.22, 0.85)`, `ShaderPass(GRADE)` avec le shader d'origine (copie conforme
// dans `../sillage/engine/postfx-grade.ts`), `OutputPass` ; tonemapping `NeutralToneMapping`, exposition 1
// (`sillage-scene.ts`). Les uniformes de chaque étape sont ceux que cyber-funk a réellement envoyés à son shader
// (`phases.json` écrit par l'exemple) : on compare les deux rendus, pas deux logiques.
//
// **Scène à garder identique** à `spawn_scene` de `postfx_demo/main.rs` (matériaux sans éclairage, lumière linéaire).

import {
  BackSide,
  BoxGeometry,
  Color,
  HalfFloatType,
  Mesh,
  MeshBasicMaterial,
  NeutralToneMapping,
  PerspectiveCamera,
  PlaneGeometry,
  Scene,
  SphereGeometry,
  Vector2,
  WebGLRenderer,
  WebGLRenderTarget,
} from "three";
import { EffectComposer } from "three/examples/jsm/postprocessing/EffectComposer.js";
import { OutputPass } from "three/examples/jsm/postprocessing/OutputPass.js";
import { RenderPass } from "three/examples/jsm/postprocessing/RenderPass.js";
import { ShaderPass } from "three/examples/jsm/postprocessing/ShaderPass.js";
import { UnrealBloomPass } from "three/examples/jsm/postprocessing/UnrealBloomPass.js";
import { GRADE } from "../sillage/engine/postfx-grade";

/** Uniformes d'une étape, tels qu'écrits par `postfx_demo` dans `phases.json`. */
export interface Phase {
  name: string;
  bloom: number;
  aberration: number;
  time: number;
  grain: number;
  lens: number;
  rainTime: number;
  aspect: number;
  flash: number;
  sweep: number;
  sweepPos: number;
  contrast: number;
  saturation: number;
  slow: number;
  blur: number;
  flashColor: [number, number, number];
  sweepColor: [number, number, number];
}

const W = 960;
const H = 540;
/** Charte : rouge, vert, bleu, cyan, blanc, gris 18 % (même ordre que `CHART` de `check.rs`). */
const CHART: [number, number, number][] = [
  [0.8, 0.05, 0.05],
  [0.05, 0.6, 0.1],
  [0.05, 0.1, 0.8],
  [0.0, 0.6, 0.8],
  [0.8, 0.8, 0.8],
  [0.18, 0.18, 0.18],
];

/** Matériau sans éclairage ; `new Color(r, g, b)` prend des composantes linéaires (espace de travail de three.js). */
function unlit(r: number, g: number, b: number): MeshBasicMaterial {
  return new MeshBasicMaterial({ color: new Color(r, g, b) });
}

function buildScene(): Scene {
  const scene = new Scene();
  const sky = new Mesh(new SphereGeometry(1000, 32, 18), new MeshBasicMaterial({ color: new Color(0.02, 0.012, 0.04), side: BackSide }));
  scene.add(sky);
  const ground = new Mesh(new BoxGeometry(200, 0.02, 200), unlit(0.015, 0.015, 0.02));
  ground.position.set(0, -0.01, -90);
  scene.add(ground);
  const body = unlit(0.04, 0.03, 0.08);
  const neon = [unlit(0.0, 3.0, 4.0), unlit(3.0, 0.6, 4.0)];
  const strip = new BoxGeometry(0.15, 0.15, 3);
  for (let i = 0; i < 8; i++) {
    const z = -8 - 6 * i;
    const height = 6 + (i % 3) * 2;
    const block = new BoxGeometry(3, height, 3);
    ([[-1, neon[0]!], [1, neon[1]!]] as const).forEach(([side, material]) => {
      const b = new Mesh(block, body);
      b.position.set(side * 5, height / 2, z);
      scene.add(b);
      const s = new Mesh(strip, material);
      s.position.set(side * 3.5, height, z);
      scene.add(s);
    });
  }
  const dash = new BoxGeometry(0.2, 0.02, 1.5);
  const amber = unlit(2.5, 2.0, 0.6);
  for (let i = 0; i < 14; i++) {
    const d = new Mesh(dash, amber);
    d.position.set(0, 0.01, -3 - 4 * i);
    scene.add(d);
  }
  const patch = new PlaneGeometry(0.7, 0.7);
  CHART.forEach(([r, g, b], i) => {
    const p = new Mesh(patch, unlit(r, g, b));
    p.position.set(-2 + 0.8 * i, 1.6, -6);
    scene.add(p);
  });
  return scene;
}

const canvas = document.createElement("canvas");
document.body.appendChild(canvas);
const renderer = new WebGLRenderer({ canvas, antialias: false, preserveDrawingBuffer: true });
renderer.setPixelRatio(1);
renderer.setSize(W, H);
renderer.toneMapping = NeutralToneMapping;
renderer.toneMappingExposure = 1;

const scene = buildScene();
const camera = new PerspectiveCamera(62, W / H, 0.1, 2600);
camera.position.set(0, 1.6, 0);
camera.lookAt(0, 1.6, -1);

// Constructeur de `PostFX`, mode spectacle (`calm = false`), sans passe de cockpit.
const target = new WebGLRenderTarget(W, H, { type: HalfFloatType });
const composer = new EffectComposer(renderer, target);
composer.addPass(new RenderPass(scene, camera));
const bloom = new UnrealBloomPass(new Vector2(W / 2, H / 2), 0.3, 0.22, 0.85);
composer.addPass(bloom);
const grade = new ShaderPass(GRADE);
grade.uniforms.uFlashColor!.value = new Color();
grade.uniforms.uSweepColor!.value = new Color();
composer.addPass(grade);
composer.addPass(new OutputPass());
// `PostFX.setSize`.
composer.setPixelRatio(1);
composer.setSize(W, H);
grade.uniforms.uAspect!.value = W / H;
bloom.setSize(W / 2, H / 2);

/** Rend une étape et renvoie l'image (PNG en data URL). */
function renderPhase(p: Phase): string {
  const u = grade.uniforms;
  bloom.enabled = p.bloom > 0;
  bloom.strength = p.bloom;
  u.uAberration!.value = p.aberration;
  u.uTime!.value = p.time;
  u.uGrain!.value = p.grain;
  u.uLens!.value = p.lens;
  u.uRainTime!.value = p.rainTime;
  u.uFlash!.value = p.flash;
  u.uSweep!.value = p.sweep;
  u.uSweepPos!.value = p.sweepPos;
  u.uContrast!.value = p.contrast;
  u.uSaturation!.value = p.saturation;
  u.uSlow!.value = p.slow;
  u.uBlur!.value = p.blur;
  (u.uFlashColor!.value as Color).setRGB(...p.flashColor);
  (u.uSweepColor!.value as Color).setRGB(...p.sweepColor);
  composer.render();
  return canvas.toDataURL("image/png");
}

(window as unknown as { renderPhase: typeof renderPhase }).renderPhase = renderPhase;
