import { BoxGeometry, Color, Group, InstancedBufferAttribute, InstancedMesh, Matrix4, Quaternion, Vector3, type ShaderMaterial } from "three";
import { BASE_DEPTH } from "./building-material";
import { prismaticCycle, type ScenePalette } from "./palette";
import { hash } from "./random";

/**
 * Immeubles-arches : un tablier fenêtré qui enjambe la route, posé sur deux
 * piliers de part et d'autre. Rares : une cellule de 1 900 m en contient une,
 * décalée au hasard, soit un passage toutes les 55 à 100 secondes de route.
 */
const CELL = 1900;
const FIRST_OFFSET = 700;
const JITTER = 600;
const SPAN_HALF = 19.5; // arête extérieure des piliers
const PILLAR_WIDTH = 8.5;
const DECK_BOTTOM = 24;
const DECK_HEIGHT = 11;
const DEPTH = 16;
const VISIBLE = 520;
const SLOTS = 2;
const PARTS = 3;

const IDENTITY = new Quaternion();
const matrix = new Matrix4();
const position = new Vector3();
const scale = new Vector3();

/** Profondeur (z monde, négatif vers l'avant) de l'arche de la cellule `cell`. */
export function bridgeZ(cell: number): number {
  return -(cell * CELL + FIRST_OFFSET + hash(cell, 41) * JITTER);
}

/** Tranche d'altitude du tablier (m) et demi-épaisseur de l'arche le long de la route : pour décider d'une esquive en vol. */
export function bridgeSpan(): { lo: number; hi: number; halfDepth: number } {
  return { lo: DECK_BOTTOM, hi: DECK_BOTTOM + DECK_HEIGHT, halfDepth: DEPTH / 2 };
}

/** Vrai si `z` tombe dans l'emprise d'une arche, marge comprise (la ville s'écarte pour elle). */
export function isNearBridge(z: number, margin: number): boolean {
  const cell = Math.floor(-z / CELL);
  for (let c = cell - 1; c <= cell + 1; c++) {
    if (c < 0) continue;
    if (Math.abs(z - bridgeZ(c)) < DEPTH / 2 + margin) return true;
  }
  return false;
}

export class Bridges {
  readonly group = new Group();
  private readonly mesh: InstancedMesh;
  private readonly info = new Float32Array(SLOTS * PARTS * 4);
  private readonly cycle: Color[];
  private lastKey = "";

  constructor(palette: ScenePalette, material: ShaderMaterial) {
    this.cycle = prismaticCycle(palette);
    const geometry = new BoxGeometry(1, 1, 1);
    geometry.translate(0, 0.5, 0);
    geometry.setAttribute("aInfo", new InstancedBufferAttribute(this.info, 4));
    this.mesh = new InstancedMesh(geometry, material, SLOTS * PARTS);
    this.mesh.frustumCulled = false;
    this.group.add(this.mesh);
    for (let i = 0; i < SLOTS * PARTS; i++) this.hide(i);
  }

  update(carZ: number): void {
    const cell = Math.max(0, Math.floor(-carZ / CELL));
    const visible: number[] = [];
    for (let c = Math.max(0, cell - 1); c <= cell + 1; c++) {
      const z = bridgeZ(c);
      if (z > carZ + 40 || z < carZ - VISIBLE) continue;
      visible.push(c);
    }
    const key = visible.join(",");
    if (key === this.lastKey) return;
    this.lastKey = key;

    for (let slot = 0; slot < SLOTS; slot++) {
      const first = slot * PARTS;
      const c = visible[slot];
      if (c === undefined) {
        for (let part = 0; part < PARTS; part++) this.hide(first + part);
        continue;
      }
      const z = bridgeZ(c);
      const tint = this.cycle[Math.floor(hash(c, 42) * this.cycle.length)]!;
      const seed = Math.floor(hash(c, 43) * 1000);
      const deckTop = DECK_BOTTOM + DECK_HEIGHT;
      const pillarX = SPAN_HALF - PILLAR_WIDTH / 2;
      // Piliers : du vide jusqu'au dessous du tablier. Tablier : d'un pilier à l'autre.
      this.place(first, -pillarX, -BASE_DEPTH, z, PILLAR_WIDTH, BASE_DEPTH + DECK_BOTTOM, DEPTH, tint, seed, -BASE_DEPTH);
      this.place(first + 1, pillarX, -BASE_DEPTH, z, PILLAR_WIDTH, BASE_DEPTH + DECK_BOTTOM, DEPTH, tint, seed + 7, -BASE_DEPTH);
      this.place(first + 2, 0, DECK_BOTTOM, z, SPAN_HALF * 2, deckTop - DECK_BOTTOM, DEPTH, tint, seed + 13, DECK_BOTTOM);
    }
    (this.mesh.geometry.getAttribute("aInfo") as InstancedBufferAttribute).needsUpdate = true;
    this.mesh.instanceMatrix.needsUpdate = true;
    if (this.mesh.instanceColor) this.mesh.instanceColor.needsUpdate = true;
  }

  dispose(): void {
    this.mesh.geometry.dispose();
    this.mesh.dispose();
  }

  private place(index: number, x: number, y: number, z: number, w: number, h: number, d: number, tint: Color, seed: number, offset: number): void {
    position.set(x, y, z);
    scale.set(w, h, d);
    matrix.compose(position, IDENTITY, scale);
    this.mesh.setMatrixAt(index, matrix);
    this.mesh.setColorAt(index, tint);
    this.info.set([seed, -1, 0, offset], index * 4);
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
