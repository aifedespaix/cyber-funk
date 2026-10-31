import * as THREE from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { EffectComposer } from 'three/examples/jsm/postprocessing/EffectComposer.js';
import { RenderPass } from 'three/examples/jsm/postprocessing/RenderPass.js';
import { UnrealBloomPass } from 'three/examples/jsm/postprocessing/UnrealBloomPass.js';
import { OutputPass } from 'three/examples/jsm/postprocessing/OutputPass.js';

/** Environnement synthwave pour les reflets : quelques panneaux émissifs cyan et magenta autour d'un ciel noir. */
function neonEnvironment(renderer: THREE.WebGLRenderer): THREE.Texture {
  const env = new THREE.Scene();
  env.background = new THREE.Color(0x04040a);
  const panel = (color: number, x: number, y: number, z: number, w: number, h: number) => {
    const mat = new THREE.MeshBasicMaterial({ color: new THREE.Color(color).multiplyScalar(6), side: THREE.DoubleSide });
    const m = new THREE.Mesh(new THREE.PlaneGeometry(w, h), mat);
    m.position.set(x, y, z);
    m.lookAt(0, 0, 0);
    env.add(m);
  };
  panel(0x00e5ff, -8, 4, 3, 8, 3);
  panel(0xff0d73, 8, 3, -4, 8, 3);
  panel(0xff0d73, 0, 9, 8, 10, 2);
  panel(0x00e5ff, 2, 2, -9, 10, 2);
  const pmrem = new THREE.PMREMGenerator(renderer);
  const tex = pmrem.fromScene(env, 0.02).texture;
  pmrem.dispose();
  return tex;
}

/** Rendu, caméra orbitale, sol, bloom. */
export class Stage {
  readonly renderer: THREE.WebGLRenderer;
  readonly scene = new THREE.Scene();
  readonly camera = new THREE.PerspectiveCamera(50, 1, 0.02, 400);
  readonly orbit: OrbitControls;
  private readonly composer: EffectComposer;
  private readonly bloom: UnrealBloomPass;
  private readonly floor: THREE.Mesh;
  private readonly grid: THREE.GridHelper;

  constructor(readonly canvas: HTMLCanvasElement) {
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.scene.background = new THREE.Color(0x05050a);
    this.scene.fog = new THREE.Fog(0x05050a, 25, 90);
    this.scene.environment = neonEnvironment(this.renderer);
    this.scene.environmentIntensity = 0.6;

    this.floor = new THREE.Mesh(
      new THREE.PlaneGeometry(400, 400),
      new THREE.MeshStandardMaterial({ color: 0x0b0b14, roughness: 0.45, metalness: 0.3 }),
    );
    this.floor.rotation.x = -Math.PI / 2;
    this.grid = new THREE.GridHelper(120, 120, 0x1c6a7a, 0x161630);
    this.scene.add(this.floor, this.grid);

    this.camera.position.set(5, 2.2, 6);
    this.orbit = new OrbitControls(this.camera, canvas);
    this.orbit.target.set(0, 0.3, 0.3);
    this.orbit.enableDamping = true;

    this.composer = new EffectComposer(this.renderer);
    this.composer.addPass(new RenderPass(this.scene, this.camera));
    this.bloom = new UnrealBloomPass(new THREE.Vector2(1, 1), 0.6, 0.5, 0.9);
    this.composer.addPass(this.bloom);
    this.composer.addPass(new OutputPass());
    window.addEventListener('resize', () => this.resize());
    this.resize();
  }

  resize(): void {
    const w = window.innerWidth;
    const h = window.innerHeight;
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.setSize(w, h, false);
    this.composer.setSize(w, h);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
  }

  setGround(y: number): void {
    this.floor.position.y = y - 0.002;
    this.grid.position.y = y;
  }

  setLook(bloom: number, threshold: number, exposure: number): void {
    this.bloom.strength = bloom;
    this.bloom.threshold = threshold;
    this.renderer.toneMappingExposure = exposure;
  }

  render(): void {
    this.composer.render();
  }
}
