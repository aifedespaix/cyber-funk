/**
 * CyberCar — Three.js controller for car-high.glb / car-low.glb
 *
 * Conventions (same as glTF / three.js): +Y up, the car FACES +Z, its LEFT side is +X
 * (left-hand drive: the driver sits at x = +0.35). Units are metres.
 * See docs/CYBERCAR.md for the full node/material reference.
 */
import {
  AdditiveBlending,
  CanvasTexture,
  CircleGeometry,
  Color,
  type ColorRepresentation,
  ConeGeometry,
  CylinderGeometry,
  DoubleSide,
  Euler,
  Group,
  MathUtils,
  Mesh,
  type MeshStandardMaterial,
  type Object3D,
  Quaternion,
  Vector3,
  type PerspectiveCamera,
  PlaneGeometry,
  PointLight,
  ShaderMaterial,
  SpotLight,
  SRGBColorSpace,
  type Texture,
} from 'three';
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js';
import { MeshoptDecoder } from 'three/examples/jsm/libs/meshopt_decoder.module.js';
import { PaintSystem, type PaintFinish, type PaintFinishName } from './CyberCarPaint';
import { ParticlePool } from './CyberCarParticles';
import { WindshieldRain } from './CyberCarRain';

export { PAINT_FINISHES } from './CyberCarPaint';
export type { PaintFinish, PaintFinishName };

export type Quality = 'high' | 'low';
export type Gear = 'R' | 'N' | 1 | 2 | 3 | 4 | 5 | 6;
export type BlinkerMode = 'off' | 'left' | 'right' | 'hazard';
export type WiperMode = 'off' | 'intermittent' | 'slow' | 'fast';
/** grounded -> deploying (wheels flip, glow spools up, car rises) -> hovering -> retracting (reverse order) -> grounded. */
export type HoverPhase = 'grounded' | 'deploying' | 'hovering' | 'retracting';
export interface HoverState {
  phase: HoverPhase;
  /** 0..1 timeline position (0 = grounded, 1 = fully hovering). */
  progress: number;
  /** 0..1 how far the wheels are flipped flat (can overshoot slightly while deploying). */
  tuck: number;
  /** 0..1 intensity of the repulsor glow. */
  glow: number;
  /** current lift of the body above the ground, metres. */
  height: number;
}

export interface CyberCarOptions {
  /** 'auto' (default) picks from device memory / cores / network. */
  quality?: Quality | 'auto';
  /** Override the model URLs. */
  urls?: Partial<Record<Quality, string>>;
  /** Real SpotLights on the headlights. Default: true on 'high', false on 'low'. */
  spotLights?: boolean;
  /** Coloured PointLights (underglow + cabin) that follow the neon colours. Default: true on 'high'. */
  neonLights?: boolean;
  /** Volumetric light cones in front of the headlights. Default: true on 'high', false on 'low'. */
  beams?: boolean;
  /** Sparks, tyre smoke and windscreen rain drops. Default: true (smaller pools on 'low'). */
  effects?: boolean;
  /**
   * true (default): the car stays still in the scene and the world scrolls under it, so particles are pushed
   * backwards at the car speed. false: you move the car through the scene yourself.
   */
  worldScrolls?: boolean;
  /** Called with 0..1 while the GLB downloads. */
  onProgress?: (ratio: number) => void;
}

/* ------------------------------------------------------------------ constants */

const WHEEL_RADIUS = 0.39; // metres (tyre outer radius)
const MAX_WHEEL_STEER = 0.55; // rad, front wheels at full lock (~31 deg)
const MAX_STEERING_WHEEL = 4.2; // rad, steering wheel at full lock (~240 deg each way)
const MAX_WIPER_ANGLE = 1.15; // rad, sweep up the windscreen (~66 deg)
const PEDAL_TRAVEL = { clutch: 0.3, brake: 0.3, accel: 0.22 }; // rad
const LEVER_LENGTH = 0.2; // m, pivot -> knob centre
const RADIO_TITLE_FONT = 'bold 36px "Orbitron","Rajdhani",monospace';
const AXIS_Y = new Vector3(0, 1, 0);
const RADIO_GAP = 90; // px between the two copies of a scrolling title
const WIPER_LENGTH = 0.68; // m, arm length (see wiper_L/R in the GLB)
const WHEELBASE = 2.76; // m
const PITCH_PER_G = 0.04; // rad of body pitch per g of longitudinal acceleration (~2.3 deg / g)
const ROLL_PER_G = 0.05; // rad of body roll per g of lateral acceleration (~2.9 deg / g)
const SUSP_K = 85; // spring stiffness (1/s²)  -> natural frequency ~1.5 Hz
const SUSP_C = 9.5; // damping (1/s)           -> slightly under-damped, one visible overshoot
const SUSP_PIVOT = new Vector3(0, -0.35, 0); // roll/pitch centre (about axle height)
const AXIS_Z = new Vector3(0, 0, 1);
const WHEEL_TUCK_DROP = 0.22; // m, flipped wheels sink below the body so their rims clear the floor pan
const WHEEL_TUCK_IN = 0.1; // m, and move slightly inwards
const HOVER_DEFAULT_HEIGHT = 0.45; // m of body lift when fully deployed
const GROUND_Y = -0.75; // m, ground level in car space (tyres touch it at rest)

const GLOW_VERT = /* glsl */ `varying vec2 vUv; varying vec3 vN; varying vec3 vV; varying float vY;
  void main(){ vUv = uv; vY = position.y; vN = normalize(normalMatrix * normal);
    vec4 mv = modelViewMatrix * vec4(position, 1.0); vV = normalize(-mv.xyz); gl_Position = projectionMatrix * mv; }`;
/** Repulsor pad under each flipped wheel: soft core, bright rim with rotating segments. */
const PAD_FRAG = /* glsl */ `uniform vec3 uColor; uniform float uA; uniform float uT; varying vec2 vUv;
  void main(){ vec2 p = vUv - 0.5; float r = length(p) * 2.0; if (r > 1.0) discard; float a = atan(p.y, p.x);
    float core = pow(1.0 - r, 1.6);
    float seg = 0.5 + 0.5 * sin(a * 14.0 - uT * 7.0);
    float rim = smoothstep(0.07, 0.0, abs(r - 0.88)) * (0.45 + 0.55 * seg);
    float inner = smoothstep(0.6, 0.0, r);
    float alpha = (core * 0.55 + rim * 1.4 + inner * 0.9) * uA;
    gl_FragColor = vec4(uColor * (1.0 + inner), alpha); }`;
/** Vertical light column from the pad down to the ground: bright at the pad, fading out, fresnel-soft edges. */
const COLUMN_FRAG = /* glsl */ `uniform vec3 uColor; uniform float uA; uniform float uT; varying vec2 vUv; varying vec3 vN; varying vec3 vV; varying float vY;
  void main(){ float t = clamp(-vY, 0.0, 1.0);
    float edge = pow(abs(dot(normalize(vN), normalize(vV))), 1.3);
    float streak = 0.8 + 0.2 * sin(vUv.x * 40.0 + t * 6.0 - uT * 9.0);
    float a = uA * (pow(1.0 - t, 1.5) * 0.7 + smoothstep(0.82, 1.0, t) * 0.5) * edge * streak;
    gl_FragColor = vec4(uColor, a); }`;
/** Ground ripple: expanding rings + hot centre. */
const RING_FRAG = /* glsl */ `uniform vec3 uColor; uniform float uA; uniform float uT; varying vec2 vUv;
  void main(){ float r = length(vUv - 0.5) * 2.0; if (r > 1.0) discard;
    float fall = pow(1.0 - r, 1.4);
    float ph = fract(uT * 0.55);
    float ring = smoothstep(0.09, 0.0, abs(r - ph)) * (1.0 - ph);
    float ring2 = smoothstep(0.09, 0.0, abs(r - fract(ph + 0.5))) * (1.0 - fract(ph + 0.5)) * 0.6;
    float hot = smoothstep(0.45, 0.0, r);
    gl_FragColor = vec4(uColor, (fall * 0.5 + hot * 0.7 + (ring + ring2) * 1.1) * uA); }`;
/** Big soft pool of light under the whole car. */
const POOL_FRAG = /* glsl */ `uniform vec3 uColor; uniform float uA; uniform float uT; varying vec2 vUv;
  void main(){ float r = length((vUv - 0.5) * 2.0); if (r > 1.0) discard;
    gl_FragColor = vec4(uColor, pow(1.0 - r, 1.9) * uA * (0.9 + 0.1 * sin(uT * 2.0))); }`;

function glowMaterial(frag: string, color: Color): ShaderMaterial {
  return new ShaderMaterial({
    transparent: true,
    depthWrite: false,
    blending: AdditiveBlending,
    side: DoubleSide,
    uniforms: { uColor: { value: color.clone() }, uA: { value: 0 }, uT: { value: 0 } },
    vertexShader: GLOW_VERT,
    fragmentShader: frag,
  });
}

interface WheelTuck {
  id: string;
  /** top-level wheel node (steer_F* or spin_R*) that now lives inside `group`. */
  node: Object3D;
  group: Group;
  base: Vector3;
  /** +1 left (x > 0), -1 right. */
  sign: number;
  pad: Mesh;
  column?: Mesh;
  ring?: Mesh;
  /** pad position in car space, refreshed every frame while hovering. */
  gx: number;
  gy: number;
  gz: number;
}

/** H-pattern: column offset (m, +X = driver's left) and fore/aft offset (m, + = forward). */
const GATE_COL = [0.0525, 0.0175, -0.0175, -0.0525];
const GEAR_POS: Record<string, { col: number; fore: number }> = {
  N: { col: -1, fore: 0 }, // between 3/4 and 5/6
  1: { col: 0, fore: 0.05 },
  2: { col: 0, fore: -0.05 },
  3: { col: 1, fore: 0.05 },
  4: { col: 1, fore: -0.05 },
  5: { col: 2, fore: 0.05 },
  6: { col: 2, fore: -0.05 },
  R: { col: 3, fore: 0.05 },
};

/* ------------------------------------------------------------------ helpers */

export function detectQuality(): Quality {
  const nav = navigator as Navigator & {
    deviceMemory?: number;
    connection?: { saveData?: boolean; effectiveType?: string };
  };
  const slowNet =
    nav.connection?.saveData === true ||
    ['slow-2g', '2g', '3g'].includes(nav.connection?.effectiveType ?? '');
  const lowMem = (nav.deviceMemory ?? 8) <= 4;
  const lowCpu = (nav.hardwareConcurrency ?? 8) <= 4;
  return slowNet || lowMem || lowCpu ? 'low' : 'high';
}

/** A canvas drawn by us and shown through an emissive map. */
class CanvasScreen {
  readonly canvas = document.createElement('canvas');
  readonly ctx: CanvasRenderingContext2D;
  readonly texture: CanvasTexture;
  constructor(w: number, h: number, mat: MeshStandardMaterial) {
    this.canvas.width = w;
    this.canvas.height = h;
    this.ctx = this.canvas.getContext('2d')!;
    this.texture = new CanvasTexture(this.canvas);
    this.texture.flipY = false; // glTF UV convention
    this.texture.colorSpace = SRGBColorSpace;
    mat.map = null;
    mat.color.set(0x000000);
    mat.emissive.set(0xffffff);
    mat.emissiveMap = this.texture;
    mat.emissiveIntensity = 1.2;
    mat.needsUpdate = true;
  }
  touch(): void {
    this.texture.needsUpdate = true;
  }
  dispose(): void {
    this.texture.dispose();
  }
}

/* ------------------------------------------------------------------ main class */

export class CyberCar {
  readonly root: Group;
  readonly quality: Quality;

  private nodes = new Map<string, Object3D>();
  private mats = new Map<string, MeshStandardMaterial>();
  private baseEmissive = new Map<string, number>();
  private paint?: PaintSystem;
  private readonly chassis = new Group();

  // state
  private steer = 0; // -1..1, +1 = left
  private speedKmh = 0;
  private rpm = 0;
  private maxRpm = 8000;
  private gear: Gear = 'N';
  private audioBars: number[] = [];
  private audioTarget: number[] = [];
  private analyser: AnalyserNode | null = null;
  private freq: Uint8Array<ArrayBuffer> | null = null;
  private barsAlive = false;
  private pedals = { throttle: 0, brake: 0, clutch: 0 };
  private headlights = false;
  private blinker: BlinkerMode = 'off';
  private neonPrimary = new Color(0x00e5ff);
  private neonSecondary = new Color(0xff0d73);
  private neonIntensity = 0.45; // exported strength (4) is too hot under ACES tone mapping
  private wiperMode: WiperMode = 'off';
  private wiperRunning = false;
  private wiperPhase = 0;
  private wiperPause = 0;
  private time = 0;
  private radioText = '';
  private radioScroll = 0;
  private radioWidth = 0;
  private speedDirty = true;
  private speedAccum = 0;
  private radioAccum = 0;
  private leverPitch = 0;
  private leverRoll = 0;
  private neonLightList: { light: PointLight; channel: 'p' | 's'; base: number; logo?: boolean }[] = [];
  private wiperBase = new Map<string, Quaternion>();
  private tmpQ = new Quaternion();

  // suspension / body motion (all springs: x = position, v = velocity)
  private sPitch = { x: 0, v: 0 };
  private sRoll = { x: 0, v: 0 };
  private sHeave = { x: 0, v: 0 };
  private manualMotion: { longG?: number; latG?: number } | null = null;
  private suspAmount = 1;
  private shakeAmount = 1;
  private speedSmooth = 0;
  private autoLong = 0;
  private readonly euler = new Euler();
  private readonly tmpV = new Vector3();
  private clock = 0;

  // headlights / beams
  private highBeams = false;
  private rig: { spot?: SpotLight; beam?: Mesh; anchor: Object3D }[] = [];

  // rear logo reactivity
  private logoMat?: MeshStandardMaterial;
  private logoNode?: Object3D;
  private logoBaseScale = new Vector3(1, 1, 1);
  private logoColor = new Color(0x00e5ff);
  private logoFollowNeon = true;
  private logoIntensity = 1;
  private logoBoost = 1.6;
  private logoManual: number | null = null;
  private logoLevel = 0;
  private logoKick = 0;
  private bassTarget = 0;
  private bassAvg = 0;
  private lastBeat = -1;
  private audioFresh = 0;
  private beatCbs = new Set<(strength: number) => void>();

  // effects
  private sparks?: ParticlePool;
  private smoke?: ParticlePool;
  private sparkRate = 0;
  private sparkWhere: 'front' | 'rear' | 'sills' | 'all' = 'all';
  private smokeRate = 0;
  private smokeAxle: 'rear' | 'all' = 'rear';
  private smokeColor = new Color(0xcfd6e6);
  private sparkColor = new Color(0xff9a2e);
  private emitSparkAcc = 0;
  private emitSmokeAcc = 0;
  private worldScrolls = true;
  private rain?: WindshieldRain;
  private rainIntensity = 0;
  private wiperAngle = 0;
  private mirrorScreen?: CanvasScreen;
  private mirrorMat?: MeshStandardMaterial;
  private readonly fwd = new Vector3();

  // hover mode
  private hover = {
    target: 0,
    p: 0,
    duration: 2.4,
    height: HOVER_DEFAULT_HEIGHT,
    lift: { x: 0, v: 0 },
    tuck: 0,
    glow: 0,
    phase: 'grounded' as HoverPhase,
  };
  private hoverIntensity = 1;
  private hoverColor: Color | null = null;
  private groundY = GROUND_Y;
  private readonly tucks: WheelTuck[] = [];
  private readonly wheelBase = new Map<string, Vector3>();
  private readonly hoverRoot = new Group();
  private pool?: Mesh;
  private hoverMats: ShaderMaterial[] = [];
  private underglow?: { light: PointLight; base: number };
  private hoverCbs = new Set<(phase: HoverPhase) => void>();
  private hoverWasActive = false;
  private emitDustAcc = 0;
  private readonly tmpV2 = new Vector3();
  private readonly tmpV3 = new Vector3();
  private readonly tmpQ2 = new Quaternion();
  private readonly hoverTint = new Color();

  private speedScreen?: CanvasScreen;
  private radioScreen?: CanvasScreen;

  static async load(options: CyberCarOptions = {}): Promise<CyberCar> {
    const quality: Quality =
      options.quality === 'high' || options.quality === 'low' ? options.quality : detectQuality();
    const url =
      options.urls?.[quality] ?? (quality === 'high' ? '/models/car-high.glb' : '/models/car-low.glb');
    const loader = new GLTFLoader();
    loader.setMeshoptDecoder(MeshoptDecoder);
    const gltf = await loader.loadAsync(url, (e) => {
      if (options.onProgress && e.total) options.onProgress(e.loaded / e.total);
    });
    return new CyberCar(gltf.scene, quality, options);
  }

  private constructor(root: Group, quality: Quality, options: CyberCarOptions) {
    this.root = root;
    this.quality = quality;

    root.traverse((o) => {
      if (o.name) this.nodes.set(o.name, o);
      const mesh = o as Mesh;
      if (mesh.isMesh) {
        const list = Array.isArray(mesh.material) ? mesh.material : [mesh.material];
        for (const m of list) {
          const sm = m as MeshStandardMaterial;
          if (!this.mats.has(sm.name)) {
            this.mats.set(sm.name, sm);
            this.baseEmissive.set(sm.name, sm.emissiveIntensity);
          }
        }
      }
    });

    // Everything except the wheels moves with the suspension (see setMotion).
    const wheelNodes = new Set(['steer_FL', 'steer_FR', 'spin_RL', 'spin_RR']);
    this.chassis.name = 'chassis';
    for (const c of [...root.children]) if (!wheelNodes.has(c.name)) this.chassis.add(c);
    root.add(this.chassis);
    this.nodes.set('chassis', this.chassis);
    this.setupHover();

    for (const n of ['wiperpivot_L', 'wiperpivot_R']) {
      const o = this.nodes.get(n);
      if (o) this.wiperBase.set(n, o.quaternion.clone());
    }
    const bodyPaint = this.mats.get('body_paint');
    if (bodyPaint) {
      this.paint = new PaintSystem(bodyPaint, root, quality);
      this.mats.set('body_paint', this.paint.material);
    }
    this.setupGlass();
    this.setupScreens();
    this.setupLogo();
    this.setupRain(options.effects ?? true);
    this.setNeonColors(this.neonPrimary, this.neonSecondary);
    this.setBraking(false);
    this.applySignals(false, false);
    this.setRadioText('');
    this.worldScrolls = options.worldScrolls ?? true;
    this.setViewport(typeof window !== 'undefined' ? window.innerHeight : 720, 50);
    if (options.effects ?? true) {
      this.sparks = new ParticlePool(quality === 'high' ? 320 : 100, true);
      this.smoke = new ParticlePool(quality === 'high' ? 220 : 70, false);
    }

    this.createHeadlightRig(options.spotLights ?? quality === 'high', options.beams ?? quality === 'high');
    if (options.neonLights ?? quality === 'high') this.createNeonLights();
    this.setHeadlights(false);
  }

  /* -------------------------------------------------------------- accessors */

  /** Any node of the GLB by its Blender name (e.g. 'wheel_FL'). */
  node(name: string): Object3D | undefined {
    return this.nodes.get(name);
  }
  /** Any material by name (e.g. 'neon_primary'). */
  material(name: string): MeshStandardMaterial | undefined {
    return this.mats.get(name);
  }

  /* ------------------------------------------------------------------ paint */

  /**
   * Recolours the bodywork paint only (masked: tyres, trims, lights and glass are untouched).
   * Any CSS colour / hex number / THREE.Color. The metal, flakes and reflections are kept.
   */
  setPaintColor(color: ColorRepresentation): void {
    this.paint?.setColor(color);
  }

  /** Back to the original violet paint. */
  resetPaint(): void {
    this.paint?.reset();
  }

  /**
   * Paint finish: a preset name ('gloss' | 'metallic' | 'matte' | 'chrome' | 'candy' | 'holo')
   * or a partial set of parameters (metalness, roughness, clearcoat, clearcoatRoughness, flake, iridescence, rim).
   * Clear coat and iridescence are only active on the 'high' quality tier.
   */
  setPaintFinish(finish: PaintFinishName | Partial<PaintFinish>): void {
    this.paint?.setFinish(finish);
  }

  getPaintFinish(): PaintFinish | undefined {
    return this.paint?.getFinish();
  }

  private setupGlass(): void {
    const g = this.mats.get('glass');
    if (!g) return;
    g.transparent = true;
    g.depthWrite = false;
    g.envMapIntensity = 2;
    g.roughness = 0.1; // a mirror-sharp glass turns every small light into a hard disc
  }

  /** 0 = invisible, 1 = opaque. Default is 0.28. */
  setGlassOpacity(opacity: number): void {
    const g = this.mats.get('glass');
    if (g) g.opacity = MathUtils.clamp(opacity, 0, 1);
  }

  /* -------------------------------------------------------------------- neon */

  /** Primary = underglow, wheel rings, dash edges. Secondary = door stripe, wipers, accents. */
  setNeonColors(primary: ColorRepresentation, secondary?: ColorRepresentation): void {
    this.neonPrimary.set(primary);
    if (secondary !== undefined) this.neonSecondary.set(secondary);
    this.paintNeon('neon_primary', this.neonPrimary);
    this.paintNeon('neon_secondary', this.neonSecondary);
    for (const n of this.neonLightList) {
      n.light.color.copy(n.channel === 'p' ? this.neonPrimary : this.neonSecondary);
      if (!n.logo) n.light.intensity = (n.base * this.neonIntensity) / 0.45;
    }
    this.paint?.setRimColors(this.neonPrimary, this.neonSecondary);
    if (this.logoFollowNeon) this.logoColor.copy(this.neonPrimary);
    this.speedDirty = true;
    this.redrawRadio();
    this.redrawMirror();
  }
  /** Multiplier on the exported neon strength (0 = off, 0.45 = default, 1 = very bright). */
  setNeonIntensity(k: number): void {
    this.neonIntensity = Math.max(0, k);
    this.setNeonColors(this.neonPrimary, this.neonSecondary);
  }
  private paintNeon(name: string, c: Color): void {
    const m = this.mats.get(name);
    if (!m) return;
    m.color.copy(c).multiplyScalar(0.25);
    m.emissive.copy(c);
    m.emissiveIntensity = (this.baseEmissive.get(name) ?? 4) * this.neonIntensity * (1 + 0.35 * this.hover.glow);
  }

  /* ------------------------------------------------------------ driving state */

  /** -1 (full right) .. +1 (full left). Moves front wheels and steering wheel. */
  setSteering(value: number): void {
    this.steer = MathUtils.clamp(value, -1, 1);
    const a = this.steer * MAX_WHEEL_STEER;
    for (const n of ['steer_FL', 'steer_FR']) {
      const o = this.nodes.get(n);
      if (o) o.rotation.y = a;
    }
    const sw = this.nodes.get('steering_pivot');
    if (sw) sw.rotation.y = this.steer * MAX_STEERING_WHEEL;
  }

  /** km/h. Updates the dash number and (in update()) the wheel spin. */
  setSpeed(kmh: number): void {
    const v = Math.max(0, Math.round(kmh));
    if (v !== this.speedKmh) {
      this.speedKmh = v;
      this.speedDirty = true;
    }
  }

  /** Engine speed in rev/min (shown as a number + a bar with a red zone). */
  setRpm(rpm: number): void {
    const v = Math.max(0, Math.round(rpm / 10) * 10);
    if (v !== this.rpm) {
      this.rpm = v;
      this.speedDirty = true;
    }
  }
  /** Redline / full scale of the rev bar (default 8000). */
  setMaxRpm(maxRpm: number): void {
    this.maxRpm = Math.max(1000, maxRpm);
    this.speedDirty = true;
  }

  setGear(gear: Gear): void {
    if (this.gear === gear) return;
    this.gear = gear;
    this.speedDirty = true;
  }

  /** Each value 0..1. Brake > 0.05 also lights the brake lights. */
  setPedals(p: Partial<{ throttle: number; brake: number; clutch: number }>): void {
    Object.assign(this.pedals, p);
    const t = this.nodes.get('pivot_pedal_accel');
    const b = this.nodes.get('pivot_pedal_brake');
    const c = this.nodes.get('pivot_pedal_clutch');
    if (t) t.rotation.x = -MathUtils.clamp(this.pedals.throttle, 0, 1) * PEDAL_TRAVEL.accel;
    if (b) b.rotation.x = -MathUtils.clamp(this.pedals.brake, 0, 1) * PEDAL_TRAVEL.brake;
    if (c) c.rotation.x = -MathUtils.clamp(this.pedals.clutch, 0, 1) * PEDAL_TRAVEL.clutch;
    this.setBraking(this.pedals.brake > 0.05);
  }

  /* ------------------------------------------------------------------ lights */

  setHeadlights(on: boolean): void {
    this.headlights = on;
    this.applyHeadlights();
    this.setBraking(this.pedals.brake > 0.05); // tail light level depends on headlights
  }

  /** High beams (feux de route): longer, narrower, brighter. Turning them on also switches the headlights on. */
  setHighBeams(on: boolean): void {
    this.highBeams = on;
    if (on) this.headlights = true;
    this.applyHeadlights();
  }

  private applyHeadlights(): void {
    const on = this.headlights;
    const hb = this.highBeams;
    this.setEmissive('light_head', on ? (hb ? 1.35 : 1) : 0.04);
    for (const r of this.rig) {
      if (r.spot) {
        r.spot.visible = on;
        r.spot.intensity = hb ? 150 : 60;
        r.spot.angle = hb ? 0.3 : 0.5;
        r.spot.penumbra = hb ? 0.5 : 0.7;
        r.spot.distance = hb ? 100 : 55;
        r.spot.target.position.set(r.anchor.position.x, r.anchor.position.y - (hb ? 0.3 : 0.9), r.anchor.position.z + (hb ? 40 : 14));
      }
      if (r.beam) {
        r.beam.visible = on;
        r.beam.scale.setScalar(hb ? 30 : 16);
        (r.beam.material as ShaderMaterial).uniforms.uOpacity.value = hb ? 0.17 : 0.1;
      }
    }
  }

  /** Brake lights: bright when braking, dim red when headlights are on, off otherwise. */
  setBraking(on: boolean): void {
    this.setEmissive('light_brake', on ? 1 : this.headlights ? 0.18 : 0.03);
  }

  setBlinker(mode: BlinkerMode): void {
    this.blinker = mode;
    this.time = 0; // restart the cycle so the first flash is immediate
  }

  private applySignals(left: boolean, right: boolean): void {
    this.setEmissive('signal_FL', left ? 1 : 0.03);
    this.setEmissive('signal_RL', left ? 1 : 0.03);
    this.setEmissive('signal_FR', right ? 1 : 0.03);
    this.setEmissive('signal_RR', right ? 1 : 0.03);
  }

  private setEmissive(name: string, k: number): void {
    const m = this.mats.get(name);
    if (m) m.emissiveIntensity = (this.baseEmissive.get(name) ?? 1) * k;
  }

  /** Coloured PointLights at the neon places: underglow, cabin, dash, front and rear logos. */
  private createNeonLights(): void {
    // decay < 2 and low intensity: these lights sit a few cm from glossy surfaces, so a physical 1/d² law would
    // give hard hot spots and mirror-like discs on the glass and clear coat.
    const defs = [
      { anchor: 'anchor_underglow', channel: 'p' as const, base: 1.6, distance: 5, decay: 1 },
      { anchor: 'anchor_cabin_light', channel: 's' as const, base: 0.45, distance: 2.2, decay: 1.5 },
      { anchor: 'anchor_dash_light', channel: 'p' as const, base: 0.3, distance: 1.6, decay: 1.5 },
      { anchor: 'anchor_front_logo_light', channel: 'p' as const, base: 0.7, distance: 2.5, decay: 1.5 },
      { anchor: 'anchor_rear_logo_light', channel: 'p' as const, base: 0.7, distance: 2.5, decay: 1.5, logo: true },
    ];
    for (const d of defs) {
      const a = this.nodes.get(d.anchor);
      if (!a) continue;
      const l = new PointLight(0xffffff, d.base, d.distance, d.decay);
      l.position.copy(a.position);
      this.chassis.add(l);
      this.neonLightList.push({ light: l, channel: d.channel, base: d.base, logo: d.logo });
    }
    this.setNeonColors(this.neonPrimary, this.neonSecondary);
  }

  private createHeadlightRig(useSpots: boolean, useBeams: boolean): void {
    for (const side of ['L', 'R']) {
      const a = this.nodes.get('anchor_headlight_' + side);
      if (!a) continue;
      const rig: { spot?: SpotLight; beam?: Mesh; anchor: Object3D } = { anchor: a };
      if (useSpots) {
        const s = new SpotLight(0xcfe9ff, 60, 55, 0.5, 0.7, 1.4);
        s.position.copy(a.position);
        s.visible = false;
        this.chassis.add(s, s.target);
        rig.spot = s;
      }
      if (useBeams) {
        // unit cone: apex at the origin, base at z = +1, radius 0.25 -> scaled by the beam length
        const g = new ConeGeometry(0.25, 1, 28, 1, true);
        g.translate(0, -0.5, 0);
        g.rotateX(-Math.PI / 2);
        const m = new ShaderMaterial({
          transparent: true,
          depthWrite: false,
          blending: AdditiveBlending,
          side: DoubleSide,
          uniforms: { uColor: { value: new Color(0xbfe4ff) }, uOpacity: { value: 0.1 } },
          vertexShader: /* glsl */ `varying float vT; varying vec3 vN; varying vec3 vV;
            void main(){ vT = position.z; vN = normalize(normalMatrix * normal);
              vec4 mv = modelViewMatrix * vec4(position, 1.0); vV = normalize(-mv.xyz); gl_Position = projectionMatrix * mv; }`,
          fragmentShader: /* glsl */ `uniform vec3 uColor; uniform float uOpacity; varying float vT; varying vec3 vN; varying vec3 vV;
            void main(){ float edge = pow(abs(dot(normalize(vN), normalize(vV))), 1.6);
              float a = uOpacity * pow(1.0 - clamp(vT, 0.0, 1.0), 1.7) * edge * smoothstep(0.0, 0.04, vT);
              gl_FragColor = vec4(uColor, a); }`,
        });
        const beam = new Mesh(g, m);
        beam.position.copy(a.position);
        beam.rotation.x = 0.03; // slight downward tilt
        beam.visible = false;
        beam.frustumCulled = false;
        this.chassis.add(beam);
        rig.beam = beam;
      }
      this.rig.push(rig);
    }
  }

  /* ------------------------------------------------------------------ wipers */

  setWipers(mode: WiperMode): void {
    this.wiperMode = mode;
  }
  /**
   * Rain intensity 0..1: drops on the windscreen (high/low quality pools) and, with `autoWipers`
   * (default), the matching wiper mode. Wipers really clear the drops they sweep over.
   */
  setRain(intensity: number, autoWipers = true): void {
    this.rainIntensity = MathUtils.clamp(intensity, 0, 1);
    if (autoWipers) this.setWipers(intensity <= 0.02 ? 'off' : intensity < 0.35 ? 'intermittent' : intensity < 0.7 ? 'slow' : 'fast');
  }

  private setupRain(enabled: boolean): void {
    if (!enabled) return;
    const mesh = this.nodes.get('rain_windshield') as Mesh | undefined;
    const mat = this.mats.get('rain_glass');
    const pivots = ['wiperpivot_L', 'wiperpivot_R'].map((n) => this.nodes.get(n));
    if (!mesh || !mesh.isMesh || !mat || pivots.some((o) => !o)) return;
    const wipers = (pivots as Object3D[]).map((pivot) => ({ pivot, base: this.wiperBase.get(pivot.name)!, length: WIPER_LENGTH }));
    this.rain = new WindshieldRain(mesh, mat, wipers, this.quality);
  }
  private updateWipers(dt: number): void {
    const m = this.wiperMode;
    if (!this.wiperRunning && m !== 'off') {
      if (this.wiperPause > 0) this.wiperPause -= dt;
      else {
        this.wiperRunning = true;
        this.wiperPhase = 0;
      }
    }
    let angle = 0;
    if (this.wiperRunning) {
      const period = m === 'fast' ? 0.7 : 1.3;
      this.wiperPhase += dt / period;
      if (this.wiperPhase >= 1) {
        this.wiperPhase = 0;
        this.wiperRunning = m === 'fast' || m === 'slow';
        if (m === 'intermittent') this.wiperPause = 2.5;
      }
      if (this.wiperRunning) angle = MAX_WIPER_ANGLE * (0.5 - 0.5 * Math.cos(2 * Math.PI * this.wiperPhase));
    }
    this.wiperAngle = angle;
    this.tmpQ.setFromAxisAngle(AXIS_Y, angle);
    for (const [n, base] of this.wiperBase) this.nodes.get(n)?.quaternion.copy(base).multiply(this.tmpQ);
  }

  /* ------------------------------------------------------------------- radio */

  /** Scrolling title on the car radio. Empty string shows "NO SIGNAL". */
  setRadioText(text: string): void {
    this.radioText = text;
    this.radioScroll = 0;
    const ctx = this.radioScreen?.ctx;
    if (ctx) {
      ctx.font = RADIO_TITLE_FONT;
      this.radioWidth = ctx.measureText(text || '').width;
    }
    this.redrawRadio();
  }

  /**
   * Live audio bars under the title. Feed it every frame with either
   * normalised levels (0..1, scale = 1) or bytes from getByteFrequencyData (scale = 255).
   * Any length is accepted; it is binned to 24 bars (16 on 'low').
   */
  setAudioLevels(levels: ArrayLike<number>, scale = 1): void {
    this.ingestAudio(levels, scale);
  }

  /** Bars + bass energy from one spectrum. `binHz` (when known) makes the bass band exactly 20-150 Hz. */
  private ingestAudio(levels: ArrayLike<number>, scale: number, binHz?: number): void {
    const n = this.barCount();
    const len = levels.length;
    if (!len) return;
    const kmax = Math.max(1, binHz ? Math.ceil(150 / binHz) : Math.round(len * 0.05));
    let bs = 0;
    for (let k = 0; k < kmax && k < len; k++) bs += levels[k];
    this.bassTarget = Math.min(1, bs / Math.min(kmax, len) / scale);
    this.audioFresh = 0.3;
    const usable = Math.max(n, Math.floor(len * 0.5)); // music energy sits in the lower half of an FFT
    for (let i = 0; i < n; i++) {
      const a = Math.floor(Math.pow(i / n, 1.7) * usable);
      const b = Math.max(a + 1, Math.floor(Math.pow((i + 1) / n, 1.7) * usable));
      let sum = 0;
      for (let k = a; k < b && k < len; k++) sum += levels[k];
      this.audioTarget[i] = Math.min(1, sum / (b - a) / scale);
    }
    this.barsAlive = true;
  }

  /** Easiest live beat: pass your Web Audio AnalyserNode (or null to detach). Read automatically in update(). */
  attachAnalyser(analyser: AnalyserNode | null): void {
    this.analyser = analyser;
    this.freq = analyser ? new Uint8Array(analyser.frequencyBinCount) : null;
    if (!analyser) this.audioTarget.fill(0);
  }

  private barCount(): number {
    return this.quality === 'high' ? 24 : 16;
  }

  private redrawRadio(): void {
    const s = this.radioScreen;
    if (!s) return;
    const { ctx, canvas } = s;
    const W = canvas.width;
    const H = canvas.height;
    ctx.fillStyle = '#02030a';
    ctx.fillRect(0, 0, W, H);
    const sec = '#' + this.neonSecondary.getHexString();
    const pri = '#' + this.neonPrimary.getHexString();
    ctx.font = 'bold 20px "Orbitron","Rajdhani",monospace';
    ctx.fillStyle = sec;
    ctx.fillText(this.radioText ? '▶ NOW PLAYING' : 'NO SIGNAL', 12, 28);
    ctx.fillStyle = pri;
    ctx.fillRect(12, 36, W - 24, 2);
    if (this.radioText) {
      ctx.font = RADIO_TITLE_FONT;
      ctx.fillStyle = pri;
      const room = W - 24;
      if (this.radioWidth <= room) ctx.fillText(this.radioText, 12, 78);
      else {
        const x = 12 - this.radioScroll;
        ctx.save();
        ctx.beginPath();
        ctx.rect(12, 42, room, 46);
        ctx.clip();
        ctx.fillText(this.radioText, x, 78);
        ctx.fillText(this.radioText, x + this.radioWidth + RADIO_GAP, 78);
        ctx.restore();
      }
    }
    // audio bars: stacked segments, primary colour -> secondary on the top ones
    const n = this.barCount();
    const slot = (W - 24) / n;
    const barW = slot * 0.72;
    const segs = 9;
    const segH = 5;
    for (let i = 0; i < n; i++) {
      const lit = Math.round((this.audioBars[i] ?? 0) * segs);
      for (let k = 0; k < segs; k++) {
        ctx.fillStyle = k < lit ? (k >= segs - 2 ? sec : pri) : 'rgba(255,255,255,0.07)';
        ctx.fillRect(12 + i * slot, H - 6 - (k + 1) * (segH + 1), barW, segH);
      }
    }
    s.touch();
  }

  /* --------------------------------------------------------------- dashboard */

  private redrawSpeed(): void {
    const s = this.speedScreen;
    if (!s) return;
    const { ctx, canvas } = s;
    const W = canvas.width;
    const H = canvas.height;
    const pri = '#' + this.neonPrimary.getHexString();
    const sec = '#' + this.neonSecondary.getHexString();
    ctx.fillStyle = '#02030a';
    ctx.fillRect(0, 0, W, H);
    if (this.quality === 'high') {
      ctx.shadowColor = pri;
      ctx.shadowBlur = 18;
    }
    if (this.hover.phase !== 'grounded') {
      ctx.shadowBlur = 0;
      ctx.font = 'bold 15px "Orbitron","Rajdhani",monospace';
      ctx.fillStyle = sec;
      ctx.fillText('\u25C6 HOVER', 14, 17);
    }
    // speed (big)
    ctx.fillStyle = pri;
    ctx.font = 'bold 84px "Orbitron","Rajdhani",monospace';
    ctx.textAlign = 'right';
    ctx.fillText(String(this.speedKmh).padStart(3, '0'), 226, 86);
    ctx.shadowBlur = 0;
    ctx.textAlign = 'left';
    ctx.font = 'bold 20px "Orbitron","Rajdhani",monospace';
    ctx.fillStyle = sec;
    ctx.fillText('KM/H', 234, 86);
    // rpm number
    const redline = this.rpm > this.maxRpm * 0.85;
    ctx.textAlign = 'right';
    ctx.font = 'bold 44px "Orbitron","Rajdhani",monospace';
    ctx.fillStyle = redline ? sec : pri;
    ctx.fillText(String(this.rpm).padStart(4, '0'), 398, 52);
    ctx.textAlign = 'left';
    ctx.font = 'bold 18px "Orbitron","Rajdhani",monospace';
    ctx.fillStyle = sec;
    ctx.fillText('RPM', 316, 80);
    // gear box
    ctx.strokeStyle = sec;
    ctx.lineWidth = 3;
    ctx.strokeRect(416, 14, 80, 74);
    ctx.font = 'bold 56px "Orbitron","Rajdhani",monospace';
    ctx.textAlign = 'center';
    ctx.fillStyle = pri;
    ctx.fillText(String(this.gear), 456, 70);
    ctx.textAlign = 'left';
    // rev bar (0..maxRpm), last 15 % in the secondary colour
    const n = 40;
    const on = Math.min(n, Math.round((this.rpm / this.maxRpm) * n));
    for (let i = 0; i < n; i++) {
      ctx.fillStyle = i < on ? (i >= n * 0.85 ? sec : pri) : 'rgba(255,255,255,0.08)';
      ctx.fillRect(14 + i * 12, 106, 9, 24);
    }
    s.touch();
  }

  private setupScreens(): void {
    const sm = this.mats.get('screen_speed');
    const rm = this.mats.get('screen_radio');
    if (sm) this.speedScreen = new CanvasScreen(512, 145, sm);
    if (rm) this.radioScreen = new CanvasScreen(380, 150, rm);
    const mm = this.mats.get('screen_mirror');
    if (mm) {
      this.mirrorMat = mm;
      this.mirrorScreen = new CanvasScreen(256, 56, mm);
    }
    this.redrawSpeed();
    this.redrawMirror();
  }

  private redrawMirror(): void {
    const s = this.mirrorScreen;
    if (!s || this.mirrorMat?.emissiveMap !== s.texture) return;
    const { ctx, canvas } = s;
    const W = canvas.width;
    const H = canvas.height;
    const pri = '#' + this.neonPrimary.getHexString();
    const sec = '#' + this.neonSecondary.getHexString();
    ctx.fillStyle = '#02030a';
    ctx.fillRect(0, 0, W, H);
    ctx.strokeStyle = pri;
    ctx.globalAlpha = 0.5;
    ctx.lineWidth = 1;
    ctx.beginPath(); // road perspective lines
    for (let i = -4; i <= 4; i++) {
      ctx.moveTo(W / 2, 14);
      ctx.lineTo(W / 2 + i * 60, H);
    }
    ctx.stroke();
    ctx.globalAlpha = 1;
    ctx.fillStyle = sec;
    ctx.fillRect(0, 13, W, 2); // horizon
    ctx.font = 'bold 12px "Orbitron","Rajdhani",monospace';
    ctx.fillText('REAR', 8, 11);
    s.touch();
  }

  /**
   * Shows your own texture on the rear-view mirror screen (e.g. the colour texture of a WebGLRenderTarget fed by a
   * camera that looks backwards). Pass null to restore the built-in neon display.
   */
  setRearViewTexture(texture: Texture | null): void {
    const m = this.mirrorMat;
    if (!m) return;
    m.emissiveMap = texture ?? this.mirrorScreen?.texture ?? null;
    m.needsUpdate = true;
    if (!texture) this.redrawMirror();
  }

  /* ------------------------------------------------------------ cockpit view */

  /** Parents `camera` to the car at the driver's eye point, looking forward. */
  attachCameraToCockpit(camera: PerspectiveCamera): void {
    const a = this.nodes.get('cam_cockpit');
    this.chassis.add(camera); // moves with the body (suspension)
    if (a) camera.position.copy(a.position);
    camera.rotation.set(0, Math.PI, 0); // three cameras look down -Z, the car faces +Z
    camera.near = 0.05;
    camera.updateProjectionMatrix();
  }

  /* ------------------------------------------------------------- frame update */

  /** Call once per frame with the elapsed time in seconds. */
  update(dt: number): void {
    this.time += dt;
    this.clock += dt;

    // wheel spin
    // (hover: the wheels lie flat and turn like turbines, independent of the ground speed; front wheels stop steering)
    this.stepHover(dt);
    const tuckN = Math.min(1, this.hover.tuck);
    const spin = ((this.speedKmh / 3.6) * dt) / WHEEL_RADIUS * (this.gear === 'R' ? -1 : 1);
    const turbine = (4 + 18 * this.hover.glow) * dt;
    for (const n of ['spin_FL', 'spin_FR', 'spin_RL', 'spin_RR']) {
      const o = this.nodes.get(n);
      if (!o) continue;
      const side = n.endsWith('L') ? 1 : -1;
      o.rotation.x = (o.rotation.x + spin * (1 - tuckN) + side * turbine * tuckN) % (Math.PI * 2);
    }
    for (const n of ['steer_FL', 'steer_FR']) {
      const o = this.nodes.get(n);
      if (o) o.rotation.y = this.steer * MAX_WHEEL_STEER * (1 - tuckN);
    }

    // blinkers: 1.5 Hz, on for the first half of each cycle
    if (this.blinker === 'off') this.applySignals(false, false);
    else {
      const on = this.time % 0.66 < 0.33;
      this.applySignals(on && (this.blinker === 'left' || this.blinker === 'hazard'), on && (this.blinker === 'right' || this.blinker === 'hazard'));
    }

    this.updateWipers(dt);
    this.updateLever(dt);
    this.updateSuspension(dt);
    this.applyHover(dt);
    this.rain?.update(dt, this.rainIntensity, [this.wiperAngle, this.wiperAngle]);
    this.updateLogo(dt);
    this.updateEffects(dt);

    // dash redraw capped at ~15 Hz (and only when something changed)
    this.speedAccum += dt;
    if (this.speedDirty && this.speedAccum > 1 / 15) {
      this.speedAccum = 0;
      this.speedDirty = false;
      this.redrawSpeed();
    }

    // radio: marquee and/or audio bars. 30 Hz on high, 15 Hz on low, and only while something moves.
    if (this.analyser && this.freq) {
      this.analyser.getByteFrequencyData(this.freq);
      this.ingestAudio(this.freq, 255, this.analyser.context.sampleRate / this.analyser.fftSize);
    }
    const scrolling = this.radioText !== '' && this.radioWidth > 380 - 24;
    const barsMoving = this.barsAlive || this.audioBars.some((b) => b > 0.01);
    if (scrolling || barsMoving) {
      this.radioAccum += dt;
      const step = this.quality === 'high' ? 1 / 30 : 1 / 15;
      if (this.radioAccum >= step) {
        const el = this.radioAccum;
        this.radioAccum = 0;
        if (scrolling) this.radioScroll = (this.radioScroll + 70 * el) % (this.radioWidth + RADIO_GAP);
        // fast attack, smooth decay
        const n = this.barCount();
        for (let i = 0; i < n; i++) {
          const t = this.audioTarget[i] ?? 0;
          const b = this.audioBars[i] ?? 0;
          this.audioBars[i] = t > b ? t : Math.max(t, b - el * 2.2);
        }
        if (!this.analyser) this.audioTarget.fill(0); // push mode: bars fall unless fed again
        this.barsAlive = false;
        this.redrawRadio();
      }
    }
  }

  private updateLever(dt: number): void {
    const lever = this.nodes.get('pivot_gear_lever');
    if (!lever) return;
    const g = GEAR_POS[String(this.gear)];
    const roll = g.col < 0 ? 0 : Math.asin(GATE_COL[g.col] / LEVER_LENGTH); // + = toward the left
    const pitch = Math.asin(g.fore / LEVER_LENGTH); // + = forward
    const k = 1 - Math.exp(-dt * 18);
    this.leverPitch += (pitch - this.leverPitch) * k;
    this.leverRoll += (roll - this.leverRoll) * k;
    lever.rotation.x = this.leverPitch;
    lever.rotation.z = -this.leverRoll; // glTF +Z rotation moves the knob toward -X
  }

  /* --------------------------------------------------------------- suspension */

  /**
   * Body motion. By default (auto) it is derived from setSpeed() (longitudinal acceleration) and setSteering()
   * (lateral acceleration = v² · tan(steer) / wheelbase). Call setMotion({ longG, latG }) to drive it yourself from
   * your physics (then auto is disabled); setMotion(null) goes back to auto.
   *
   * Sign convention: longG > 0 = accelerating (nose up, tail squats), longG < 0 = braking (nose dives);
   * latG > 0 = turning LEFT (body rolls to the right). Values are in g (9.81 m/s²), clamped to about ±1.5.
   */
  setMotion(m: { longG?: number; latG?: number } | null): void {
    this.manualMotion = m;
  }

  /** Overall strength of the body motion (0 = rigid, 1 = default, 2 = exaggerated). */
  setSuspensionAmount(k: number): void {
    this.suspAmount = MathUtils.clamp(k, 0, 3);
  }

  /** Engine vibration of the body at idle/revs (0 = none, 1 = default). Driven by setRpm(). */
  setEngineShake(k: number): void {
    this.shakeAmount = Math.max(0, k);
  }

  /** One-shot jolt (kerb, pothole, landing). strength 0..2. */
  bump(strength = 1): void {
    this.sHeave.v -= 0.5 * strength;
    this.sPitch.v += (Math.random() - 0.5) * 0.25 * strength;
    this.sRoll.v += (Math.random() - 0.5) * 0.25 * strength;
  }

  private updateSuspension(dt: number): void {
    const step = Math.min(dt, 0.05);
    if (step <= 0) return;
    const g = 9.81;
    let longG = 0;
    let latG = 0;
    if (this.manualMotion) {
      longG = this.manualMotion.longG ?? 0;
      latG = this.manualMotion.latG ?? 0;
    } else {
      const v = (this.speedKmh / 3.6) * (this.gear === 'R' ? -1 : 1);
      const prev = this.speedSmooth;
      this.speedSmooth += (v - this.speedSmooth) * (1 - Math.exp(-step * 8)); // speed arrives in integer km/h steps
      const a = (this.speedSmooth - prev) / step;
      this.autoLong += (MathUtils.clamp(a / g, -1.5, 1.5) - this.autoLong) * (1 - Math.exp(-step * 6));
      longG = this.autoLong;
      latG = MathUtils.clamp((v * v * Math.tan(this.steer * MAX_WHEEL_STEER)) / WHEELBASE / g, -1.2, 1.2);
    }
    const k = this.suspAmount * (1 + 0.6 * this.hover.glow); // hovering: softer, more banking
    const spring = (s: { x: number; v: number }, target: number): void => {
      s.v += (SUSP_K * (target - s.x) - SUSP_C * s.v) * step;
      s.x += s.v * step;
    };
    spring(this.sPitch, -MathUtils.clamp(longG, -1.5, 1.5) * PITCH_PER_G * k);
    spring(this.sRoll, MathUtils.clamp(latG, -1.5, 1.5) * ROLL_PER_G * k);
    spring(this.sHeave, -(Math.abs(longG) * 0.006 + Math.abs(latG) * 0.01) * k);
    const rev = this.maxRpm > 0 ? this.rpm / this.maxRpm : 0;
    const shake = (Math.sin(this.clock * 61) + 0.6 * Math.sin(this.clock * 97)) * 0.0006 * rev * this.shakeAmount;
    this.euler.set(this.sPitch.x, 0, this.sRoll.x);
    this.chassis.rotation.copy(this.euler);
    this.tmpV.copy(SUSP_PIVOT).applyEuler(this.euler);
    const hv = this.hover.glow;
    const bob = hv * (Math.sin(this.clock * 1.9) * 0.012 + Math.sin(this.clock * 0.73) * 0.007) * this.hoverIntensity;
    this.chassis.position.set(
      SUSP_PIVOT.x - this.tmpV.x,
      SUSP_PIVOT.y - this.tmpV.y + this.sHeave.x + shake + this.hover.lift.x + bob,
      SUSP_PIVOT.z - this.tmpV.z,
    );
    this.chassis.rotation.z += hv * Math.sin(this.clock * 0.9) * 0.006 * this.hoverIntensity; // slow sway
  }

  /* --------------------------------------------------------------- rear logo */

  /**
   * Rear logo: a neon emblem that reacts to the music. Its glow (and size, and the PointLight behind it) follows the
   * BASS energy, and every detected beat gives it a kick. Feed the audio with attachAnalyser() (easiest) or
   * setAudioLevels(); with no audio it breathes slowly.
   *
   * @param color      CSS colour, or null to follow the primary neon colour again (default).
   * @param intensity  global brightness multiplier (default 1).
   * @param boost      how much the music adds on top of the idle glow (default 1.6; 0 = no reaction).
   */
  setRearLogo(o: { color?: ColorRepresentation | null; intensity?: number; boost?: number }): void {
    if (o.color === null) this.logoFollowNeon = true;
    else if (o.color !== undefined) {
      this.logoFollowNeon = false;
      this.logoColor.set(o.color);
    }
    if (this.logoFollowNeon) this.logoColor.copy(this.neonPrimary);
    if (o.intensity !== undefined) this.logoIntensity = Math.max(0, o.intensity);
    if (o.boost !== undefined) this.logoBoost = Math.max(0, o.boost);
  }

  /** Drive the logo yourself with a 0..1 level (e.g. your own beat detector). null = back to audio / idle. */
  setLogoLevel(level: number | null): void {
    this.logoManual = level === null ? null : MathUtils.clamp(level, 0, 1);
  }

  /** Current audio energy as seen by the car: bass (0..1, 20-150 Hz) and the logo level (0..1, smoothed). */
  getAudioEnergy(): { bass: number; level: number } {
    return { bass: this.bassTarget, level: this.logoLevel };
  }

  /** Called on each detected bass beat (strength 0..1). Returns an unsubscribe function. */
  onBeat(cb: (strength: number) => void): () => void {
    this.beatCbs.add(cb);
    return () => this.beatCbs.delete(cb);
  }

  private setupLogo(): void {
    this.logoMat = this.mats.get('logo_rear_neon');
    this.logoNode = this.nodes.get('logo_rear');
    if (this.logoNode) this.logoBaseScale.copy(this.logoNode.scale);
  }

  private updateLogo(dt: number): void {
    this.audioFresh = Math.max(0, this.audioFresh - dt);
    const audio = this.analyser !== null || this.audioFresh > 0;
    if (!audio) this.bassTarget *= Math.exp(-dt * 5);
    let target: number;
    if (this.logoManual !== null) target = this.logoManual;
    else if (audio) {
      const b = this.bassTarget;
      this.bassAvg += (b - this.bassAvg) * (1 - Math.exp(-dt * 1.2));
      if (b > this.bassAvg * 1.28 + 0.06 && b > 0.32 && this.clock - this.lastBeat > 0.2) {
        this.lastBeat = this.clock;
        this.logoKick = 1;
        const strength = MathUtils.clamp((b - this.bassAvg) * 2.5, 0, 1);
        for (const cb of this.beatCbs) cb(strength);
      }
      target = MathUtils.clamp(Math.pow(b, 1.6) * 1.15, 0, 1);
    } else target = 0.12 + 0.1 * Math.sin(this.clock * 2);
    this.logoKick = Math.max(0, this.logoKick - dt * 5);
    target = Math.max(target, this.logoKick * 0.9);
    this.logoLevel = target > this.logoLevel ? target : Math.max(target, this.logoLevel - dt * 4); // instant attack, ~0.25 s release
    const L = this.logoLevel;
    const m = this.logoMat;
    if (m) {
      m.emissive.copy(this.logoColor);
      m.color.copy(this.logoColor).multiplyScalar(0.25);
      m.emissiveIntensity = (this.baseEmissive.get('logo_rear_neon') ?? 10) * this.logoIntensity * (0.6 + this.logoBoost * L);
    }
    if (this.logoNode) this.logoNode.scale.copy(this.logoBaseScale).multiplyScalar(1 + 0.05 * L * Math.min(1, this.logoBoost));
    for (const n of this.neonLightList) {
      if (!n.logo) continue;
      n.light.color.copy(this.logoColor);
      n.light.intensity = n.base * this.logoIntensity * (0.5 + 3.2 * L * Math.min(1.5, this.logoBoost)) * (this.neonIntensity / 0.45);
    }
  }

  /* ------------------------------------------------------------------ effects */

  /**
   * Sparks under the car (scraping). intensity 0..1 (0 = off); where: 'front' splitter, 'rear' diffuser,
   * 'sills' (side skirts) or 'all'.
   */
  setSparks(intensity: number, where: 'front' | 'rear' | 'sills' | 'all' = 'all'): void {
    this.sparkRate = MathUtils.clamp(intensity, 0, 1);
    this.sparkWhere = where;
  }

  /** Tyre smoke. intensity 0..1 (use your slip / drift amount); axle: rear wheels only or all four. */
  setTireSmoke(intensity: number, axle: 'rear' | 'all' = 'rear'): void {
    this.smokeRate = MathUtils.clamp(intensity, 0, 1);
    this.smokeAxle = axle;
  }

  setSmokeColor(color: ColorRepresentation): void {
    this.smokeColor.set(color);
  }

  setSparkColor(color: ColorRepresentation): void {
    this.sparkColor.set(color);
  }

  /**
   * Tells the particle shader how big a metre is on screen. Call it on init and on every resize/FOV change:
   * heightPx = renderer height in CSS pixels × pixel ratio, fovDeg = camera.fov.
   */
  setViewport(heightPx: number, fovDeg: number): void {
    const s = heightPx / (2 * Math.tan(MathUtils.degToRad(fovDeg) / 2));
    this.sparks?.setScale(s);
    this.smoke?.setScale(s);
    this.viewScale = s;
  }
  private viewScale = 700;

  private updateEffects(dt: number): void {
    const sp = this.sparks;
    const sm = this.smoke;
    const parent = this.root.parent;
    if (!sp || !sm || !parent) return;
    if (sp.points.parent !== parent) {
      parent.add(sp.points, sm.points);
      sp.setScale(this.viewScale);
      sm.setScale(this.viewScale);
    }
    const tuckN = Math.min(1, this.hover.tuck);
    const sparkK = this.sparkRate * (1 - tuckN);
    const smokeK = this.smokeRate * (1 - tuckN);
    if (sparkK <= 0 && smokeK <= 0 && this.hover.glow <= 0.02 && !sp.active && !sm.active) return;
    const hi = this.quality === 'high';
    this.root.updateWorldMatrix(true, true);
    const v = (this.speedKmh / 3.6) * (this.gear === 'R' ? -1 : 1);
    this.fwd.set(0, 0, 1).transformDirection(this.root.matrixWorld);
    const dx = this.worldScrolls ? -this.fwd.x * v : 0;
    const dz = this.worldScrolls ? -this.fwd.z * v : 0;
    const r = Math.random;

    if (sparkK > 0) {
      this.emitSparkAcc += sparkK * (hi ? 240 : 80) * dt;
      while (this.emitSparkAcc >= 1) {
        this.emitSparkAcc -= 1;
        const where = this.sparkWhere === 'all' ? (['front', 'rear', 'sills'] as const)[Math.floor(r() * 3)] : this.sparkWhere;
        if (where === 'front') this.tmpV.set((r() - 0.5) * 1.3, -0.58, 2.35);
        else if (where === 'rear') this.tmpV.set((r() - 0.5) * 1.3, -0.55, -2.05);
        else this.tmpV.set(r() < 0.5 ? 0.9 : -0.9, -0.5, (r() - 0.5) * 1.4);
        this.chassis.localToWorld(this.tmpV);
        sp.emit({
          x: this.tmpV.x, y: this.tmpV.y, z: this.tmpV.z,
          vx: this.fwd.x * v * 0.3 + (r() - 0.5) * 3, vy: 0.6 + r() * 2.4, vz: this.fwd.z * v * 0.3 + (r() - 0.5) * 3,
          life: 0.25 + r() * 0.4, size0: 0.05 + r() * 0.05, size1: 0.015, alpha: 1, color: this.sparkColor,
        });
      }
    }
    if (smokeK > 0) {
      this.emitSmokeAcc += smokeK * (hi ? 70 : 24) * dt;
      const wheels = this.smokeAxle === 'all' ? ['spin_RL', 'spin_RR', 'steer_FL', 'steer_FR'] : ['spin_RL', 'spin_RR'];
      while (this.emitSmokeAcc >= 1) {
        this.emitSmokeAcc -= 1;
        const w = this.wheelBase.get(wheels[Math.floor(r() * wheels.length)]);
        if (!w) break;
        this.tmpV.set(w.x + (r() - 0.5) * 0.25, w.y - 0.39, w.z + (r() - 0.5) * 0.4);
        this.root.localToWorld(this.tmpV);
        sm.emit({
          x: this.tmpV.x, y: this.tmpV.y, z: this.tmpV.z,
          vx: this.fwd.x * v * 0.5 + (r() - 0.5) * 0.8, vy: 0.5 + r() * 1.1, vz: this.fwd.z * v * 0.5 + (r() - 0.5) * 0.8,
          life: 1.2 + r() * 1.0, size0: 0.2, size1: 0.9 + r() * 0.5, alpha: 0.1 + 0.16 * this.smokeRate, color: this.smokeColor,
        });
      }
    }
    // hover: dust / light mist blown outwards from the ground under each pad
    const gl = this.hover.glow;
    if (gl > 0.3 && this.tucks.length > 0) {
      this.emitDustAcc += (hi ? 30 : 10) * gl * dt;
      const c = this.hoverTint.copy(this.hoverColor ?? this.neonPrimary).lerp(this.smokeColor, 0.35);
      while (this.emitDustAcc >= 1) {
        this.emitDustAcc -= 1;
        const tk = this.tucks[Math.floor(r() * this.tucks.length)];
        const clearance = Math.max(0.05, tk.gy - this.groundY);
        if (clearance > 1.5) continue;
        const ang = r() * 6.2832;
        const spd = 1.2 + r() * 2.2;
        this.tmpV.set(tk.gx, this.groundY + 0.03, tk.gz);
        this.root.localToWorld(this.tmpV);
        sm.emit({
          x: this.tmpV.x, y: this.tmpV.y, z: this.tmpV.z,
          vx: Math.cos(ang) * spd, vy: 0.15 + r() * 0.45, vz: Math.sin(ang) * spd,
          life: 0.8 + r() * 0.7, size0: 0.15, size1: 0.6 + r() * 0.4, alpha: 0.08 * gl * (1.2 - clearance * 0.6), color: c,
        });
      }
    }
    sp.update(dt, -7, 0.6, dx, 0, dz);
    sm.update(dt, 0.25, 0.9, dx, 0, dz);
  }

  /* --------------------------------------------------------------- hover mode */

  /**
   * HOVER MODE ("mode volant"). Timeline (duration 2.4 s by default, reverse order when retracting):
   *   0.00 - 0.40  the four wheels flip flat (rim facing the ground, with a small overshoot)
   *   0.25 - 0.70  the repulsor glow spools up (pad under each wheel, light column, ground ripple, light pool, dust)
   *   0.35 - 0.52  the body dips a few cm (anticipation)
   *   0.45 - 1.00  the body rises to `height` with a springy overshoot, then bobs and sways
   * While flipped, the wheels turn like turbines, the front wheels stop steering, sparks and tyre smoke are muted,
   * the wheels follow the body (pitch / roll / lift) and the dash shows "HOVER".
   * See docs/INTEGRATION.md "Mode volant" for how to wire it into a game.
   */
  setHover(on: boolean, options: { duration?: number } = {}): void {
    this.hover.target = on ? 1 : 0;
    if (options.duration !== undefined) this.hover.duration = Math.max(0.4, options.duration);
  }

  /** true when hover mode has been requested (even if the transition is still running). */
  isHovering(): boolean {
    return this.hover.target === 1;
  }

  /** Current phase, progress and values. Cheap: call it every frame if you need to sync camera / UI / audio. */
  getHoverState(): HoverState {
    const h = this.hover;
    return { phase: h.phase, progress: h.p, tuck: h.tuck, glow: h.glow, height: h.lift.x };
  }

  /** Body lift when fully deployed, metres (default 0.45). Can be changed every frame (e.g. altitude control). */
  setHoverHeight(metres: number): void {
    this.hover.height = MathUtils.clamp(metres, 0.1, 1.6);
  }

  /** Repulsor colour. null = follow the primary neon colour (default). */
  setHoverColor(color: ColorRepresentation | null): void {
    this.hoverColor = color === null ? null : new Color(color);
  }

  /** Glow / bob strength multiplier (default 1, 0 = no glow). */
  setHoverIntensity(k: number): void {
    this.hoverIntensity = Math.max(0, k);
  }

  /** Ground level in car space, metres (default -0.75: where the tyres touch). The ground glow is drawn there. */
  setGroundLevel(y: number): void {
    this.groundY = y;
  }

  /** Called when the transition ends: 'hovering' (deployed) or 'grounded' (retracted). Returns an unsubscribe function. */
  onHoverChange(cb: (phase: HoverPhase) => void): () => void {
    this.hoverCbs.add(cb);
    return () => this.hoverCbs.delete(cb);
  }

  /** Puts the wheels in tuck groups (position = wheel centre) and builds the glow meshes. */
  private setupHover(): void {
    const hi = this.quality === 'high';
    const defs: [string, string][] = [
      ['steer_FL', 'FL'], ['steer_FR', 'FR'], ['spin_RL', 'RL'], ['spin_RR', 'RR'],
    ];
    this.hoverRoot.name = 'hover_fx';
    this.hoverRoot.visible = false;
    this.root.add(this.hoverRoot);
    const col = new Color(0x00e5ff);
    for (const [nodeName, id] of defs) {
      const node = this.nodes.get(nodeName);
      if (!node || !node.parent) continue;
      const base = node.position.clone();
      const sign = base.x > 0 ? 1 : -1;
      const group = new Group();
      group.name = 'tuck_' + id;
      group.position.copy(base);
      node.parent.add(group);
      group.add(node);
      node.position.set(0, 0, 0);
      this.wheelBase.set(nodeName, base);
      // pad on the outer face of the wheel (normal = outward axle, which points down once flipped)
      const padMat = glowMaterial(PAD_FRAG, col);
      const pad = new Mesh(new CircleGeometry(0.36, 40), padMat);
      pad.rotation.y = sign * Math.PI / 2;
      pad.position.set(sign * 0.17, 0, 0);
      pad.frustumCulled = false;
      pad.visible = false;
      group.add(pad);
      this.hoverMats.push(padMat);
      let column: Mesh | undefined;
      let ring: Mesh | undefined;
      if (hi) {
        const cg = new CylinderGeometry(0.3, 0.5, 1, 28, 1, true);
        cg.translate(0, -0.5, 0); // top at y = 0, extends down to y = -1 (scaled to the clearance)
        const colMat = glowMaterial(COLUMN_FRAG, col);
        column = new Mesh(cg, colMat);
        column.frustumCulled = false;
        this.hoverRoot.add(column);
        const rg = new PlaneGeometry(1, 1);
        rg.rotateX(-Math.PI / 2);
        const ringMat = glowMaterial(RING_FRAG, col);
        ring = new Mesh(rg, ringMat);
        ring.frustumCulled = false;
        this.hoverRoot.add(ring);
        this.hoverMats.push(colMat, ringMat);
      }
      this.tucks.push({ id, node, group, base, sign, pad, column, ring, gx: base.x, gy: base.y, gz: base.z });
    }
    const pg = new PlaneGeometry(1, 1);
    pg.rotateX(-Math.PI / 2);
    const poolMat = glowMaterial(POOL_FRAG, col);
    this.pool = new Mesh(pg, poolMat);
    this.pool.frustumCulled = false;
    this.hoverRoot.add(this.pool);
    this.hoverMats.push(poolMat);
  }

  /** Advances the timeline and the lift spring, and fires the phase callbacks. */
  private stepHover(dt: number): void {
    const h = this.hover;
    const step = Math.min(dt, 0.05);
    h.p = MathUtils.clamp(h.p + (h.target ? 1 : -1) * (step / h.duration), 0, 1);
    const smooth = (a: number, b: number, x: number): number => {
      const k = MathUtils.clamp((x - a) / (b - a), 0, 1);
      return k * k * (3 - 2 * k);
    };
    const k = MathUtils.clamp(h.p / 0.4, 0, 1);
    h.tuck = 1 + 2.70158 * Math.pow(k - 1, 3) + 1.70158 * Math.pow(k - 1, 2); // easeOutBack: small overshoot
    h.glow = smooth(0.25, 0.7, h.p) * this.hoverIntensity;
    const dip = smooth(0.35, 0.42, h.p) * (1 - smooth(0.42, 0.52, h.p));
    const target = h.height * smooth(0.45, 1, h.p) - 0.035 * dip;
    h.lift.v += (62 * (target - h.lift.x) - 6.2 * h.lift.v) * step; // light damping: ~6 % overshoot, one visible bounce
    h.lift.x += h.lift.v * step;
    if (h.p <= 0 && Math.abs(h.lift.x) < 0.002) {
      h.lift.x = 0;
      h.lift.v = 0;
    }
    const prev = h.phase;
    if (h.target && h.p >= 1) h.phase = 'hovering';
    else if (!h.target && h.p <= 0) h.phase = 'grounded';
    else h.phase = h.target ? 'deploying' : 'retracting';
    if (h.phase !== prev) {
      this.speedDirty = true; // dash shows / hides the HOVER label
      if (h.phase === 'hovering' || h.phase === 'grounded') for (const cb of this.hoverCbs) cb(h.phase);
    }
  }

  /** Poses the wheels, drives the glow meshes and the lights. Runs after the suspension (needs the final body pose). */
  private applyHover(dt: number): void {
    const h = this.hover;
    const active = h.p > 0 || Math.abs(h.lift.x) > 0.002;
    if (!active && !this.hoverWasActive) return;
    this.hoverWasActive = active;
    const tuckN = Math.min(1, h.tuck);
    const follow = Math.max(tuckN, MathUtils.clamp(h.lift.x / Math.max(0.05, h.height), 0, 1));
    this.chassis.updateMatrix();
    this.root.updateWorldMatrix(true, true);

    for (const tk of this.tucks) {
      const local = this.tmpV.copy(tk.base);
      local.x -= tk.sign * WHEEL_TUCK_IN * tuckN;
      local.y -= WHEEL_TUCK_DROP * tuckN;
      this.tmpV2.copy(local).applyMatrix4(this.chassis.matrix); // same point once carried by the body
      tk.group.position.copy(local).lerp(this.tmpV2, follow);
      this.tmpQ.identity().slerp(this.chassis.quaternion, follow);
      this.tmpQ2.setFromAxisAngle(AXIS_Z, -tk.sign * (Math.PI / 2) * h.tuck); // outer face -> down
      tk.group.quaternion.copy(this.tmpQ).multiply(this.tmpQ2);
    }
    this.root.updateWorldMatrix(true, true);

    const glow = h.glow;
    const show = glow > 0.003;
    this.hoverRoot.visible = show;
    const colA = (this.hoverColor ?? this.neonPrimary);
    const colB = (this.hoverColor ?? this.neonSecondary);
    const react = 0.85 + 0.5 * this.bassTarget; // glow breathes with the bass when music is playing
    const lowK = this.quality === 'high' ? 1 : 2.2; // 'low' has no light columns / ground ripples: stronger pads and pool instead
    const T = this.clock;
    for (const m of this.hoverMats) m.uniforms.uT.value = T;

    for (const tk of this.tucks) {
      tk.pad.visible = show;
      if (!show) continue;
      tk.pad.getWorldPosition(this.tmpV3);
      this.root.worldToLocal(this.tmpV3);
      tk.gx = this.tmpV3.x;
      tk.gy = this.tmpV3.y;
      tk.gz = this.tmpV3.z;
      const clearance = Math.max(0.05, tk.gy - this.groundY);
      const padMat = tk.pad.material as ShaderMaterial;
      padMat.uniforms.uColor.value.copy(colA);
      padMat.uniforms.uA.value = glow * 0.7 * lowK * react;
      if (tk.column) {
        const cm = tk.column.material as ShaderMaterial;
        cm.uniforms.uColor.value.copy(colA);
        cm.uniforms.uA.value = glow * 0.26 * react;
        tk.column.position.set(tk.gx, tk.gy, tk.gz);
        tk.column.scale.set(1, clearance, 1);
      }
      if (tk.ring) {
        const rm = tk.ring.material as ShaderMaterial;
        rm.uniforms.uColor.value.copy(colB);
        rm.uniforms.uA.value = glow * 0.8 * MathUtils.clamp(1.25 - clearance * 1.1, 0.15, 1) * react;
        const radius = 0.55 + 1.4 * clearance;
        tk.ring.position.set(tk.gx, this.groundY + 0.012, tk.gz);
        tk.ring.scale.set(radius * 2, 1, radius * 2);
      }
    }
    if (this.pool) {
      const pm = this.pool.material as ShaderMaterial;
      pm.uniforms.uColor.value.copy(colA).lerp(colB, 0.3);
      pm.uniforms.uA.value = glow * 0.2 * lowK * MathUtils.clamp(1.3 - h.lift.x * 1.2, 0.3, 1) * react;
      this.pool.position.set(0, this.groundY + 0.008, 0.2);
      this.pool.scale.set(2.5 + h.lift.x, 1, 4.9 + h.lift.x * 1.5);
    }

    // the underglow PointLight and the neon strips brighten with the glow
    if (!this.underglow) {
      const u = this.neonLightList[0];
      if (u) this.underglow = { light: u.light, base: u.base };
    }
    if (this.underglow) {
      this.underglow.light.intensity = (this.underglow.base * this.neonIntensity) / 0.45 * (1 + 1.3 * glow * react);
      this.underglow.light.distance = 5 + 2 * glow;
    }
    for (const n of ['neon_primary', 'neon_secondary']) {
      const m = this.mats.get(n);
      if (m) m.emissiveIntensity = (this.baseEmissive.get(n) ?? 4) * this.neonIntensity * (1 + 0.35 * glow);
    }
    if (!active) {
      for (const tk of this.tucks) {
        tk.group.position.copy(tk.base);
        tk.group.quaternion.identity();
      }
      this.hoverRoot.visible = false;
    }
    void dt;
  }

  /* ------------------------------------------------------------------ cleanup */

  dispose(): void {
    this.speedScreen?.dispose();
    this.radioScreen?.dispose();
    this.mirrorScreen?.dispose();
    this.paint?.dispose();
    this.sparks?.dispose();
    this.smoke?.dispose();
    this.rain?.dispose();
    this.hoverRoot.traverse((o) => {
      const m = o as Mesh;
      if (m.isMesh) m.geometry.dispose();
    });
    for (const m of this.hoverMats) m.dispose();
    this.root.traverse((o) => {
      const mesh = o as Mesh;
      if (!mesh.isMesh) return;
      mesh.geometry.dispose();
      const list = Array.isArray(mesh.material) ? mesh.material : [mesh.material];
      for (const m of list) {
        const sm = m as MeshStandardMaterial;
        for (const t of [sm.map, sm.normalMap, sm.roughnessMap, sm.metalnessMap, sm.emissiveMap]) t?.dispose();
        sm.dispose();
      }
    });
    this.root.removeFromParent();
  }
}
