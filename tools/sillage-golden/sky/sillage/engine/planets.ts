import {
  Color,
  DoubleSide,
  Group,
  Mesh,
  RingGeometry,
  SRGBColorSpace,
  ShaderMaterial,
  SphereGeometry,
  Texture,
  TextureLoader,
  Vector3,
} from "three";
import type { ScenePalette, SceneColorName } from "./palette";
import type { RingPlanet } from "./cinema/types";

interface PlanetSpec {
  texture: string;
  /** Direction depuis la caméra : azimut (0 = droit devant) et élévation, en degrés. */
  azimuth: number;
  elevation: number;
  distance: number;
  radius: number;
  tilt: number;
  /** Vitesse de rotation sur l'axe, en tours par minute. */
  rpm: number;
  atmosphere: SceneColorName;
  ring?: { tint: SceneColorName; inner: number; outer: number };
}

/**
 * Du plus proche au plus lointain : lune voisine, géantes gazeuses, mondes
 * glacés et océaniques qui ne sont plus que des billes. Textures CC0 (voir
 * public/sillage/planets/LICENCE.md).
 */
const SPECS: PlanetSpec[] = [
  { texture: "moon", azimuth: 16, elevation: 30, distance: 300, radius: 20, tilt: 0.1, rpm: 0.8, atmosphere: "ice" },
  { texture: "gas-ember", azimuth: -30, elevation: 21, distance: 560, radius: 120, tilt: 0.38, rpm: 0.5, atmosphere: "amber", ring: { tint: "amber", inner: 1.45, outer: 2.35 } },
  { texture: "ocean", azimuth: 38, elevation: 13, distance: 640, radius: 66, tilt: 0.3, rpm: 1.2, atmosphere: "azure" },
  { texture: "gas-azure", azimuth: -62, elevation: 36, distance: 820, radius: 50, tilt: -0.2, rpm: 0.9, atmosphere: "mint" },
  { texture: "ice", azimuth: -4, elevation: 8, distance: 900, radius: 17, tilt: 0.5, rpm: 1.6, atmosphere: "ice" },
  { texture: "gas-rose", azimuth: 72, elevation: 28, distance: 760, radius: 40, tilt: 0.15, rpm: 0.7, atmosphere: "rose", ring: { tint: "rose", inner: 1.4, outer: 1.9 } },
  { texture: "gas-teal", azimuth: 160, elevation: 22, distance: 600, radius: 70, tilt: 0.25, rpm: 0.6, atmosphere: "mint" },
];

const LIGHT = new Vector3(-0.38, 0.35, 0.86).normalize();

interface Body {
  group: Group;
  surface: Mesh;
  rpm: number;
  materials: ShaderMaterial[];
}

/** Astres texturés qui tournent sur eux-mêmes, éclairés par une étoile hors champ. */
export class Planets {
  readonly group = new Group();
  private readonly bodies: Body[] = [];
  private readonly textures: Texture[] = [];
  private readonly sphere = new SphereGeometry(1, 64, 40);
  private ring: RingPlanet | null = null;

  constructor(private readonly palette: ScenePalette) {}

  /** Charge les textures ; un astre dont la texture manque est simplement omis. */
  async load(baseUrl: string): Promise<void> {
    const loader = new TextureLoader();
    const results = await Promise.allSettled(SPECS.map((spec) => loader.loadAsync(`${baseUrl}/${spec.texture}.jpg`)));
    results.forEach((result, i) => {
      if (result.status !== "fulfilled") return;
      const texture = result.value;
      texture.colorSpace = SRGBColorSpace;
      texture.anisotropy = 4;
      this.textures.push(texture);
      this.add(SPECS[i]!, texture);
    });
  }

  /** Textures chargées avec succès : réutilisées par les astres qui tombent. */
  get loadedTextures(): readonly Texture[] {
    return this.textures;
  }

  /** La géante à anneau, en repère local du ciel (le groupe du ciel n'est ni tourné ni mis à l'échelle). Null tant que sa texture manque. */
  get ringPlanet(): RingPlanet | null {
    return this.ring;
  }

  update(dt: number, pulse: number): void {
    for (const body of this.bodies) {
      body.surface.rotation.y += ((body.rpm * Math.PI * 2) / 60) * dt;
      for (const material of body.materials) material.uniforms.uPulse!.value = pulse;
    }
  }

  dispose(): void {
    this.sphere.dispose();
    for (const texture of this.textures) texture.dispose();
    for (const body of this.bodies) {
      for (const material of body.materials) material.dispose();
      for (const child of body.group.children) if (child !== body.surface && child instanceof Mesh) child.geometry.dispose();
    }
  }

  private add(spec: PlanetSpec, texture: Texture): void {
    const az = (spec.azimuth * Math.PI) / 180;
    const el = (spec.elevation * Math.PI) / 180;
    const group = new Group();
    group.position.set(Math.sin(az) * Math.cos(el), Math.sin(el), -Math.cos(az) * Math.cos(el)).multiplyScalar(spec.distance);
    group.rotation.z = spec.tilt;

    const surfaceMaterial = this.surfaceMaterial(texture, this.palette[spec.atmosphere]);
    const surface = new Mesh(this.sphere, surfaceMaterial);
    surface.scale.setScalar(spec.radius);
    group.add(surface);
    const materials = [surfaceMaterial];

    if (spec.ring) {
      const geometry = new RingGeometry(spec.radius * spec.ring.inner, spec.radius * spec.ring.outer, 128, 1);
      geometry.rotateX(-Math.PI / 2 + 0.28);
      const ringMaterial = this.ringMaterial(this.palette[spec.ring.tint], spec.radius * spec.ring.outer);
      group.add(new Mesh(geometry, ringMaterial));
      materials.push(ringMaterial);
      if (spec.texture === "gas-ember") {
        // Normale de l'anneau : celle du disque (+Z), pivotée comme sa géométrie (rotateX) puis comme le groupe (rotation.z).
        const ringNormal = new Vector3(0, 0, 1).applyAxisAngle(new Vector3(1, 0, 0), -Math.PI / 2 + 0.28).applyAxisAngle(new Vector3(0, 0, 1), spec.tilt);
        this.ring = { center: group.position.clone(), radius: spec.radius, ringInner: spec.radius * spec.ring.inner, ringOuter: spec.radius * spec.ring.outer, ringNormal };
      }
    }

    this.group.add(group);
    this.bodies.push({ group, surface, rpm: spec.rpm, materials });
  }

  private surfaceMaterial(map: Texture, atmosphere: Color): ShaderMaterial {
    return new ShaderMaterial({
      uniforms: {
        uMap: { value: map },
        uLight: { value: LIGHT },
        uAtmo: { value: atmosphere },
        uPulse: { value: 0 },
      },
      vertexShader: /* glsl */ `
        varying vec2 vUv;
        varying vec3 vN;
        varying vec3 vW;
        void main() {
          vUv = uv;
          vN = normalize(mat3(modelMatrix) * normal);
          vec4 wp = modelMatrix * vec4(position, 1.0);
          vW = wp.xyz;
          gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
        }
      `,
      fragmentShader: /* glsl */ `
        uniform sampler2D uMap;
        uniform vec3 uLight;
        uniform vec3 uAtmo;
        uniform float uPulse;
        varying vec2 vUv;
        varying vec3 vN;
        varying vec3 vW;
        void main() {
          vec3 albedo = texture2D(uMap, vUv).rgb;
          vec3 n = normalize(vN);
          vec3 v = normalize(cameraPosition - vW);
          float ndl = dot(n, uLight);
          float day = smoothstep(-0.1, 0.55, ndl);
          vec3 col = albedo * (0.09 + 1.0 * day);
          // Atmosphère : liseré de Fresnel, plus vif côté éclairé, et lueur au terminateur.
          float fres = pow(1.0 - max(dot(n, v), 0.0), 3.2);
          col += uAtmo * fres * (0.18 + 1.1 * smoothstep(-0.35, 0.6, ndl)) * (1.0 + uPulse * 0.5);
          col += uAtmo * 0.12 * smoothstep(0.25, 0.0, abs(ndl)) * (1.0 - fres);
          gl_FragColor = vec4(col, 1.0);
        }
      `,
    });
  }

  private ringMaterial(tint: Color, outer: number): ShaderMaterial {
    return new ShaderMaterial({
      transparent: true,
      depthWrite: false,
      side: DoubleSide,
      uniforms: { uTint: { value: tint }, uOuter: { value: outer }, uPulse: { value: 0 } },
      vertexShader: /* glsl */ `
        varying vec2 vP;
        void main() {
          vP = position.xz;
          gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
        }
      `,
      fragmentShader: /* glsl */ `
        uniform vec3 uTint;
        uniform float uOuter;
        uniform float uPulse;
        varying vec2 vP;
        void main() {
          float r = length(vP) / uOuter;
          // Anneaux concentriques de densités variées, avec une division nette.
          float bands = 0.45 + 0.35 * sin(r * 160.0) * sin(r * 47.0 + 1.3) + 0.2 * sin(r * 310.0);
          float gap = smoothstep(0.0, 0.015, abs(r - 0.72));
          float a = clamp(bands, 0.0, 1.0) * gap * 0.55;
          gl_FragColor = vec4(uTint * (0.55 + uPulse * 0.25) * a, a);
        }
      `,
    });
  }
}
