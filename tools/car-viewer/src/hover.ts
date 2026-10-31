import * as THREE from 'three';
import type { AppState } from './state';

export type HoverPhase = 'grounded' | 'deploying' | 'hovering' | 'retracting';

const CORNERS = ['FL', 'FR', 'RL', 'RR'] as const;
const DURATION = 2.4;

const clamp01 = (x: number) => Math.min(1, Math.max(0, x));
const smooth = (x: number) => clamp01(x) * clamp01(x) * (3 - 2 * clamp01(x));
/** Dépassement léger puis retour : le « ressort » de la chronologie de CYBERCAR.md §11. */
const backOut = (x: number, c1: number) => {
  const t = clamp01(x) - 1;
  return 1 + (c1 + 1) * t * t * t + c1 * t * t;
};

interface Corner {
  tuck: THREE.Group;
  base: THREE.Vector3;
  side: number;
  column: THREE.Mesh;
  pad: THREE.Mesh;
  ripple: THREE.Mesh;
}

/**
 * Mode volant : réorganise le modèle en « châssis » (tout sauf les roues) et en 4 groupes de roues, puis rejoue la
 * chronologie de CYBERCAR.md §11 (roues à plat, lueur, affaissement, montée). C'est une réécriture pour valider le
 * modèle, pas le code de référence.
 */
export class Hover {
  readonly chassis = new THREE.Group();
  readonly fx = new THREE.Group();
  /** Faux si les roues ne sont pas isolables dans le modèle : le décollage est alors désactivé. */
  readonly supported: boolean;
  progress = 0;
  glow = 0;
  private readonly corners: Corner[] = [];
  private readonly puddle: THREE.Mesh;
  private readonly mats: THREE.MeshBasicMaterial[] = [];

  constructor(
    root: THREE.Object3D,
    private readonly s: AppState,
    private readonly groundY: number,
  ) {
    root.updateMatrixWorld(true);
    const tops = new Map<string, THREE.Object3D>();
    for (const c of CORNERS) {
      const node = root.getObjectByName(c.startsWith('F') ? `steer_${c}` : `spin_${c}`);
      let top = node ?? null;
      while (top?.parent && top.parent !== root) top = top.parent;
      if (top && top !== root) tops.set(c, top);
    }
    this.supported = tops.size === 4 && new Set(tops.values()).size === 4;

    const box = new THREE.Box3().setFromObject(root);
    const size = box.getSize(new THREE.Vector3());
    const center = box.getCenter(new THREE.Vector3());
    const glowMat = () => {
      const m = new THREE.MeshBasicMaterial({
        color: 0xffffff, transparent: true, opacity: 0, blending: THREE.AdditiveBlending, depthWrite: false, side: THREE.DoubleSide,
      });
      this.mats.push(m);
      return m;
    };

    if (this.supported) {
      for (const c of CORNERS) {
        const top = tops.get(c)!;
        const axle = root.getObjectByName(`spin_${c}`)!.getWorldPosition(new THREE.Vector3());
        const tuck = new THREE.Group();
        tuck.name = `tuck_${c}`;
        tuck.position.copy(axle);
        root.add(tuck);
        tuck.attach(top);
        const column = new THREE.Mesh(new THREE.CylinderGeometry(0.16, 0.26, 1, 20, 1, true), glowMat());
        const pad = new THREE.Mesh(new THREE.RingGeometry(0.12, 0.34, 28, 1), glowMat());
        pad.rotation.x = -Math.PI / 2;
        const ripple = new THREE.Mesh(new THREE.RingGeometry(0.92, 1, 40, 1), glowMat());
        ripple.rotation.x = -Math.PI / 2;
        this.fx.add(column, pad, ripple);
        this.corners.push({ tuck, base: axle.clone(), side: axle.x >= 0 ? 1 : -1, column, pad, ripple });
      }
    }
    // Tout le reste (carrosserie, habitacle, repères, caméra) monte et bouge ensemble.
    const tuckSet = new Set(this.corners.map((c) => c.tuck));
    for (const child of [...root.children]) if (!tuckSet.has(child as THREE.Group)) this.chassis.add(child);
    root.add(this.chassis);

    this.puddle = new THREE.Mesh(new THREE.CircleGeometry(1, 48), glowMat());
    this.puddle.rotation.x = -Math.PI / 2;
    this.puddle.scale.set(size.x * 1.5, size.z * 0.75, 1);
    this.puddle.position.set(center.x, groundY + 0.012, center.z);
    this.fx.add(this.puddle);
    this.fx.visible = false;
    root.add(this.fx);
  }

  get phase(): HoverPhase {
    if (this.progress <= 0 && !this.s.hoverOn) return 'grounded';
    if (this.progress >= 1 && this.s.hoverOn) return 'hovering';
    return this.s.hoverOn ? 'deploying' : 'retracting';
  }

  /** Hauteur actuelle de la caisse (m). */
  get height(): number {
    return this.chassis.position.y;
  }

  update(dt: number, time: number, primary: THREE.Color, secondary: THREE.Color): void {
    const s = this.s;
    const want = this.supported && s.hoverOn;
    this.progress = clamp01(this.progress + (want ? dt : -dt) / DURATION);
    const p = this.progress;
    const tuck = backOut(p / 0.4, 0.8);
    this.glow = smooth((p - 0.25) / 0.45);
    const lift = backOut((p - 0.45) / 0.55, 1.2);
    const sag = Math.sin(clamp01((p - 0.35) / 0.17) * Math.PI) * 0.03;
    const idle = p >= 1 ? 1 : 0;
    const y = lift * s.hoverHeight - sag + idle * Math.sin(time * 1.6) * 0.012 * s.hoverIntensity;
    this.chassis.position.y = y;
    this.chassis.rotation.z = idle * Math.sin(time * 0.9) * 0.012 * s.hoverIntensity;

    this.fx.visible = this.glow > 0.001;
    const color = s.hoverFollowNeon ? primary : new THREE.Color(s.hoverColor);
    const ringColor = s.hoverFollowNeon ? secondary : color;
    const g = this.glow * s.hoverIntensity;
    for (const c of this.corners) {
      // Roue à plat (face extérieure vers le sol), 22 cm plus bas et 10 cm vers l'intérieur.
      c.tuck.rotation.z = -c.side * (Math.PI / 2) * tuck;
      c.tuck.position.set(c.base.x - c.side * 0.1 * tuck, c.base.y + y - 0.22 * tuck, c.base.z);
      const top = c.tuck.position.y;
      const h = Math.max(top - this.groundY, 0.01);
      c.column.scale.set(1, h, 1);
      c.column.position.set(c.tuck.position.x, this.groundY + h / 2, c.tuck.position.z);
      c.pad.position.set(c.tuck.position.x, this.groundY + 0.02, c.tuck.position.z);
      c.pad.rotation.z = time * 1.5;
      const r = (time * 0.8 + (c.side > 0 ? 0 : 0.5) + (c.base.z > 0 ? 0 : 0.25)) % 1;
      c.ripple.position.set(c.tuck.position.x, this.groundY + 0.015, c.tuck.position.z);
      c.ripple.scale.setScalar(0.3 + r * 1.6);
      (c.column.material as THREE.MeshBasicMaterial).color.copy(color);
      (c.column.material as THREE.MeshBasicMaterial).opacity = 0.22 * g;
      (c.pad.material as THREE.MeshBasicMaterial).color.copy(color);
      (c.pad.material as THREE.MeshBasicMaterial).opacity = 0.9 * g;
      (c.ripple.material as THREE.MeshBasicMaterial).color.copy(ringColor);
      (c.ripple.material as THREE.MeshBasicMaterial).opacity = (1 - r) * 0.8 * g;
    }
    const pm = this.puddle.material as THREE.MeshBasicMaterial;
    pm.color.copy(color);
    pm.opacity = 0.35 * g;
  }

  dispose(): void {
    for (const c of this.corners) {
      c.column.geometry.dispose();
      c.pad.geometry.dispose();
      c.ripple.geometry.dispose();
    }
    this.puddle.geometry.dispose();
    for (const m of this.mats) m.dispose();
  }
}
