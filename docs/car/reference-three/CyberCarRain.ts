/**
 * Rain droplets on the windscreen. Drops are drawn on a canvas that is mapped onto the `rain_windshield`
 * mesh (a copy of the glass, 5 mm above it). Wipers really clear the drops they sweep over.
 *
 * Geometry-independent: the UV <-> mesh mapping is fitted from the mesh vertices at start-up.
 */
import {
  CanvasTexture,
  DoubleSide,
  Matrix4,
  type Mesh,
  type MeshStandardMaterial,
  type Object3D,
  Quaternion,
  SRGBColorSpace,
  Vector3,
} from 'three';
import type { Quality } from './CyberCar';

export interface WiperSweep {
  pivot: Object3D;
  /** resting orientation of the pivot (quaternion captured at load). */
  base: Quaternion;
  /** arm length in metres. */
  length: number;
}

interface Drop {
  u: number; v: number;
  r: number;          // radius, px
  speed: number;      // px/s, 0 = clinging
  life: number;       // seconds left when rain stops
}

interface Affine {
  // pivot-rest-frame position = b + au*u + av*v
  b: Vector3; au: Vector3; av: Vector3;
}

export class WindshieldRain {
  private readonly canvas = document.createElement('canvas');
  private readonly ctx: CanvasRenderingContext2D;
  private readonly texture: CanvasTexture;
  private readonly drops: Drop[] = [];
  private readonly max: number;
  private readonly W: number;
  private readonly H: number;
  private readonly downV: number; // +1 / -1: direction of "down the glass" in v
  private readonly uvToLocal: (u: number, v: number, out: Vector3) => Vector3;
  private affine: Affine[] | null = null;
  private acc = 0;
  private spawn = 0;
  private lastAngle: number[] = [];
  private readonly step: number;

  constructor(
    private readonly mesh: Mesh,
    material: MeshStandardMaterial,
    private readonly wipers: WiperSweep[],
    quality: Quality,
  ) {
    const hi = quality === 'high';
    this.W = hi ? 512 : 256;
    this.H = hi ? 256 : 128;
    this.max = hi ? 150 : 60;
    this.step = hi ? 1 / 24 : 1 / 12;
    this.canvas.width = this.W;
    this.canvas.height = this.H;
    this.ctx = this.canvas.getContext('2d')!;
    this.texture = new CanvasTexture(this.canvas);
    this.texture.flipY = false; // glTF UV convention
    this.texture.colorSpace = SRGBColorSpace;
    material.map = this.texture;
    material.color.set(0xffffff);
    material.opacity = 1;
    material.transparent = true;
    material.alphaTest = 0; // the GLB exports this material as alphaMode MASK (cutoff 0.5): drops are semi-transparent
    material.depthWrite = false;
    material.side = DoubleSide;
    material.roughness = 0.05;
    material.metalness = 0;
    material.needsUpdate = true;
    mesh.visible = false;

    // ---- fit uv <-> local position from the mesh vertices (extreme points along x and z)
    const g = mesh.geometry;
    const p = g.attributes.position;
    const t = g.attributes.uv;
    let iMinX = 0, iMaxX = 0, iMinZ = 0, iMaxZ = 0;
    for (let i = 1; i < p.count; i++) {
      if (p.getX(i) < p.getX(iMinX)) iMinX = i;
      if (p.getX(i) > p.getX(iMaxX)) iMaxX = i;
      if (p.getZ(i) < p.getZ(iMinZ)) iMinZ = i;
      if (p.getZ(i) > p.getZ(iMaxZ)) iMaxZ = i;
    }
    const dx = p.getX(iMaxX) - p.getX(iMinX) || 1;
    const dz = p.getZ(iMaxZ) - p.getZ(iMinZ) || 1;
    const kuX = (t.getX(iMaxX) - t.getX(iMinX)) / dx; // du / dx
    const kvZ = (t.getY(iMaxZ) - t.getY(iMinZ)) / dz; // dv / dz
    const kyZ = (p.getY(iMaxZ) - p.getY(iMinZ)) / dz; // dy / dz (glass slope)
    this.downV = Math.sign(kvZ) || 1; // +z is "down" the glass (the base of the windscreen is at larger z)
    this.uvToLocal = (u, v, out) => {
      const x = p.getX(iMinX) + (u - t.getX(iMinX)) / (kuX || 1);
      const z = p.getZ(iMinZ) + (v - t.getY(iMinZ)) / (kvZ || 1);
      return out.set(x, p.getY(iMinZ) + (z - p.getZ(iMinZ)) * kyZ, z);
    };
    this.lastAngle = wipers.map(() => 0);
  }

  /** Builds the uv -> wiper-frame affine maps (once the scene graph has its final transforms). */
  private buildAffine(): void {
    this.mesh.updateWorldMatrix(true, false);
    this.affine = this.wipers.map((w) => {
      w.pivot.parent?.updateWorldMatrix(true, false);
      const inv = new Matrix4().copy(w.pivot.parent!.matrixWorld).invert().multiply(this.mesh.matrixWorld);
      const qi = w.base.clone().invert();
      const f = (u: number, v: number): Vector3 =>
        this.uvToLocal(u, v, new Vector3()).applyMatrix4(inv).sub(w.pivot.position).applyQuaternion(qi);
      const b = f(0, 0);
      return { b, au: f(1, 0).sub(b), av: f(0, 1).sub(b) };
    });
  }

  /**
   * @param intensity 0..1 rain intensity (0 lets the drops dry out)
   * @param angles current wiper angle (rad) for each wiper, same order as `wipers`
   */
  update(dt: number, intensity: number, angles: number[]): void {
    const rainy = intensity > 0.02;
    if (!this.affine) this.buildAffine();
    const aff = this.affine!;

    // wipers: remove every drop between the previous and the current arm angle
    for (let w = 0; w < this.wipers.length; w++) {
      const a0 = Math.min(this.lastAngle[w], angles[w]) - 0.05;
      const a1 = Math.max(this.lastAngle[w], angles[w]) + 0.05;
      this.lastAngle[w] = angles[w];
      if (a1 - a0 > 0.11 || Math.abs(angles[w]) > 1e-4) {
        const { b, au, av } = aff[w];
        for (let i = this.drops.length - 1; i >= 0; i--) {
          const d = this.drops[i];
          const x = b.x + au.x * d.u + av.x * d.v;
          const z = b.z + au.z * d.u + av.z * d.v;
          const ang = Math.atan2(-z, x);
          if (ang >= a0 && ang <= a1 && Math.hypot(x, z) <= this.wipers[w].length + 0.02) this.drops.splice(i, 1);
        }
      }
    }

    this.acc += dt;
    if (this.acc < this.step) return;
    const el = this.acc;
    this.acc = 0;

    // spawn
    if (rainy) {
      this.spawn += intensity * (this.max > 100 ? 70 : 28) * el;
      while (this.spawn >= 1 && this.drops.length < this.max) {
        this.spawn -= 1;
        const big = Math.random() < 0.12;
        this.drops.push({ u: Math.random(), v: Math.random(), r: big ? 4.5 + Math.random() * 3 : 1.4 + Math.random() * 2.6, speed: 0, life: 1 });
      }
      this.spawn = Math.min(this.spawn, 1);
    }

    // simulate
    const h = this.H;
    for (let i = this.drops.length - 1; i >= 0; i--) {
      const d = this.drops[i];
      if (rainy) {
        d.r = Math.min(9, d.r + el * 0.35 * intensity);
        if (d.r > 6.2 && d.speed === 0) d.speed = 14 + Math.random() * 28;
      } else {
        d.life -= el * 0.35; // dry out
        if (d.life <= 0) {
          this.drops.splice(i, 1);
          continue;
        }
      }
      if (d.speed > 0) {
        d.v += (this.downV * d.speed * el) / h;
        d.speed *= 0.97 + Math.random() * 0.05;
        d.r = Math.max(2.2, d.r - el * 0.5);
        if (d.speed < 5) d.speed = 0;
        if (d.v > 1.02 || d.v < -0.02) this.drops.splice(i, 1);
      }
    }
    this.draw();
    this.mesh.visible = this.drops.length > 0;
  }

  private draw(): void {
    const { ctx, W, H } = this;
    ctx.clearRect(0, 0, W, H);
    for (const d of this.drops) {
      const x = d.u * W;
      const y = d.v * H;
      const a = Math.min(1, d.life + 0.2);
      if (d.speed > 0) {
        ctx.fillStyle = `rgba(190,215,235,${0.07 * a})`;
        const len = Math.min(26, d.speed * 0.6);
        ctx.fillRect(x - d.r * 0.3, y - this.downV * len, d.r * 0.6, len);
      }
      ctx.fillStyle = `rgba(175,205,230,${0.28 * a})`;
      ctx.beginPath();
      ctx.arc(x, y, d.r, 0, 6.2832);
      ctx.fill();
      ctx.strokeStyle = `rgba(235,248,255,${0.55 * a})`;
      ctx.lineWidth = 0.8;
      ctx.beginPath();
      ctx.arc(x, y, d.r, 3.5, 5.2); // lit upper-left edge
      ctx.stroke();
      ctx.fillStyle = `rgba(255,255,255,${0.9 * a})`;
      ctx.fillRect(x - d.r * 0.45, y - d.r * 0.45, Math.max(1, d.r * 0.3), Math.max(1, d.r * 0.3));
    }
    this.texture.needsUpdate = true;
  }

  get dropCount(): number {
    return this.drops.length;
  }

  dispose(): void {
    this.texture.dispose();
  }
}
