/**
 * Optional visual effects for CyberCar: neon bloom + a synthwave reflection environment.
 * The GLB only carries emissive materials; glow and reflections are produced here, in three.js.
 * Everything is skipped / cheaper on the 'low' quality tier.
 */
import {
  BoxGeometry,
  CanvasTexture,
  Color,
  type ColorRepresentation,
  Mesh,
  MeshBasicMaterial,
  MeshStandardMaterial,
  PerspectiveCamera,
  PlaneGeometry,
  PMREMGenerator,
  RepeatWrapping,
  Scene,
  type Texture,
  Vector2,
  type WebGLRenderer,
} from 'three';
import { EffectComposer } from 'three/examples/jsm/postprocessing/EffectComposer.js';
import { RenderPass } from 'three/examples/jsm/postprocessing/RenderPass.js';
import { UnrealBloomPass } from 'three/examples/jsm/postprocessing/UnrealBloomPass.js';
import { OutputPass } from 'three/examples/jsm/postprocessing/OutputPass.js';
import type { Quality } from './CyberCar';

/**
 * A small synthwave "studio": dark room, coloured light strips. Use as scene.environment so the
 * paint, chrome logos and glass reflect neon. Call once; dispose the returned texture when done.
 */
export function makeNeonEnvironment(
  renderer: WebGLRenderer,
  a: ColorRepresentation = 0x00e5ff,
  b: ColorRepresentation = 0xff0d73,
): Texture {
  const room = new Scene();
  room.background = new Color(0x05060d);
  const strip = (c: ColorRepresentation, w: number, h: number, d: number, x: number, y: number, z: number): void => {
    const m = new Mesh(new BoxGeometry(w, h, d), new MeshBasicMaterial({ color: new Color(c).multiplyScalar(3) }));
    m.position.set(x, y, z);
    room.add(m);
  };
  strip(a, 12, 0.3, 0.3, 0, 6, -8); // ceiling strips
  strip(b, 12, 0.3, 0.3, 0, 6, 8);
  strip(a, 0.3, 4, 0.3, -9, 2, 0); // side bars
  strip(b, 0.3, 4, 0.3, 9, 2, 0);
  strip(0xffffff, 3, 0.2, 0.2, 0, 8, 0); // soft top light for the roof
  const panel = (c: ColorRepresentation, k: number, w: number, h: number, x: number, y: number, z: number, ry: number): void => {
    const m = new Mesh(new BoxGeometry(w, h, 0.1), new MeshBasicMaterial({ color: new Color(c).multiplyScalar(k) }));
    m.position.set(x, y, z);
    m.rotation.y = ry;
    room.add(m);
  };
  panel(a, 0.55, 16, 5, -11, 3, 0, Math.PI / 2); // large soft walls: they colour the body reflections
  panel(b, 0.55, 16, 5, 11, 3, 0, Math.PI / 2);
  panel(0xffffff, 0.6, 10, 8, 0, 9, 0, 0); // top softbox
  panel(a, 0.35, 14, 4, 0, 2.5, 12, 0);
  panel(b, 0.35, 14, 4, 0, 2.5, -12, 0);
  const pm = new PMREMGenerator(renderer);
  const tex = pm.fromScene(room, 0.04).texture;
  pm.dispose();
  return tex;
}

export interface NeonFX {
  /** Use instead of renderer.render(scene, camera). */
  render(dt?: number): void;
  setSize(width: number, height: number): void;
  /** 0 = off. Typical 0.6..1.2. */
  setBloomStrength(s: number): void;
  dispose(): void;
}

/**
 * Bloom that only picks up very bright pixels (the emissive neon / lights), so the paint
 * does not glow. On 'low' it returns a plain renderer pass-through (no post-processing cost).
 */
export function createNeonFX(
  renderer: WebGLRenderer,
  scene: Scene,
  camera: PerspectiveCamera,
  quality: Quality,
): NeonFX {
  if (quality === 'low') {
    return {
      render: () => renderer.render(scene, camera),
      setSize: (w, h) => renderer.setSize(w, h),
      setBloomStrength: () => {},
      dispose: () => {},
    };
  }
  const size = renderer.getSize(new Vector2());
  const composer = new EffectComposer(renderer);
  composer.addPass(new RenderPass(scene, camera));
  const bloom = new UnrealBloomPass(size.clone(), 0.9, 0.55, 0.92); // strength, radius, threshold
  composer.addPass(bloom);
  composer.addPass(new OutputPass()); // tone mapping + colour space after bloom
  return {
    render: (dt) => composer.render(dt),
    setSize: (w, h) => composer.setSize(w, h),
    setBloomStrength: (s) => {
      bloom.strength = s;
      bloom.enabled = s > 0;
    },
    dispose: () => composer.dispose(),
  };
}

/* ------------------------------------------------------------------ wet road */

export interface WetRoad {
  mesh: Mesh;
  /** 0 = dry asphalt, 1 = soaked: low roughness, strong reflections of the neon environment, puddles. */
  setWetness(w: number): void;
  /** Scrolls the asphalt so the road looks like it moves under a static car. Call every frame. */
  scroll(dt: number, speedKmh: number): void;
  dispose(): void;
}

/**
 * A simple asphalt plane whose roughness is mapped with puddles. Reflections come from `scene.environment`
 * (see makeNeonEnvironment), so the neon colours show up in the wet road. For true mirror reflections of the car,
 * swap the material for three's `Reflector` (high quality only).
 */
export function createWetRoad(size = 120, textureRepeat = 24): WetRoad {
  const c = document.createElement('canvas');
  c.width = c.height = 512;
  const g = c.getContext('2d')!;
  g.fillStyle = '#e6e6e6'; // dry = rough
  g.fillRect(0, 0, 512, 512);
  for (let i = 0; i < 150; i++) {
    // puddles: dark = smooth
    const x = Math.random() * 512;
    const y = Math.random() * 512;
    const r = 8 + Math.random() * 26;
    const grad = g.createRadialGradient(x, y, 0, x, y, r);
    grad.addColorStop(0, 'rgba(18,18,18,0.95)');
    grad.addColorStop(0.5, 'rgba(18,18,18,0.7)');
    grad.addColorStop(1, 'rgba(18,18,18,0)');
    g.fillStyle = grad;
    g.beginPath();
    g.ellipse(x, y, r, r * (0.5 + Math.random() * 0.5), Math.random() * 3, 0, 6.2832);
    g.fill();
  }
  const tex = new CanvasTexture(c);
  tex.wrapS = tex.wrapT = RepeatWrapping;
  tex.repeat.set(textureRepeat, textureRepeat);
  const mat = new MeshStandardMaterial({ color: 0x0b0c12, roughness: 1, metalness: 0.25, roughnessMap: tex, envMapIntensity: 0.5 });
  const mesh = new Mesh(new PlaneGeometry(size, size), mat);
  mesh.rotation.x = -Math.PI / 2;
  const api: WetRoad = {
    mesh,
    setWetness(w) {
      const k = Math.min(1, Math.max(0, w));
      mat.roughness = 1 - 0.75 * k;
      mat.metalness = 0.25 + 0.3 * k;
      mat.envMapIntensity = 0.5 + 2.2 * k;
    },
    scroll(dt, speedKmh) {
      tex.offset.y += ((speedKmh / 3.6) * dt * textureRepeat) / size; // plane +Y (texture) points to the car's -Z
    },
    dispose() {
      tex.dispose();
      mat.dispose();
      mesh.geometry.dispose();
      mesh.removeFromParent();
    },
  };
  api.setWetness(0);
  return api;
}
