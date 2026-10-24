import { BufferAttribute, BufferGeometry, Color, Group, Mesh, PlaneGeometry, Points, ShaderMaterial, SphereGeometry, Vector3 } from "three";
import { Planets } from "./planets";
import type { RingPlanet } from "./cinema/types";
import { watchPalette } from "./live-palette";
import type { ScenePalette } from "./palette";
import { hash } from "./random";
import { createAurora, createDome, createStars, GALAXY_AXIS } from "./sky-materials";
import { ShootingStars } from "./shooting-stars";

interface Layer {
  count: number;
  /** Taille en pixels : [min, max], tirée avec un exposant pour rendre les grosses plus rares. */
  size: [number, number];
  power: number;
  /** Part des étoiles resserrées autour de la Voie lactée. */
  galactic: number;
}

const LAYERS: Layer[] = [
  { count: 5200, size: [1, 1.7], power: 1, galactic: 0.55 }, // poussière d'étoiles lointaines
  { count: 1100, size: [1.8, 3], power: 2, galactic: 0.3 },
  { count: 70, size: [4.5, 8.5], power: 1.5, galactic: 0 }, // astres brillants proches
];
const RADIUS = 1000;

/**
 * Espace : voûte sombre à nébuleuses, étoiles sur trois profondeurs, étoiles
 * filantes, astres texturés qui tournent et aurore boréale lointaine. Le
 * groupe suit la caméra : le ciel est « à l'infini », il ne se rapproche jamais.
 */
export class Sky {
  readonly group = new Group();
  private readonly planets: Planets;
  private readonly shooting: ShootingStars;
  private readonly materials: ShaderMaterial[] = [];
  private readonly disposables: { dispose(): void }[] = [];
  /** Décalage du ciel (mode cinéma) : à zéro il est « à l'infini », sinon il est dessiné en `caméra - drift`. */
  private readonly drift = new Vector3();
  private readonly unwatch: (() => void)[] = [];

  constructor(
    palette: ScenePalette,
    private readonly calm: boolean,
    pixelRatio: number,
  ) {
    const motion = calm ? 0.3 : 1;
    this.group.add(this.mesh(new SphereGeometry(RADIUS, 40, 24), this.track(createDome(palette))));
    for (const [i, layer] of LAYERS.entries()) {
      const points = new Points(this.stars(palette, layer, i), this.track(createStars(pixelRatio)));
      points.frustumCulled = false;
      this.group.add(points);
    }

    // Aurores lointaines : basses sur l'horizon, très étirées, discrètes.
    this.group.add(this.mesh(new PlaneGeometry(1900, 230, 1, 1), this.track(createAurora([palette.mint, palette.azure, palette.violet], 0, motion)), [-80, 85, -820]));
    this.group.add(this.mesh(new PlaneGeometry(1700, 190, 1, 1), this.track(createAurora([palette.rose, palette.violet, palette.azure], 1.7, motion)), [260, 70, -940]));

    this.planets = new Planets(palette);
    this.shooting = new ShootingStars(palette);
    this.group.add(this.planets.group, this.shooting.group);
    this.disposables.push(this.planets, this.shooting);
  }

  load(planetsUrl: string): Promise<void> {
    return this.planets.load(planetsUrl);
  }

  get planetTextures() {
    return this.planets.loadedTextures;
  }

  /** La géante à anneau, en repère local du ciel. */
  get ringPlanet(): RingPlanet | null {
    return this.planets.ringPlanet;
  }

  /** Décale le ciel : la caméra « voyage » vers les planètes (voir `CameraPose.skyDrift`). Copié, à rappeler chaque image. */
  setDrift(drift: Vector3): void {
    this.drift.copy(drift);
  }

  /** Lance une étoile filante immédiatement. */
  shoot(): void {
    this.shooting.launch();
  }

  /** `anchor` : position de la caméra, que le ciel suit en permanence. */
  update(time: number, dt: number, pulse: number, anchor: Vector3): void {
    this.group.position.copy(anchor).sub(this.drift);
    for (const material of this.materials) {
      material.uniforms.uTime!.value = time;
      material.uniforms.uPulse!.value = pulse;
    }
    this.planets.update(dt, pulse);
    this.shooting.update(dt, this.calm);
  }

  dispose(): void {
    for (const stop of this.unwatch) stop();
    for (const d of this.disposables) d.dispose();
  }

  private mesh(geometry: BufferGeometry, material: ShaderMaterial, at: [number, number, number] = [0, 0, 0]): Mesh {
    this.disposables.push(geometry);
    const mesh = new Mesh(geometry, material);
    mesh.position.set(...at);
    // Le fond est dessiné avant tout le reste ; les astres opaques gèrent leur profondeur.
    mesh.renderOrder = -10;
    return mesh;
  }

  private track(material: ShaderMaterial): ShaderMaterial {
    this.materials.push(material);
    this.disposables.push(material);
    return material;
  }

  private stars(palette: ScenePalette, layer: Layer, layerIndex: number): BufferGeometry {
    const tints: Color[] = [palette.ice, palette.ice, palette.azure, palette.mint, palette.amber, palette.rose];
    const positions = new Float32Array(layer.count * 3);
    const sizes = new Float32Array(layer.count);
    const phases = new Float32Array(layer.count);
    const colors = new Float32Array(layer.count * 3);
    const salt = layerIndex * 100;

    for (let i = 0; i < layer.count; i++) {
      // Direction uniforme sur la sphère…
      const z = hash(i, salt + 1) * 2 - 1;
      const a = hash(i, salt + 2) * Math.PI * 2;
      const r = Math.sqrt(1 - z * z);
      let x = r * Math.cos(a);
      let y = z;
      let zz = r * Math.sin(a);
      // …puis une partie est ramenée vers le plan de la Voie lactée.
      if (hash(i, salt + 3) < layer.galactic) {
        const k = x * GALAXY_AXIS.x + y * GALAXY_AXIS.y + zz * GALAXY_AXIS.z;
        const squash = 0.82 + hash(i, salt + 4) * 0.12;
        x -= GALAXY_AXIS.x * k * squash;
        y -= GALAXY_AXIS.y * k * squash;
        zz -= GALAXY_AXIS.z * k * squash;
        const norm = Math.hypot(x, y, zz) || 1;
        x /= norm;
        y /= norm;
        zz /= norm;
      }
      const radius = RADIUS * (0.78 + hash(i, salt + 5) * 0.15);
      positions.set([x * radius, y * radius, zz * radius], i * 3);
      sizes[i] = layer.size[0] + (layer.size[1] - layer.size[0]) * hash(i, salt + 6) ** layer.power;
      phases[i] = hash(i, salt + 7);
      const tint = tints[Math.floor(hash(i, salt + 8) * tints.length)]!;
      const dim = layerIndex === 0 ? 0.9 : 1;
      colors.set([tint.r * dim, tint.g * dim, tint.b * dim], i * 3);
    }

    const geometry = new BufferGeometry();
    geometry.setAttribute("position", new BufferAttribute(positions, 3));
    geometry.setAttribute("aSize", new BufferAttribute(sizes, 1));
    geometry.setAttribute("aPhase", new BufferAttribute(phases, 1));
    geometry.setAttribute("aColor", new BufferAttribute(colors, 3));
    this.disposables.push(geometry);
    // Les couleurs sont cuites dans les sommets : on les refait quand le fondu de palette est fini (même tirage, nouvelles teintes).
    const dim = layerIndex === 0 ? 0.9 : 1;
    this.unwatch.push(
      watchPalette(palette, (_, settled) => {
        if (!settled) return;
        for (let i = 0; i < layer.count; i++) {
          const tint = tints[Math.floor(hash(i, salt + 8) * tints.length)]!;
          colors[i * 3] = tint.r * dim;
          colors[i * 3 + 1] = tint.g * dim;
          colors[i * 3 + 2] = tint.b * dim;
        }
        geometry.getAttribute("aColor").needsUpdate = true;
      }),
    );
    return geometry;
  }
}
