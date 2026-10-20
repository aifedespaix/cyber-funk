// Données de référence des six niveaux de Sillage et du fondu des portes (J2-02) : fait tourner les vrais `acts.ts` et
// `gates.ts` de Sillage (copies conformes dans `sillage/engine/show/`, commit fd34481) et écrit le JSON relu par
// `crates/cf-show/tests/gates_golden.rs`.
//
//   bun run tools/sillage-golden/gates.ts > crates/cf-show/tests/golden/gates.json
//
// Le script s'arrête si l'empreinte git d'une copie ne correspond plus. `SILLAGE_DIR` (facultatif) compare les copies
// à un clone local de Sillage. Le scénario (opérations + sorties) est entièrement dans le JSON : le test Rust le
// rejoue tel quel, sans réimplémenter le générateur pseudo-aléatoire du scénario.
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { formatJson, type Json } from "./json.ts";

const TOOL_DIR = dirname(fileURLToPath(import.meta.url));
/** Empreintes git (blob SHA-1) des copies, telles qu'au commit fd34481 de Sillage. */
const VENDORED: Readonly<Record<string, string>> = {
  "engine/show/acts.ts": "18629ce241a784b0369b39d5fada4c2e6e976d43",
  "engine/show/gates.ts": "b7a840f8449f490c3895accddb5101622b69b3ca",
};
const gitBlobSha = (content: Buffer): string =>
  createHash("sha1").update(`blob ${content.length}\0`).update(content).digest("hex");
const sillageDir = process.env.SILLAGE_DIR;
for (const [file, blob] of Object.entries(VENDORED)) {
  const copy = await readFile(join(TOOL_DIR, "sillage", file));
  if (gitBlobSha(copy) !== blob) throw new Error(`sillage/${file} ne correspond plus à Sillage fd34481`);
  if (sillageDir && !copy.equals(await readFile(join(sillageDir, file)))) {
    throw new Error(`sillage/${file} diffère de ${sillageDir}`);
  }
}

type Gates = Record<string, unknown> & { fx: Record<string, number> };
interface Blender {
  readonly gates: Gates;
  open(): void;
  update(source: number, mix: number, dt: number, snap: boolean): void;
}
interface VariationLike {
  source: number;
  mix: number;
  update(dt: number, on: boolean): boolean;
}
const acts = (await import(join(TOOL_DIR, "sillage/engine/show/acts.ts"))) as {
  LEVEL_STARTS: number[];
  OPEN_GATES: Gates;
};
const gatesModule = (await import(join(TOOL_DIR, "sillage/engine/show/gates.ts"))) as {
  GateBlender: new () => Blender;
  Variation: new (random: () => number) => VariationLike;
};

/** Copie profonde des portes (l'objet du blender est mis à jour sur place). */
const copy = (g: Gates): Json => JSON.parse(JSON.stringify(g)) as Json;

/** Les neuf nombres qui glissent (les listes ne changent qu'avec `source`, et sont déjà couvertes par les niveaux). */
const numbers = (g: Gates): Json => ({
  flightRate: g.flightRate as number,
  stuntGapScale: g.stuntGapScale as number,
  eventRate: g.eventRate as number,
  meteorRate: g.meteorRate as number,
  cruise: g.cruise as number,
  groundTraffic: g.groundTraffic as number,
  rgbSplit: g.fx.rgbSplit!,
  flash: g.fx.flash!,
  cityPulse: g.fx.cityPulse!,
});

// LCG 32 bits (Numerical Recipes) : même suite à chaque exécution.
const lcg = (start: number) => {
  let seed = start;
  return (): number => {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
    return seed / 4294967296;
  };
};

// 1. Portes de chaque niveau, au début (mix 0), au milieu et à la fin de l'acte, après un saut (`snap`).
const levels: Json[] = [];
for (let level = 0; level <= 5; level++) {
  const snapshots = [0, 0.5, 1].map((mix) => {
    const blender = new gatesModule.GateBlender();
    blender.update(level, mix, 0, true);
    return { mix, gates: copy(blender.gates) };
  });
  levels.push({ level, snapshots });
}

// 2. Glissement : une suite d'opérations sur un même blender (dt variables, sauts de niveau, `snap` occasionnels).
const rnd = lcg(0x1d0e5);
const blender = new gatesModule.GateBlender();
blender.update(0, 0, 0, true);
const glideOps: Json[] = [];
const glideOuts: Json[] = [];
let source = 0;
for (let i = 0; i < 600; i++) {
  if (rnd() < 0.02) source = Math.floor(rnd() * 6);
  const mix = Math.round(rnd() * 100) / 100;
  const dt = [0.004, 0.008, 0.016, 0.016, 0.033, 0.05, 0.12, 0.6][Math.floor(rnd() * 8)]! * (0.5 + rnd());
  const snap = rnd() < 0.01;
  blender.update(source, mix, dt, snap);
  glideOps.push({ source, mix, dt, snap });
  glideOuts.push(numbers(blender.gates));
}

// 3. Variations de l'après-set : mêmes tirages (même LCG, même ordre) dans le test Rust.
const VARIATION_SEED = 0x5eed;
const variation = new gatesModule.Variation(lcg(VARIATION_SEED));
const variationOps: Json[] = [];
const variationOuts: Json[] = [];
const dtRnd = lcg(0xbadc0de);
for (let i = 0; i < 1200; i++) {
  const dt = 0.2 + dtRnd() * 0.6;
  const on = i < 200 || i >= 500;
  const active = variation.update(dt, on);
  variationOps.push({ dt, on });
  variationOuts.push({ active, source: variation.source, mix: variation.mix });
}

const out: Json = {
  sillage: { repository: "aifedespaix/aifedespaix.com", commit: "fd344819121f292dd0646289a898c049b2e9d67f" },
  levelStarts: acts.LEVEL_STARTS,
  openGates: copy(acts.OPEN_GATES),
  levels,
  glide: { ops: glideOps, outs: glideOuts },
  variation: { seed: VARIATION_SEED, ops: variationOps, outs: variationOuts },
};
process.stdout.write(formatJson(out));
