/**
 * Données de référence (golden) de la ville de Sillage, pour les tests de `cf-world-neon` (J5-02).
 *
 * Fait tourner les fichiers TypeScript d'origine, copiés tels quels dans `sillage/` : `scenery.ts` (les rangées
 * d'immeubles, les collisions), `bridges.ts`, `random.ts` et `palette.ts`. Trois choses sont remplacées par des
 * bouchons, car elles ne changent rien aux calculs : `three` (`stubs/three.ts`), `building-material.ts` (constantes
 * seulement, pas de shader), et la palette (des couleurs qui ne portent qu'un numéro). Écrit
 * `crates/cf-world-neon/tests/golden/scenery.json`.
 *
 *   bun run tools/sillage-golden/scenery/index.ts           régénère le fichier
 *   bun run tools/sillage-golden/scenery/index.ts --check   recalcule et compare (code 1 si écart)
 *
 * Variable facultative `SILLAGE_DIR` : chemin de `apps/main/src/components/sillage` d'un clone de Sillage ; les copies
 * sont alors comparées octet par octet aux originaux.
 */
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { Scenery, type Clearing } from "./sillage/engine/scenery.ts";
import { Color, Vector3, type InstancedMesh } from "three";

/** Version de Sillage dont les fichiers de `sillage/` sont copiés (branche `main`). */
const SILLAGE_COMMIT = "fd344819121f292dd0646289a898c049b2e9d67f";
/** Empreintes git (blob SHA-1) des copies conformes : une copie modifiée arrête le script. */
const VENDORED: Readonly<Record<string, string>> = {
    "engine/scenery.ts": "53402982e0310506371a9145fbe4c18931792952",
    "engine/bridges.ts": "7991adee65466ad547f10e0c1288f2c96707c913",
    "engine/random.ts": "0a576ed902d7a42b25b1255166a5b6a68c7f20c8",
    "engine/palette.ts": "e9da0a143b88537cd1e40051b64e86799c742691",
};

const TOOL_DIR = import.meta.dir;
const ROOT = resolve(TOOL_DIR, "../../..");
const OUT = join(ROOT, "crates/cf-world-neon/tests/golden/scenery.json");

const SPACING = 26;
const BEHIND = 15;
const PER_ROW = 36;

/** Générateur déterministe (mulberry32) : les scénarios ne changent pas d'un lancement à l'autre. */
function rng(seed: number): () => number {
    let a = seed >>> 0;
    return () => {
        a = (a + 0x6d2b79f5) >>> 0;
        let t = a;
        t = Math.imul(t ^ (t >>> 15), t | 1);
        t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
        return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
}

async function verifyVendored(): Promise<void> {
    const sillageDir = process.env.SILLAGE_DIR;
    for (const [file, expected] of Object.entries(VENDORED)) {
        const local = await readFile(join(TOOL_DIR, "sillage", file));
        const header = new TextEncoder().encode(`blob ${local.length}\0`);
        const actual = createHash("sha1").update(header).update(local).digest("hex");
        if (actual !== expected) throw new Error(`${file} a été modifié : ${actual} ≠ ${expected} (commit ${SILLAGE_COMMIT})`);
        if (sillageDir) {
            const original = await readFile(join(sillageDir, file));
            if (!Buffer.from(original).equals(Buffer.from(local))) throw new Error(`${file} diffère de l'original dans ${sillageDir}`);
        }
    }
}

/**
 * Une palette dont chaque lecture rend une nouvelle couleur numérotée : le cycle prismatique de la ville
 * (`[mint, azure, violet, mint, rose, ice]`) reçoit donc les numéros 0 à 5, dans l'ordre, et le rang de teinte d'une tour
 * se lit sur sa couleur. (`Scenery` lit le cycle en premier, puis `Bridges`.)
 */
function numberedPalette(): never {
    let next = 0;
    return new Proxy({}, { get: () => new Color(String(next++)) }) as never;
}

interface Case {
    carZ: number;
    quality: number;
    clearings: Clearing[];
    signClearing: Clearing | null;
}

function cases(): Case[] {
    const rand = rng(20260702);
    const pick = <T>(values: readonly T[]): T => values[Math.floor(rand() * values.length)]!;
    const out: Case[] = [
        // Le départ, rangées négatives comprises, et un point au milieu d'une arche.
        { carZ: 0, quality: 0, clearings: [], signClearing: null },
        { carZ: -13, quality: 0, clearings: [], signClearing: null },
    ];
    for (let c = 0; c < 8; c++) {
        const carZ = -Math.round(rand() * 60000 * 100) / 100;
        const clearings: Clearing[] = [];
        if (c % 2 === 1) {
            const z0 = carZ - 100 - rand() * 200;
            clearings.push({ z0, z1: z0 + 20 + rand() * 60, side: pick([-1, 0, 1] as const) });
        }
        const signClearing: Clearing | null = c % 3 === 2 ? { z0: carZ - 150, z1: carZ - 120, side: pick([-1, 0, 1] as const) } : null;
        out.push({ carZ, quality: c % 4, clearings, signClearing });
    }
    return out;
}

function snapshot(scenery: Scenery, carZ: number, rand: () => number): unknown {
    const internals = scenery as unknown as {
        mesh: InstancedMesh & { matrices: { px: number; py: number; pz: number; sx: number; sy: number; sz: number }[]; colors: ({ id: string } | undefined)[] };
        info: Float32Array;
        hitboxes: Float32Array;
        hitCount: number;
    };
    const base = Math.floor(-carZ / SPACING) - BEHIND;
    const parts: unknown[] = [];
    for (let i = 0; i < internals.mesh.matrices.length; i++) {
        const m = internals.mesh.matrices[i]!;
        if (m.sx === 0 && m.sy === 0 && m.sz === 0) continue;
        parts.push({
            row: base + Math.floor(i / PER_ROW),
            pos: [m.px, m.py, m.pz],
            size: [m.sx, m.sy, m.sz],
            tint: Number(internals.mesh.colors[i]!.id),
            info: Array.from(internals.info.slice(i * 4, i * 4 + 4)),
        });
    }
    const hitboxes: number[][] = [];
    for (let i = 0; i < internals.hitCount; i++) hitboxes.push(Array.from(internals.hitboxes.slice(i * 5, i * 5 + 5)));
    const spots = scenery.spots.map((s) => ({ x: s.x, y: s.y, z: s.z, side: s.side, phase: s.phase, tint: Number((s.tint as unknown as { id: string }).id) }));

    // Questions à la ville affichée : points tirés autour de la voiture, les deux côtés, du sol aux toits.
    const queries: unknown[] = [];
    for (let q = 0; q < 80; q++) {
        const at = { x: (rand() - 0.5) * 140, y: -5 + rand() * 70, z: carZ - 120 + rand() * 240 };
        const radius = [0.5, 1.5, 3, 6][q % 4]!;
        const side = q % 2 === 0 ? -1 : 1;
        const depth = [2, 8, 20][q % 3]!;
        const point = new Vector3(at.x, at.y, at.z);
        const facade = scenery.facade(side, at.z, depth);
        queries.push({
            at: [at.x, at.y, at.z],
            radius,
            collides: scenery.collides(point as never, radius),
            roof: scenery.roofAt(at.x, at.z),
            side,
            depth,
            facade: Number.isFinite(facade) ? facade : null,
        });
    }
    return { carZ, base, parts, hitboxes, spots, queries };
}

function scenarios(): unknown[] {
    const rand = rng(77);
    return cases().map((c) => {
        // Une ville neuve par scénario : pas de mémoire du scénario précédent (`lastBase`).
        const scenery = new Scenery(numberedPalette(), { uniforms: {} } as never);
        scenery.setQuality(c.quality);
        scenery.setClearings(c.clearings);
        scenery.setSignClearing(c.signClearing);
        scenery.update(c.carZ, 0, 0, 0, 0, 0, 0);
        return { quality: c.quality, clearings: c.clearings, signClearing: c.signClearing, ...(snapshot(scenery, c.carZ, rand) as object) };
    });
}

async function main(): Promise<void> {
    const check = process.argv.includes("--check");
    await verifyVendored();
    const data = { sillageCommit: SILLAGE_COMMIT, cases: scenarios() };
    // Quatre décimales (0,1 mm) : bien en deçà de la tolérance des tests, et un fichier trois fois plus court.
    const text = `${JSON.stringify(data, (_key, value) => (typeof value === "number" && !Number.isInteger(value) ? Math.round(value * 1e4) / 1e4 : value))}\n`;
    if (check) {
        const existing = await readFile(OUT, "utf8").catch(() => "");
        if (existing !== text) {
            console.error(`${OUT} diffère du recalcul`);
            process.exit(1);
        }
        console.log("scenery.json à jour");
        return;
    }
    await mkdir(join(ROOT, "crates/cf-world-neon/tests/golden"), { recursive: true });
    await writeFile(OUT, text);
    console.log(`écrit ${OUT} (${text.length} octets)`);
}

await main();
