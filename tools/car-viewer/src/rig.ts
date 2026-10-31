import * as THREE from 'three';
import { STEERING_WHEEL_SIGN, WHEEL_RADIUS, WIPER_SIGN } from './config';
import type { AppState, Gear } from './state';

const _q = new THREE.Quaternion();
const _e = new THREE.Euler();

/** Un nœud pivot : on compose toujours la rotation sur l'orientation de repos exportée par Blender. */
class Pivot {
  private readonly rest: THREE.Quaternion;
  constructor(readonly node: THREE.Object3D) {
    this.rest = node.quaternion.clone();
  }
  set(x = 0, y = 0, z = 0): void {
    this.node.quaternion.copy(this.rest).multiply(_q.setFromEuler(_e.set(x, y, z, 'XYZ')));
  }
}

/** Une zone lumineuse de la carrosserie : couleur et force émissive d'origine, modulées par un niveau 0…1. */
class Lamp {
  private readonly color: THREE.Color;
  private readonly strength: number;
  constructor(
    private readonly mat: THREE.MeshStandardMaterial,
    fallback: string,
  ) {
    const authored = mat.emissive.getHex() !== 0;
    this.color = authored ? mat.emissive.clone() : new THREE.Color(fallback);
    this.strength = authored ? Math.max(mat.emissiveIntensity, 0.5) : 4;
  }
  set(level: number): void {
    this.mat.emissive.copy(this.color);
    this.mat.emissiveIntensity = this.strength * level;
  }
}

/** Position du levier dans la grille en H : [avant/arrière (x), gauche/droite (z)] en radians. */
const GEAR_POS: Record<Gear, [number, number]> = {
  N: [0, 0],
  '1': [-0.22, -0.13],
  '2': [0.22, -0.13],
  '3': [-0.22, 0],
  '4': [0.22, 0],
  '5': [-0.22, 0.13],
  '6': [0.22, 0.13],
  R: [-0.22, 0.26],
};
export const GEARS = Object.keys(GEAR_POS) as Gear[];

/** Anime les pièces mobiles d'après l'état de l'interface. Tout nœud absent est simplement ignoré. */
export class CarRig {
  private readonly pivots = new Map<string, Pivot>();
  private readonly mats = new Map<string, THREE.MeshStandardMaterial>();
  private readonly lamps = new Map<string, Lamp>();
  private readonly neonBase = new Map<string, number>();
  private spin = 0;
  private wiperPhase = 0;
  private wiperAngle = 0;
  private lever: [number, number] = [0, 0];

  constructor(
    root: THREE.Object3D,
    private readonly s: AppState,
  ) {
    for (const n of [
      'steer_FL', 'steer_FR', 'spin_FL', 'spin_FR', 'spin_RL', 'spin_RR', 'steering_pivot', 'pivot_gear_lever',
      'pivot_pedal_clutch', 'pivot_pedal_brake', 'pivot_pedal_accel', 'wiperpivot_L', 'wiperpivot_R',
    ]) {
      const o = root.getObjectByName(n);
      if (o) this.pivots.set(n, new Pivot(o));
    }
    root.traverse((o) => {
      const mesh = o as THREE.Mesh;
      if (!mesh.isMesh) return;
      for (const m of Array.isArray(mesh.material) ? mesh.material : [mesh.material]) {
        if ((m as THREE.MeshStandardMaterial).isMeshStandardMaterial) this.mats.set(m.name, m as THREE.MeshStandardMaterial);
      }
    });
    const lamp = (name: string, fallback: string) => {
      const m = this.mats.get(name);
      if (m) this.lamps.set(name, new Lamp(m, fallback));
    };
    lamp('light_head', '#ffffff');
    lamp('light_brake', '#ff1010');
    for (const n of ['signal_FL', 'signal_FR', 'signal_RL', 'signal_RR']) lamp(n, '#ff8a00');
    for (const n of ['neon_primary', 'neon_secondary']) {
      const m = this.mats.get(n);
      if (m) this.neonBase.set(n, m.emissive.getHex() !== 0 ? Math.max(m.emissiveIntensity, 0.5) : 4);
    }
  }

  private p(name: string): Pivot | undefined {
    return this.pivots.get(name);
  }

  update(dt: number, time: number): void {
    const s = this.s;
    // Roues : la vitesse fait tourner autour de l'essieu, le braquage ne concerne que l'avant.
    this.spin += (s.speedKmh / 3.6 / WHEEL_RADIUS) * dt;
    this.spin %= Math.PI * 2;
    for (const n of ['FL', 'FR', 'RL', 'RR']) this.p(`spin_${n}`)?.set(this.spin);
    this.p('steer_FL')?.set(0, s.steering * 0.55);
    this.p('steer_FR')?.set(0, s.steering * 0.55);
    this.p('steering_pivot')?.set(0, STEERING_WHEEL_SIGN * s.steering * 2.4);

    // Pédales : un nombre négatif sur X enfonce la pédale.
    this.p('pivot_pedal_clutch')?.set(-s.clutch * 0.35);
    this.p('pivot_pedal_brake')?.set(-s.brake * 0.35);
    this.p('pivot_pedal_accel')?.set(-s.throttle * 0.35);

    // Levier : rattrape sa cible en douceur, comme un vrai levier.
    const target = GEAR_POS[s.gear];
    const k = 1 - Math.exp(-14 * dt);
    this.lever = [this.lever[0] + (target[0] - this.lever[0]) * k, this.lever[1] + (target[1] - this.lever[1]) * k];
    this.p('pivot_gear_lever')?.set(this.lever[0], 0, this.lever[1]);

    this.updateWipers(dt);
    this.updateLamps(time);
    this.updateColors();
  }

  private updateWipers(dt: number): void {
    const mode = this.s.wipers;
    // Un aller-retour par cycle ; en intermittent, un cycle de 1,2 s toutes les 3 s, repos entre deux.
    const cyclesPerSec = mode === 'fast' ? 1.7 : mode === 'slow' ? 0.9 : 0.34;
    let target = 0;
    if (mode !== 'off') {
      this.wiperPhase += dt * cyclesPerSec;
      const c = this.wiperPhase % 1;
      const run = mode === 'intermittent' ? (c < 0.4 ? c / 0.4 : -1) : c;
      if (run >= 0) target = 1.35 * (0.5 - 0.5 * Math.cos(run * Math.PI * 2));
    }
    // Hors balayage (arrêt, pause), les balais reviennent au repos.
    this.wiperAngle += (target - this.wiperAngle) * (1 - Math.exp(-30 * dt));
    const angle = WIPER_SIGN * this.wiperAngle;
    this.p('wiperpivot_L')?.set(0, angle);
    this.p('wiperpivot_R')?.set(0, angle);
  }

  private updateLamps(time: number): void {
    const s = this.s;
    const braking = s.braking || s.brake > 0.05;
    this.lamps.get('light_head')?.set(s.headlights ? 1 : 0);
    // Feux arrière en veilleuse quand les phares sont allumés, pleins au freinage.
    this.lamps.get('light_brake')?.set(braking ? 1 : s.headlights ? 0.25 : 0);
    const blink = (time * 1.5) % 1 < 0.5;
    const left = blink && (s.blinker === 'left' || s.blinker === 'hazard');
    const right = blink && (s.blinker === 'right' || s.blinker === 'hazard');
    // Côté gauche = +X : FL et RL.
    this.lamps.get('signal_FL')?.set(left ? 1 : 0);
    this.lamps.get('signal_RL')?.set(left ? 1 : 0);
    this.lamps.get('signal_FR')?.set(right ? 1 : 0);
    this.lamps.get('signal_RR')?.set(right ? 1 : 0);
  }

  private updateColors(): void {
    const s = this.s;
    for (const [name, hex] of [['neon_primary', s.neonPrimary], ['neon_secondary', s.neonSecondary]] as const) {
      const m = this.mats.get(name);
      if (!m) continue;
      m.emissive.set(hex);
      m.emissiveIntensity = (this.neonBase.get(name) ?? 4) * s.neonIntensity;
    }
    const glass = this.mats.get('glass');
    if (glass) {
      glass.transparent = true;
      glass.opacity = s.glassOpacity;
    }
  }
}
