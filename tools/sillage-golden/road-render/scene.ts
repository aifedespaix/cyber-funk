// Rendu de référence de la route de Sillage, sur les scènes de `crates/cf-world-neon/examples/road_demo` (`scenes.json`).
//
// La route est **la classe `Road` d'origine** (`sillage/engine/road.ts`, copie conforme, avec `landmarks/field.ts`,
// `landmarks/glsl.ts` et `world-origin.ts` dont elle dépend) : ce n'est pas une réécriture. Elle est nourrie comme
// `sillage-scene.ts` le fait (`road.update(voiture, temps, pouls, basses, lueur, teinte)`), avec les réglages que
// cyber-funk a réellement utilisés, puis rendue comme Sillage rend sa scène : cible HDR demi-flottante, `RenderPass`,
// `OutputPass` (conversion sRGB ; pas de bloom ni d'étalonnage, `J5-01` ne compare que la route), exposition 1.
//
// **Scène à garder identique** à `road_demo` : caméra de poursuite, pavé du point suivi (violet émissif, 1,9 × 1,3 ×
// 4,6 m), fond noir.

import {
  BoxGeometry,
  Color,
  HalfFloatType,
  Mesh,
  MeshBasicMaterial,
  NoToneMapping,
  PerspectiveCamera,
  Scene,
  SRGBColorSpace,
  Vector3,
  WebGLRenderer,
  WebGLRenderTarget,
} from "three";
import { EffectComposer } from "three/examples/jsm/postprocessing/EffectComposer.js";
import { OutputPass } from "three/examples/jsm/postprocessing/OutputPass.js";
import { RenderPass } from "three/examples/jsm/postprocessing/RenderPass.js";
import { CityField } from "./sillage/engine/landmarks/field";
import { Road } from "./sillage/engine/road";

/** Une scène, telle qu'écrite par `road_demo` dans `scenes.json`. */
export interface SceneSpec {
  name: string;
  carZ: number;
  time: number;
  pulse: number;
  bass: number;
  intensity: number;
  wet: number;
  rain: number;
  draw: number;
  brake: number;
  car: boolean;
  originZ: number;
  fov: number;
  camera: [number, number, number];
  forward: [number, number, number];
}

const W = 960;
const H = 540;

/** Thème « Void Velvet » (`packages/tokens/src/scene.css`), lu comme `readScenePalette` : hex sRGB → espace de travail linéaire. */
const hex = (value: string): Color => new Color().setStyle(value, SRGBColorSpace);
const palette = {
  ink: hex("#050508"),
  velvet: hex("#1c132b"),
  mint: hex("#00e5ff"),
  azure: hex("#12a8d4"),
  violet: hex("#6a4cc2"),
  rose: hex("#ff5733"),
  amber: hex("#ff9a5c"),
  ice: hex("#e6f7ff"),
  alarm: hex("#ff2e1f"),
  lime: hex("#39ff6a"),
  sun: hex("#ffd23f"),
};
const colors = palette;

const canvas = document.createElement("canvas");
document.body.appendChild(canvas);
const renderer = new WebGLRenderer({ canvas, antialias: false, preserveDrawingBuffer: true });
renderer.setPixelRatio(1);
renderer.setSize(W, H);
renderer.setClearColor(0x000000, 1);
renderer.toneMapping = NoToneMapping;

const field = new CityField();
const road = new Road(palette, field);
const scene = new Scene();
scene.add(road.mesh);
// Le pavé du point suivi : émissif, sans éclairage.
const car = new Mesh(new BoxGeometry(1.9, 1.3, 4.6), new MeshBasicMaterial({ color: new Color(0.4, 0.15, 1.2) }));
scene.add(car);

const camera = new PerspectiveCamera(62, W / H, 0.1, 2600);

const target = new WebGLRenderTarget(W, H, { type: HalfFloatType });
const composer = new EffectComposer(renderer, target);
composer.addPass(new RenderPass(scene, camera));
composer.addPass(new OutputPass());
composer.setPixelRatio(1);
composer.setSize(W, H);

/** `SectionTracker.mood` : de l'azur (calme) au violet (groove) puis au corail (sommet). */
function moodFor(intensity: number, out: Color): Color {
  const t = Math.min(1, Math.max(0, intensity));
  if (t < 0.5) return out.copy(colors.azure).lerp(colors.violet, t * 2);
  return out.copy(colors.violet).lerp(colors.rose, (t - 0.5) * 2);
}

const mood = new Color();
const glow = colors.violet.clone();

/** Rend une scène et renvoie l'image (PNG en data URL). */
function renderScene(s: SceneSpec): string {
  moodFor(s.intensity, mood);
  road.update(new Vector3(0, 0.65, s.carZ), s.time, s.pulse, s.bass, glow, mood);
  road.setDraw(s.draw);
  road.setBrake(s.brake);
  field.uniforms.uWet.value = s.wet;
  field.uniforms.uRain.value = s.rain;
  car.visible = s.car;
  car.position.set(0, 0.65, s.carZ);
  camera.fov = s.fov;
  camera.updateProjectionMatrix();
  camera.position.set(...s.camera);
  camera.lookAt(s.camera[0] + s.forward[0], s.camera[1] + s.forward[1], s.camera[2] + s.forward[2]);
  camera.updateMatrixWorld();
  composer.render();
  return canvas.toDataURL("image/png");
}

(window as unknown as { renderScene: typeof renderScene }).renderScene = renderScene;
