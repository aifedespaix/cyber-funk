/**
 * Tiny GPU-friendly particle pool (one draw call) used for sparks and tyre smoke.
 *
 * Particles live in WORLD space (added next to the car in the scene), so they stay where they were emitted
 * when the car moves. If the app keeps the car still and scrolls the world instead, pass a per-frame
 * `drift` velocity (the ground velocity relative to the car) to `update()`.
 */
import {
  AdditiveBlending,
  BufferAttribute,
  BufferGeometry,
  Color,
  NormalBlending,
  Points,
  ShaderMaterial,
} from 'three';

export interface EmitOptions {
  x: number; y: number; z: number;
  vx: number; vy: number; vz: number;
  life: number;
  size0: number; size1: number;
  alpha: number;
  color: Color;
}

const VERT = /* glsl */ `
attribute float aSize; attribute float aAlpha; attribute vec3 aColor;
uniform float uScale;
varying float vA; varying vec3 vC;
void main() {
  vec4 mv = modelViewMatrix * vec4(position, 1.0);
  gl_PointSize = max(aSize * uScale / max(-mv.z, 0.1), 0.0);
  gl_Position = projectionMatrix * mv;
  vA = aAlpha; vC = aColor;
}`;
const FRAG = /* glsl */ `
uniform float uCore;
varying float vA; varying vec3 vC;
void main() {
  float d = length(gl_PointCoord - 0.5) * 2.0;
  if (d > 1.0) discard;
  float a = uCore > 0.5 ? pow(1.0 - d, 3.0) : exp(-d * d * 3.4); // sparks: hot core, smoke: soft gaussian
  gl_FragColor = vec4(vC, vA * a);
}`;

export class ParticlePool {
  readonly points: Points;
  private readonly max: number;
  private readonly pos: Float32Array;
  private readonly col: Float32Array;
  private readonly alpha: Float32Array;
  private readonly size: Float32Array;
  private readonly vel: Float32Array;
  private readonly life: Float32Array;
  private readonly maxLife: Float32Array;
  private readonly s0: Float32Array;
  private readonly s1: Float32Array;
  private readonly a0: Float32Array;
  private readonly c0: Float32Array;
  private readonly uniforms = { uScale: { value: 600 }, uCore: { value: 0 } };
  private cursor = 0;
  private alive = 0;

  constructor(max: number, additive: boolean) {
    this.max = max;
    this.pos = new Float32Array(max * 3);
    this.col = new Float32Array(max * 3);
    this.alpha = new Float32Array(max);
    this.size = new Float32Array(max);
    this.vel = new Float32Array(max * 3);
    this.life = new Float32Array(max);
    this.maxLife = new Float32Array(max).fill(1);
    this.s0 = new Float32Array(max);
    this.s1 = new Float32Array(max);
    this.a0 = new Float32Array(max);
    this.c0 = new Float32Array(max * 3);
    const g = new BufferGeometry();
    g.setAttribute('position', new BufferAttribute(this.pos, 3));
    g.setAttribute('aColor', new BufferAttribute(this.col, 3));
    g.setAttribute('aAlpha', new BufferAttribute(this.alpha, 1));
    g.setAttribute('aSize', new BufferAttribute(this.size, 1));
    this.uniforms.uCore.value = additive ? 1 : 0;
    const mat = new ShaderMaterial({
      uniforms: this.uniforms,
      vertexShader: VERT,
      fragmentShader: FRAG,
      transparent: true,
      depthWrite: false,
      blending: additive ? AdditiveBlending : NormalBlending,
    });
    this.points = new Points(g, mat);
    this.points.frustumCulled = false;
    this.points.matrixAutoUpdate = false;
  }

  /** pixels per metre at distance 1 for the current camera (viewport height / (2 tan(fov/2))). */
  setScale(pixelsPerMetre: number): void {
    this.uniforms.uScale.value = pixelsPerMetre;
  }

  get active(): boolean {
    return this.alive > 0;
  }

  emit(o: EmitOptions): void {
    const i = this.cursor;
    this.cursor = (this.cursor + 1) % this.max;
    this.pos[i * 3] = o.x; this.pos[i * 3 + 1] = o.y; this.pos[i * 3 + 2] = o.z;
    this.vel[i * 3] = o.vx; this.vel[i * 3 + 1] = o.vy; this.vel[i * 3 + 2] = o.vz;
    this.life[i] = this.maxLife[i] = o.life;
    this.s0[i] = o.size0; this.s1[i] = o.size1; this.a0[i] = o.alpha;
    this.c0[i * 3] = o.color.r; this.c0[i * 3 + 1] = o.color.g; this.c0[i * 3 + 2] = o.color.b;
  }

  /** gravity in m/s² (negative = down), drag 1/s, drift = ground velocity relative to the car. */
  update(dt: number, gravity: number, drag: number, driftX = 0, driftY = 0, driftZ = 0): void {
    let alive = 0;
    const k = Math.exp(-drag * dt);
    for (let i = 0; i < this.max; i++) {
      if (this.life[i] <= 0) {
        this.alpha[i] = 0;
        this.size[i] = 0;
        continue;
      }
      this.life[i] -= dt;
      if (this.life[i] <= 0) {
        this.alpha[i] = 0;
        this.size[i] = 0;
        continue;
      }
      alive++;
      const t = 1 - this.life[i] / this.maxLife[i]; // 0 -> 1
      this.vel[i * 3] *= k; this.vel[i * 3 + 1] = this.vel[i * 3 + 1] * k + gravity * dt; this.vel[i * 3 + 2] *= k;
      this.pos[i * 3] += (this.vel[i * 3] + driftX) * dt;
      this.pos[i * 3 + 1] += (this.vel[i * 3 + 1] + driftY) * dt;
      this.pos[i * 3 + 2] += (this.vel[i * 3 + 2] + driftZ) * dt;
      this.size[i] = this.s0[i] + (this.s1[i] - this.s0[i]) * t;
      this.alpha[i] = this.a0[i] * (1 - t) * Math.min(1, t * 8 + 0.2);
      // hot sparks cool from white to their colour
      const heat = Math.max(0, 1 - t * 2.2);
      this.col[i * 3] = this.c0[i * 3] + (1 - this.c0[i * 3]) * heat * this.uniforms.uCore.value;
      this.col[i * 3 + 1] = this.c0[i * 3 + 1] + (1 - this.c0[i * 3 + 1]) * heat * this.uniforms.uCore.value;
      this.col[i * 3 + 2] = this.c0[i * 3 + 2] + (1 - this.c0[i * 3 + 2]) * heat * this.uniforms.uCore.value;
    }
    this.alive = alive;
    this.points.geometry.attributes.position.needsUpdate = true;
    this.points.geometry.attributes.aColor.needsUpdate = true;
    this.points.geometry.attributes.aAlpha.needsUpdate = true;
    this.points.geometry.attributes.aSize.needsUpdate = true;
  }

  dispose(): void {
    this.points.geometry.dispose();
    (this.points.material as ShaderMaterial).dispose();
    this.points.removeFromParent();
  }
}
