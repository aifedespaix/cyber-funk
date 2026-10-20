/**
 * Données de référence (golden) de la route de Sillage, pour les tests de `cf-world-neon` (J5-01).
 *
 * Fait tourner les fichiers TypeScript d'origine, copiés tels quels dans `sillage/` : le plan de route
 * (`route-plan.ts`, `flight/floors.ts`) et les ponts (`bridges.ts`, `random.ts`). Écrit
 * `crates/cf-world-neon/tests/golden/route.json`.
 *
 *   bun run tools/sillage-golden/road/index.ts           régénère le fichier
 *   bun run tools/sillage-golden/road/index.ts --check   recalcule et compare (code 1 si écart)
 *
 * Variable facultative `SILLAGE_DIR` : chemin de `apps/main/src/components/sillage` d'un clone de Sillage ; les copies
 * sont alors comparées octet par octet aux originaux.
 */
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { bridgeSpan, bridgeZ, isNearBridge } from "./sillage/engine/bridges.ts";
import { FLOORS } from "./sillage/engine/flight/floors.ts";
import { hash } from "./sillage/engine/random.ts";
import { floorAt, floorOf, floorTime, invalidateRoute, pilotHint, planRoute, routeFloor, type RouteInput } from "./sillage/engine/route-plan.ts";

/** Version de Sillage dont les fichiers de `sillage/` sont copiés (branche `main`). */
const SILLAGE_COMMIT = "fd344819121f292dd0646289a898c049b2e9d67f";
/** Empreintes git (blob SHA-1) des copies conformes : une copie modifiée arrête le script. */
const VENDORED: Readonly<Record<string, string>> = {
    "engine/route-plan.ts": "c3ac51c462183fd8bcfb27322b07c13e166a6b13",
    "engine/bridges.ts": "7991adee65466ad547f10e0c1288f2c96707c913",
    "engine/random.ts": "0a576ed902d7a42b25b1255166a5b6a68c7f20c8",
    "engine/flight/floors.ts": "20894b6d8a7bfb3655261948131cf5065cdda08c",
};

const TOOL_DIR = import.meta.dir;
const ROOT = resolve(TOOL_DIR, "../../..");
const OUT = join(ROOT, "crates/cf-world-neon/tests/golden/route.json");

/** Nombre de scénarios de plan de route. */
const CASES = 240;

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

async function gitBlobSha(bytes: Uint8Array): Promise<string> {
    const header = new TextEncoder().encode(`blob ${bytes.length}\0`);
    return createHash("sha1").update(header).update(bytes).digest("hex");
}

async function verifyVendored(): Promise<void> {
    const sillageDir = process.env.SILLAGE_DIR;
    for (const [file, expected] of Object.entries(VENDORED)) {
        const local = await readFile(join(TOOL_DIR, "sillage", file));
        const actual = await gitBlobSha(local);
        if (actual !== expected) throw new Error(`${file} a été modifié : ${actual} ≠ ${expected} (commit ${SILLAGE_COMMIT})`);
        if (sillageDir) {
            const original = await readFile(join(sillageDir, file));
            if (!Buffer.from(original).equals(Buffer.from(local))) throw new Error(`${file} diffère de l'original dans ${sillageDir}`);
        }
    }
}

function scenarios(): unknown[] {
    const rand = rng(20260620);
    const pick = <T>(values: readonly T[]): T => values[Math.floor(rand() * values.length)]!;
    const cases: unknown[] = [];
    for (let c = 0; c < CASES; c++) {
        const carZ = -Math.round(rand() * 400000 * 100) / 100;
        const rate = pick([5, 5, 3.4, 2, 0.5]);
        const input: RouteInput = {
            carX: pick([0, -2.1, 2.1, Math.round((rand() * 9.2 - 4.6) * 10) / 10]),
            carZ,
            speed: Math.round((8 + rand() * 52) * 10) / 10,
            floor: Math.floor(rand() * 7),
            delay: pick([0, 0, 0.3, 0.9, 1.4]),
            canChange: rand() < 0.7,
            lookahead: pick([110, 110, 60, 150]),
        };
        if (rand() < 0.3) input.physical = Math.floor(rand() * 7);
        const count = Math.floor(rand() * 13);
        const pickups: { kind: string; x: number; z: number; y: number }[] = [];
        for (let i = 0; i < count; i++) {
            const ahead = Math.round((-4 + rand() * 135) * 100) / 100;
            const reuse = rand() < 0.15 && pickups.length > 0;
            const previous = pickups[pickups.length - 1];
            const y = reuse && previous ? previous.y : rand() < 0.7 ? 0 : FLOORS[Math.floor(rand() * FLOORS.length)]! + 0.9 + (rand() - 0.5) * 3;
            pickups.push({
                kind: rand() < 0.6 ? "bonus" : "malus",
                // Abscisse sur une grille de 0,1 m : les égalités exactes (`gap === 0`) arrivent.
                x: Math.round((rand() * 9.2 - 4.6) * 10) / 10,
                z: reuse && previous ? previous.z : carZ - ahead,
                y: Math.round(y * 100) / 100,
            });
        }
        pilotHint.rate = rate;
        invalidateRoute();
        const route = planRoute(
            pickups.map((p) => ({ kind: p.kind as "bonus" | "malus", x: p.x, z: p.z, y: p.y })),
            input,
        );
        const n = route.count;
        const d = route.dodgeCount;
        cases.push({
            rate,
            input: { ...input, physical: input.physical ?? null },
            pickups,
            route: {
                count: n,
                aim: Array.from(route.aim.slice(0, n)),
                time: Array.from(route.time.slice(0, n)),
                floor: Array.from(route.floor.slice(0, n)),
                dodgeCount: d,
                dodgeX: Array.from(route.dodgeX.slice(0, d)),
                dodgeTime: Array.from(route.dodgeTime.slice(0, d)),
            },
            routeFloor: routeFloor(route, input.floor),
        });
    }
    return cases;
}

function floorSamples(): unknown {
    const heights = [0, 1, 2.4, 2.5, 3, 4.4, 5, 8.9, 11, 15.9, 16, 21.9, 22, 30.4, 33, 39.4, 42, 60];
    const altitudes = [0, 1.4, 1.5, 3.5, 6, 8, 12, 15, 18, 21, 25, 29.5, 34, 38.5, 50];
    const times: number[][] = [];
    for (let from = 0; from <= 6; from++) for (let to = 0; to <= 6; to++) times.push([from, to, floorTime(from, to)]);
    return {
        floorOf: heights.map((y) => [y, floorOf(y)]),
        floorAt: altitudes.map((a) => [a, floorAt(a)]),
        floorTime: times,
    };
}

function bridges(): unknown {
    const cells = Array.from({ length: 260 }, (_, c) => ({
        cell: c,
        z: bridgeZ(c),
        // Les deux tirages de `Bridges.update` : rang de la teinte (6 teintes) et graine des fenêtres.
        tint: Math.floor(hash(c, 42) * 6),
        seed: Math.floor(hash(c, 43) * 1000),
    }));
    const near: unknown[] = [];
    const rand = rng(41);
    for (let i = 0; i < 400; i++) {
        const z = -(rand() * 500000);
        const margin = [0, 5, 40][i % 3]!;
        near.push([z, margin, isNearBridge(z, margin)]);
    }
    // Des points collés aux arches : l'emprise se joue à quelques mètres.
    for (let c = 0; c < 40; c++) {
        for (const dz of [-9, -8.1, -7.9, 0, 7.9, 8.1, 9]) near.push([bridgeZ(c) + dz, 0, isNearBridge(bridgeZ(c) + dz, 0)]);
    }
    return { cells, near, span: bridgeSpan() };
}

async function main(): Promise<void> {
    const check = process.argv.includes("--check");
    await verifyVendored();
    const data = { sillageCommit: SILLAGE_COMMIT, floors: floorSamples(), bridges: bridges(), routes: scenarios() };
    const text = `${JSON.stringify(data)}\n`;
    if (check) {
        const existing = await readFile(OUT, "utf8").catch(() => "");
        if (existing !== text) {
            console.error(`${OUT} diffère du recalcul`);
            process.exit(1);
        }
        console.log("route.json à jour");
        return;
    }
    await mkdir(join(ROOT, "crates/cf-world-neon/tests/golden"), { recursive: true });
    await writeFile(OUT, text);
    console.log(`écrit ${OUT} (${text.length} octets)`);
}

await main();
