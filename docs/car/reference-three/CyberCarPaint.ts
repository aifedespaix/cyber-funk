/**
 * Paint system for CyberCar: real body-paint mask, metal, clear coat, flakes, iridescence, neon rim light.
 *
 * - The GLB embeds a paint mask as the OCCLUSION texture (R channel) of `body_paint` (see tools/build-glb.sh).
 *   Only masked pixels (the bodywork panels) are recoloured / made metallic; tyres, trims, lights are untouched.
 * - `body_paint` is upgraded from MeshStandardMaterial to MeshPhysicalMaterial (clear coat + iridescence).
 *   On the 'low' quality tier clear coat and iridescence stay off (cheaper shader).
 * - If an older GLB has no mask, a hue-based fallback (violet) is used.
 */
import {
  Color,
  type ColorRepresentation,
  DataTexture,
  MathUtils,
  Mesh,
  MeshPhysicalMaterial,
  type MeshStandardMaterial,
  type Object3D,
  RGBAFormat,
  type Texture,
  Vector3,
} from 'three';
import type { Quality } from './CyberCar';

export type PaintFinishName = 'gloss' | 'metallic' | 'matte' | 'chrome' | 'candy' | 'holo';

export interface PaintFinish {
  /** 0..1. Metalness inside the paint mask. */
  metalness: number;
  /** 0..1. Base roughness inside the paint mask (texture wear still modulates it a little). */
  roughness: number;
  /** 0..1. Clear-coat layer (high quality only). */
  clearcoat: number;
  /** 0..1. Clear-coat roughness (0 = mirror). */
  clearcoatRoughness: number;
  /** 0..1. Metallic flake sparkle. */
  flake: number;
  /** 0..1. Thin-film colour shift (high quality only). */
  iridescence: number;
  /** 0..2. Neon rim light on the paint silhouette, in the neon colours. */
  rim: number;
}

export const PAINT_FINISHES: Record<PaintFinishName, PaintFinish> = {
  gloss: { metalness: 0.35, roughness: 0.28, clearcoat: 1, clearcoatRoughness: 0.04, flake: 0.15, iridescence: 0, rim: 0.3 },
  metallic: { metalness: 0.85, roughness: 0.32, clearcoat: 1, clearcoatRoughness: 0.06, flake: 0.6, iridescence: 0, rim: 0.35 },
  matte: { metalness: 0.2, roughness: 0.78, clearcoat: 0, clearcoatRoughness: 0.4, flake: 0.05, iridescence: 0, rim: 0.55 },
  chrome: { metalness: 1, roughness: 0.07, clearcoat: 0, clearcoatRoughness: 0, flake: 0, iridescence: 0, rim: 0.1 },
  candy: { metalness: 0.9, roughness: 0.16, clearcoat: 1, clearcoatRoughness: 0.02, flake: 0.35, iridescence: 0.2, rim: 0.4 },
  holo: { metalness: 0.9, roughness: 0.22, clearcoat: 1, clearcoatRoughness: 0.03, flake: 0.5, iridescence: 1, rim: 0.45 },
};

/** Original paint (violet) measured in the diffuse texture: hue 0.76, sat 0.44, val 0.49. */
const PAINT_REF = { h: 0.76, s: 0.44, v: 0.49 };

const GLSL_FN = /* glsl */ `
uniform vec3 uPaint;        // x = target hue, y = saturation scale, z = value scale
uniform sampler2D uMask;
uniform float uMaskOn;
uniform float uRough, uMetal, uFlake, uRim;
uniform vec3 uRimA, uRimB;
float cc_pm = 0.0;
float cc_hash(vec2 p){ return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
vec3 cc_rgb2hsv(vec3 c){
  vec4 K=vec4(0.,-1./3.,2./3.,-1.); vec4 p=mix(vec4(c.bg,K.wz),vec4(c.gb,K.xy),step(c.b,c.g));
  vec4 q=mix(vec4(p.xyw,c.r),vec4(c.r,p.yzx),step(p.x,c.r)); float d=q.x-min(q.w,q.y);
  return vec3(abs(q.z+(q.w-q.y)/(6.*d+1e-10)), d/(q.x+1e-10), q.x);
}
vec3 cc_hsv2rgb(vec3 c){
  vec4 K=vec4(1.,2./3.,1./3.,3.); vec3 p=abs(fract(c.xxx+K.xyz)*6.-K.www);
  return c.z*mix(K.xxx,clamp(p-K.xxx,0.,1.),c.y);
}
float cc_hueMask(vec3 lin){
  vec3 h = cc_rgb2hsv(pow(max(lin, 0.0), vec3(1.0/2.2)));
  return smoothstep(0.66,0.70,h.x) * (1.0 - smoothstep(0.82,0.86,h.x)) * smoothstep(0.15,0.30,h.y);
}
`;

const GLSL_COLOR = /* glsl */ `
#ifdef USE_MAP
  vec2 cc_uv = vMapUv;
#else
  vec2 cc_uv = vec2(0.0);
#endif
cc_pm = uMaskOn > 0.5 ? texture2D(uMask, cc_uv).r : cc_hueMask(diffuseColor.rgb);
{
  vec3 hsv_ = cc_rgb2hsv(pow(max(diffuseColor.rgb, 0.0), vec3(1.0/2.2)));
  vec3 t_ = cc_hsv2rgb(vec3(uPaint.x, clamp(hsv_.y*uPaint.y,0.,1.), clamp(hsv_.z*uPaint.z,0.,1.)));
  diffuseColor.rgb = mix(diffuseColor.rgb, pow(t_, vec3(2.2)), cc_pm);
}
`;

const GLSL_PBR = /* glsl */ `
roughnessFactor = mix(roughnessFactor, clamp(uRough + (roughnessFactor - 0.5) * 0.5, 0.03, 1.0), cc_pm);
metalnessFactor = mix(metalnessFactor, clamp(uMetal + (metalnessFactor - 0.5) * 0.3, 0.0, 1.0), cc_pm);
`;

const GLSL_EMISSIVE = /* glsl */ `
{
  vec3 V_ = normalize(vViewPosition);
  vec3 N_ = normalize(normal);
  float ndv_ = clamp(dot(N_, V_), 0.0, 1.0);
  float rim_ = pow(1.0 - ndv_, 3.0);
  totalEmissiveRadiance += mix(uRimA, uRimB, smoothstep(-0.4, 0.4, N_.x)) * rim_ * uRim * cc_pm;
  if (uFlake > 0.001) {
    vec2 cell_ = floor(cc_uv * 560.0);
    float h0_ = cc_hash(cell_);
    vec3 fn_ = normalize(N_ + (vec3(cc_hash(cell_ + 7.3), cc_hash(cell_ + 19.1), cc_hash(cell_ + 3.7)) - 0.5) * 0.9);
    float sp_ = step(0.82, h0_) * pow(max(dot(fn_, V_), 0.0), 140.0);
    totalEmissiveRadiance += mix(vec3(1.0), diffuseColor.rgb * 3.0, 0.5) * sp_ * uFlake * 0.55 * cc_pm;
  }
}
`;

const GLSL_COAT = /* glsl */ `
#ifdef USE_CLEARCOAT
  material.clearcoat *= cc_pm;
#endif
#ifdef USE_IRIDESCENCE
  material.iridescence *= cc_pm;
#endif
`;

function rgbToHsv(c: Color): { h: number; s: number; v: number } {
  const max = Math.max(c.r, c.g, c.b);
  const d = max - Math.min(c.r, c.g, c.b);
  let h = 0;
  if (d > 1e-6) {
    if (max === c.r) h = ((c.g - c.b) / d + 6) % 6;
    else if (max === c.g) h = (c.b - c.r) / d + 2;
    else h = (c.r - c.g) / d + 4;
    h /= 6;
  }
  return { h, s: max === 0 ? 0 : d / max, v: max };
}

export class PaintSystem {
  readonly material: MeshPhysicalMaterial;
  private readonly full: boolean;
  private readonly maskTexture: Texture;
  private finish: PaintFinish = { ...PAINT_FINISHES.metallic };
  private readonly u = {
    uPaint: { value: new Vector3(PAINT_REF.h, 1, 1) },
    uMask: { value: null as Texture | null },
    uMaskOn: { value: 0 },
    uRough: { value: 0.32 },
    uMetal: { value: 0.85 },
    uFlake: { value: 0.6 },
    uRim: { value: 0.35 },
    uRimA: { value: new Color(0x00e5ff) },
    uRimB: { value: new Color(0xff0d73) },
  };

  /** Replaces `old` by a physical material in every mesh below `root`. */
  constructor(old: MeshStandardMaterial, root: Object3D, quality: Quality) {
    this.full = quality === 'high';
    const mask = old.aoMap;
    if (mask) {
      this.maskTexture = mask;
    } else {
      this.maskTexture = new DataTexture(new Uint8Array([255, 255, 255, 255]), 1, 1, RGBAFormat);
      this.maskTexture.needsUpdate = true;
    }
    this.u.uMask.value = this.maskTexture;
    this.u.uMaskOn.value = mask ? 1 : 0;

    this.material = new MeshPhysicalMaterial({
      name: old.name,
      color: old.color.clone(),
      map: old.map,
      normalMap: old.normalMap,
      roughnessMap: old.roughnessMap,
      metalnessMap: old.metalnessMap,
      roughness: old.roughness,
      metalness: old.metalness,
      envMapIntensity: old.envMapIntensity,
      side: old.side,
    });
    if (old.normalMap) this.material.normalScale.copy(old.normalScale);
    this.material.iridescenceIOR = 1.6;
    this.material.iridescenceThicknessRange = [180, 520];
    this.material.onBeforeCompile = (shader) => {
      Object.assign(shader.uniforms, this.u);
      shader.fragmentShader = shader.fragmentShader
        .replace('void main() {', GLSL_FN + '\nvoid main() {')
        .replace('#include <map_fragment>', '#include <map_fragment>\n' + GLSL_COLOR)
        .replace('#include <metalnessmap_fragment>', '#include <metalnessmap_fragment>\n' + GLSL_PBR)
        .replace('#include <emissivemap_fragment>', '#include <emissivemap_fragment>\n' + GLSL_EMISSIVE)
        .replace('#include <lights_physical_fragment>', '#include <lights_physical_fragment>\n' + GLSL_COAT);
    };
    this.material.customProgramCacheKey = () => `cybercar-paint-v2-${this.full ? 'full' : 'lite'}`;

    root.traverse((o) => {
      const mesh = o as Mesh;
      if (!mesh.isMesh) return;
      if (Array.isArray(mesh.material)) mesh.material = mesh.material.map((m) => (m === old ? this.material : m));
      else if (mesh.material === old) mesh.material = this.material;
    });
    old.aoMap = null; // the mask is not ambient occlusion
    old.dispose();
    this.applyFinish();
  }

  /** Original violet = identity (no recolour). */
  reset(): void {
    this.u.uPaint.value.set(PAINT_REF.h, 1, 1);
  }

  setColor(color: ColorRepresentation): void {
    const c = new Color(color);
    const hsv = rgbToHsv(c.clone().convertLinearToSRGB());
    this.u.uPaint.value.set(hsv.h, MathUtils.clamp(hsv.s / PAINT_REF.s, 0, 2.2), MathUtils.clamp(hsv.v / PAINT_REF.v, 0, 2.2));
  }

  setFinish(finish: PaintFinishName | Partial<PaintFinish>): void {
    this.finish = typeof finish === 'string' ? { ...PAINT_FINISHES[finish] } : { ...this.finish, ...finish };
    this.applyFinish();
  }

  getFinish(): PaintFinish {
    return { ...this.finish };
  }

  /** Neon colours used by the rim light (kept in sync by CyberCar.setNeonColors). */
  setRimColors(a: Color, b: Color): void {
    this.u.uRimA.value.copy(a);
    this.u.uRimB.value.copy(b);
  }

  private applyFinish(): void {
    const f = this.finish;
    this.u.uMetal.value = f.metalness;
    this.u.uRough.value = f.roughness;
    this.u.uFlake.value = f.flake;
    this.u.uRim.value = f.rim;
    this.material.clearcoat = this.full ? f.clearcoat : 0;
    this.material.clearcoatRoughness = f.clearcoatRoughness;
    this.material.iridescence = this.full ? f.iridescence : 0;
  }

  dispose(): void {
    this.material.dispose();
    this.maskTexture.dispose();
  }
}
