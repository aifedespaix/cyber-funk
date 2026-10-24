import { BoxGeometry, Color, Group, InstancedBufferAttribute, InstancedMesh, Matrix4, Quaternion, Vector3, type ShaderMaterial } from "three";
import { BASE_DEPTH, createBuildingMaterial, FLOOR_HEIGHT } from "./building-material";
import { Bridges, isNearBridge } from "./bridges";
import type { SpotAnchor } from "./beams";
import type { CityField } from "./landmarks/field";
import { prismaticCycle, type ScenePalette } from "./palette";
import { hash } from "./random";

const SPACING = 26; // distance entre deux rangées
const ROWS = 34;
const BEHIND = 15; // rangées conservées derrière la voiture (390 m) : les tours rétrécissent dès 300 m de la caméra et la brume les noie, le bord de la ville ne se voit pas
const LAYERS = [
  // Premier plan : trapus, collés à la route. Puis deux couronnes de tours plus hautes.
  { x: 13.5, spread: 3, width: [8, 13], depth: [8, 12], height: [9, 28], skip: 0.22, spots: 0.55, far: false },
  { x: 34, spread: 6, width: [10, 18], depth: [10, 16], height: [20, 50], skip: 0.25, spots: 0.35, far: false },
  { x: 58, spread: 10, width: [12, 22], depth: [12, 20], height: [38, 85], skip: 0.1, spots: 0, far: false },
  // Arrière-plan : trois couronnes de plus en plus hautes et larges, qui dessinent la ligne d'horizon de la ville.
  // Elles sont presque noyées dans la brume : un corps seul, sans couronne, avec un ombrage simplifié (tier 3).
  { x: 98, spread: 14, width: [16, 30], depth: [16, 24], height: [70, 140], skip: 0.12, spots: 0, far: true },
  { x: 150, spread: 20, width: [20, 36], depth: [18, 26], height: [100, 200], skip: 0.1, spots: 0, far: true },
  { x: 214, spread: 30, width: [26, 46], depth: [20, 28], height: [140, 280], skip: 0.1, spots: 0, far: true },
] as const;
/** Couronnes lointaines conservées selon le niveau de qualité (0 = tout, 3 = le strict minimum). */
const FAR_BY_QUALITY = [3, 3, 2, 1] as const;
const NEAR_LAYERS = 3;
/** Type d'une tour d'arrière-plan dans l'attribut `aInfo.z`. */
const FAR_TIER = 3;
const SLOTS = LAYERS.length * 2;
const PARTS = 3; // corps, couronne, antenne
const PER_ROW = SLOTS * PARTS;
/** Boîtes de collision : le corps et la couronne de chaque tour (les antennes, trop fines, ne comptent pas). */
const MAX_HITBOXES = ROWS * SLOTS * 2;
const HITBOX = 5;

/** Zone sans immeuble : de `z0` à `z1` (z monde, z0 < z1), des deux côtés (0) ou d'un seul (-1 gauche, 1 droite). */
export interface Clearing {
  z0: number;
  z1: number;
  side: -1 | 0 | 1;
}

const IDENTITY = new Quaternion();
const matrix = new Matrix4();
const position = new Vector3();
const scale = new Vector3();

/**
 * Cité de gratte-ciel de chaque côté de la route. Tout est recalculé à partir
 * de l'index de rangée (graine déterministe) : le décor est infini et ne bouge
 * que lorsqu'on franchit une rangée. Un seul appel de dessin pour la ville.
 */
export class Scenery {
  readonly group = new Group();
  /** Points d'attache des projecteurs, indexés par rangée courante. */
  readonly spots: SpotAnchor[] = [];

  private readonly material: ShaderMaterial;
  private readonly mesh: InstancedMesh;
  private readonly info = new Float32Array(ROWS * PER_ROW * 4);
  /** Pour chaque tour visible : x min, x max, z min, z max, hauteur du sommet. */
  private readonly hitboxes = new Float32Array(MAX_HITBOXES * HITBOX);
  private hitCount = 0;
  private readonly cycle: Color[];
  private readonly bridges: Bridges;
  private lastBase = Number.NaN;
  private farLayers: number = FAR_BY_QUALITY[0];
  private clearings: readonly Clearing[] = [];
  /** Zone dégagée pour l'enseigne du DJ : indépendante de celles du trafic, qui remplace sa liste à chaque événement. */
  private signClearing: Clearing | null = null;

  constructor(palette: ScenePalette, field: CityField) {
    this.cycle = prismaticCycle(palette);
    this.material = createBuildingMaterial(palette, field);

    const geometry = new BoxGeometry(1, 1, 1);
    geometry.translate(0, 0.5, 0);
    geometry.setAttribute("aInfo", new InstancedBufferAttribute(this.info, 4));

    this.mesh = new InstancedMesh(geometry, this.material, ROWS * PER_ROW);
    this.mesh.frustumCulled = false;
    this.group.add(this.mesh);

    this.bridges = new Bridges(palette, this.material);
    this.group.add(this.bridges.group);
  }

  /** Matériau des gratte-ciel, partagé par les structures posées à la main. */
  get blockMaterial(): ShaderMaterial {
    return this.material;
  }

  /** Quand la machine peine, on retire d'abord les couronnes les plus lointaines (les moins vues). */
  setQuality(level: number): void {
    const far = FAR_BY_QUALITY[Math.min(level, FAR_BY_QUALITY.length - 1)]!;
    if (far === this.farLayers) return;
    this.farLayers = far;
    this.lastBase = Number.NaN;
  }

  /** Remplace les zones dégagées et force le recalcul de la ville au prochain passage. */
  setClearings(clearings: readonly Clearing[]): void {
    this.clearings = clearings;
    this.lastBase = Number.NaN;
  }

  /** Zone dégagée de l'enseigne du DJ (ou `null`) ; garde à part des zones du trafic. */
  setSignClearing(clearing: Clearing | null): void {
    this.signClearing = clearing;
    this.lastBase = Number.NaN;
  }

  update(carZ: number, time: number, pulse: number, bass: number, mid: number, high: number, beat: number): void {
    const u = this.material.uniforms;
    u.uTime!.value = time;
    u.uPulse!.value = pulse;
    u.uBass!.value = bass;
    u.uMid!.value = mid;
    u.uHigh!.value = high;
    u.uBeat!.value = beat;

    this.bridges.update(carZ);
    const base = Math.floor(-carZ / SPACING) - BEHIND;
    if (base === this.lastBase) return;
    this.lastBase = base;
    this.layout(base);
  }

  dispose(): void {
    this.mesh.geometry.dispose();
    this.material.dispose();
    this.mesh.dispose();
    this.bridges.dispose();
  }

  /**
   * Une sphère (centre `at`, rayon `radius`) touche-t-elle un immeuble ? Le test suit la ville affichée : ce qu'on voit
   * est ce qu'on percute. Les tours qui plongent sous la route ont leur pied bien en dessous : seul le sommet compte.
   */
  collides(at: Vector3, radius: number): boolean {
    const reach = radius * 0.7;
    const boxes = this.hitboxes;
    for (let i = 0; i < this.hitCount; i++) {
      const o = i * HITBOX;
      if (at.y < boxes[o + 4]! + reach && at.x > boxes[o]! - reach && at.x < boxes[o + 1]! + reach && at.z > boxes[o + 2]! - reach && at.z < boxes[o + 3]! + reach) {
        return at.y > -BASE_DEPTH;
      }
    }
    return false;
  }

  /** Hauteur du toit sous le point (x, z), ou -1 s'il n'y a pas d'immeuble : de quoi rouler sur les toits. */
  roofAt(x: number, z: number): number {
    const boxes = this.hitboxes;
    let top = -1;
    for (let i = 0; i < this.hitCount; i++) {
      const o = i * HITBOX;
      if (x > boxes[o]! && x < boxes[o + 1]! && z > boxes[o + 2]! && z < boxes[o + 3]! && boxes[o + 4]! > top) top = boxes[o + 4]!;
    }
    return top;
  }

  /**
   * Distance à l'axe de la route de la façade la plus proche d'un côté (`side` : -1 gauche, 1 droite) entre `z - depth` et
   * `z + depth`, ou `Infinity` s'il n'y a rien. Les tours trop basses ou trop lointaines ne comptent pas.
   */
  facade(side: -1 | 1, z: number, depth: number): number {
    const boxes = this.hitboxes;
    let near = Infinity;
    for (let i = 0; i < this.hitCount; i++) {
      const o = i * HITBOX;
      if (boxes[o + 4]! < 5 || boxes[o + 3]! < z - depth || boxes[o + 2]! > z + depth) continue;
      const inner = side < 0 ? -boxes[o + 1]! : boxes[o]!;
      if (side < 0 ? boxes[o + 1]! > 0 : boxes[o]! < 0) continue;
      if (inner < near && inner < 26) near = inner;
    }
    return near;
  }

  private addHitbox(cx: number, cz: number, w: number, d: number, top: number): void {
    boxesSet(this.hitboxes, this.hitCount++, cx - w / 2, cx + w / 2, cz - d / 2, cz + d / 2, top);
  }

  private layout(base: number): void {
    this.spots.length = 0;
    this.hitCount = 0;
    const attribute = this.mesh.geometry.getAttribute("aInfo") as InstancedBufferAttribute;

    for (let row = 0; row < ROWS; row++) {
      const r = base + row;
      for (let slot = 0; slot < SLOTS; slot++) {
        const layerIndex = slot >> 1;
        const layer = LAYERS[layerIndex]!;
        const layerOn = layerIndex < NEAR_LAYERS || layerIndex - NEAR_LAYERS < this.farLayers;
        const side = slot % 2 === 0 ? -1 : 1;
        const seed = r * 13.37 + slot * 71.1;
        const first = (row * SLOTS + slot) * PARTS;

        // La ville s'écarte devant le pied d'une arche.
        const slotZ = -r * SPACING;
        if (!layerOn || hash(seed, 1) < layer.skip || (layerIndex === 0 && isNearBridge(slotZ, 12)) || this.cleared(slotZ, side)) {
          for (let part = 0; part < PARTS; part++) this.hide(first + part);
          continue;
        }

        const w = lerp(layer.width, hash(seed, 2));
        const d = lerp(layer.depth, hash(seed, 3));
        const h = lerp(layer.height, hash(seed, 4) ** 1.4);
        const centerX = side * (layer.x + hash(seed, 5) * layer.spread + w / 2);
        const z = -r * SPACING + (hash(seed, 6) - 0.5) * 6;
        const tint = this.cycle[Math.floor(hash(seed, 7) * this.cycle.length)]!;
        const seed01 = Math.floor(hash(seed, 8) * 1000);

        // Étage des projecteurs, quantifié sur la grille d'étages.
        let spotY = -1;
        if (hash(seed, 9) < layer.spots) {
          spotY = (Math.floor((0.45 + hash(seed, 10) * 0.45) * (h / FLOOR_HEIGHT)) + 0.5) * FLOOR_HEIGHT;
          this.spots.push({
            x: centerX - side * (w / 2),
            y: spotY,
            z,
            side,
            phase: hash(seed, 11) * Math.PI * 2,
            tint: this.cycle[Math.floor(hash(seed, 12) * this.cycle.length)]!,
          });
        }

        this.addHitbox(centerX, z, w, d, h);
        // Le corps plonge sous la route : de la route on ne voit jamais son pied.
        this.place(first, centerX, -BASE_DEPTH, z, w, h + BASE_DEPTH, d, tint, seed01, spotY, layer.far ? FAR_TIER : 0, -BASE_DEPTH);

        if (layer.far) {
          // Pas de couronne au loin ; une antenne de temps en temps pour piquer la ligne d'horizon.
          this.hide(first + 1);
          if (hash(seed, 17) < 0.3) this.place(first + 2, centerX, h, z, 1.2, 14 + hash(seed, 18) * 26, 1.2, tint, seed01, -1, 2, 0);
          else this.hide(first + 2);
          continue;
        }

        // Couronne en retrait sur une tour sur deux ; antenne sur certaines.
        const crowned = hash(seed, 13) < 0.6 && h > 22;
        if (crowned) {
          const ch = h * (0.14 + hash(seed, 14) * 0.18);
          this.place(first + 1, centerX, h, z, w * 0.62, ch, d * 0.62, tint, seed01 + 3, -1, 1, h);
          this.addHitbox(centerX, z, w * 0.62, d * 0.62, h + ch);
          if (hash(seed, 15) < 0.55) {
            this.place(first + 2, centerX, h + ch, z, 0.6, 12 + hash(seed, 16) * 16, 0.6, tint, seed01, -1, 2, 0);
          } else this.hide(first + 2);
        } else {
          this.hide(first + 1);
          if (hash(seed, 17) < 0.35) this.place(first + 2, centerX, h, z, 0.6, 8 + hash(seed, 18) * 10, 0.6, tint, seed01, -1, 2, 0);
          else this.hide(first + 2);
        }
      }
    }
    attribute.needsUpdate = true;
    this.mesh.instanceMatrix.needsUpdate = true;
    if (this.mesh.instanceColor) this.mesh.instanceColor.needsUpdate = true;
  }

  private cleared(z: number, side: number): boolean {
    const own = this.signClearing;
    if (own && z > own.z0 - 8 && z < own.z1 + 8 && (own.side === 0 || own.side === side)) return true;
    for (const c of this.clearings) {
      if (z > c.z0 - 8 && z < c.z1 + 8 && (c.side === 0 || c.side === side)) return true;
    }
    return false;
  }

  private place(index: number, x: number, y: number, z: number, w: number, h: number, d: number, tint: Color, seed: number, spotY: number, tier: number, offset: number): void {
    position.set(x, y, z);
    scale.set(w, h, d);
    matrix.compose(position, IDENTITY, scale);
    this.mesh.setMatrixAt(index, matrix);
    this.mesh.setColorAt(index, tint);
    const o = index * 4;
    this.info[o] = seed;
    this.info[o + 1] = spotY;
    this.info[o + 2] = tier;
    this.info[o + 3] = offset;
  }

  private hide(index: number): void {
    position.set(0, -50, 0);
    scale.set(0, 0, 0);
    matrix.compose(position, IDENTITY, scale);
    this.mesh.setMatrixAt(index, matrix);
    this.mesh.setColorAt(index, this.cycle[0]!);
    this.info.fill(0, index * 4, index * 4 + 4);
  }
}

function boxesSet(boxes: Float32Array, index: number, minX: number, maxX: number, minZ: number, maxZ: number, top: number): void {
  boxes.set([minX, maxX, minZ, maxZ, top], index * HITBOX);
}

function lerp(range: readonly [number, number], t: number): number {
  return range[0] + (range[1] - range[0]) * t;
}
