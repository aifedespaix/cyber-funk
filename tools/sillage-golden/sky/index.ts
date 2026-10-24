/**
 * Données de référence (golden) du ciel de Sillage, pour les tests de `cf-world-neon` (J5-03).
 *
 * Fait tourner les fichiers TypeScript d'origine, copiés tels quels dans `sillage/` : `sky.ts` (le tirage des 6 370
 * étoiles), `planets.ts` (les astres et la géante à anneau), `shooting-stars.ts` (les départs d'étoiles filantes),
 * `sky-materials.ts` (l'axe de la Voie lactée) et `random.ts`. Ce qui est remplacé par des bouchons ne change rien aux
 * calculs : `three` (`stubs/three.ts`), la palette (des couleurs numérotées) et `watchPalette`. Écrit
 * `crates/cf-world-neon/tests/golden/sky.json`.
 *
 *   bun run tools/sillage-golden/sky/index.ts           régénère le fichier
 *   bun run tools/sillage-golden/sky/index.ts --check   recalcule et compare (code 1 si écart)
 *
 * Variable facultative `SILLAGE_DIR` : chemin de `apps/main/src/components/sillage` d'un clone de Sillage ; les copies
 * sont alors comparées octet par octet aux originaux.
 */
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { Points, type BufferGeometry } from "three";
import { numberedPalette } from "./sillage/engine/palette.ts";
import { Sky } from "./sillage/engine/sky.ts";

/** Version de Sillage dont les fichiers de `sillage/` sont copiés (branche `main`). */
const SILLAGE_COMMIT = "fd344819121f292dd0646289a898c049b2e9d67f";
/** Empreintes git (blob SHA-1) des copies conformes : une copie modifiée arrête le script. */
const VENDORED: Readonly<Record<string, string>> = {
    "engine/sky.ts": "182fcda578af370313bcf4060ac85609c7c3b347",
    "engine/sky-materials.ts": "7b873a9f96afcacb2b870b50038b8ac0067f544b",
    "engine/planets.ts": "4f6c40e509daa7f904feab98ff97d13a9074061f",
    "engine/shooting-stars.ts": "1f976cb0cb04e231fd0c45facf0e486ef73734a4",
    "engine/random.ts": "0a576ed902d7a42b25b1255166a5b6a68c7f20c8",
};

const TOOL_DIR = import.meta.dir;
const ROOT = resolve(TOOL_DIR, "../../..");
const OUT = join(ROOT, "crates/cf-world-neon/tests/golden/sky.json");

/** Nombre de départs d'étoiles filantes relevés. */
const LAUNCHES = 8;

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

type Vec = { x: number; y: number; z: number };
const vec = (v: Vec): number[] => [v.x, v.y, v.z];

/** Les étoiles relevées d'une couche : les dix premières, les dix dernières et une sur 97. */
function starSamples(geometry: BufferGeometry, layerIndex: number, count: number): unknown[] {
    const position = geometry.getAttribute("position").array;
    const size = geometry.getAttribute("aSize").array;
    const phase = geometry.getAttribute("aPhase").array;
    const color = geometry.getAttribute("aColor").array;
    const dim = layerIndex === 0 ? 0.9 : 1;
    const picked: number[] = [];
    for (let i = 0; i < count; i++) if (i < 10 || i >= count - 10 || i % 97 === 0) picked.push(i);
    return picked.map((i) => ({
        i,
        position: [position[i * 3]!, position[i * 3 + 1]!, position[i * 3 + 2]!],
        size: size[i]!,
        phase: phase[i]!,
        // `tint.r * dim` avec `Color.r = numéro + 1` (voir le bouchon) : on relit le numéro du rôle de couleur.
        color: Math.round(color[i * 3]! / dim) - 1,
    }));
}

async function data(): Promise<unknown> {
    const sky = new Sky(numberedPalette(), false, 1);
    await sky.load("textures");
    const internals = sky as unknown as {
        planets: { bodies: { group: { position: Vec; rotation: { z: number } }; surface: { scale: Vec }; rpm: number }[] };
        shooting: {
            streaks: { mesh: { scale: Vec; material: { uniforms: { uColor: { value: { id: number } } } } }; position: Vec; direction: Vec; speed: number; life: number; age: number }[];
        };
    };

    const layers = sky.group.children
        .filter((child): child is Points => child instanceof Points)
        .map((points, layerIndex) => {
            const count = points.geometry.getAttribute("aSize").array.length;
            return { count, samples: starSamples(points.geometry, layerIndex, count) };
        });

    // Départs successifs : après chacun, le trait est libéré (comme s'il avait fini de voler) pour que le suivant reparte du
    // pool ; l'étoile filante qui part toute seule n'est pas simulée (`update` n'est jamais appelé).
    const launches = [];
    for (let k = 0; k < LAUNCHES; k++) {
        sky.shoot();
        const streak = internals.shooting.streaks.find((s) => s.age < s.life)!;
        launches.push({
            k,
            position: vec(streak.position),
            direction: vec(streak.direction),
            speed: streak.speed,
            life: streak.life,
            scale: [streak.mesh.scale.x, streak.mesh.scale.y],
            tint: streak.mesh.material.uniforms.uColor.value.id,
        });
        streak.age = streak.life;
    }

    const planets = internals.planets.bodies.map((body) => ({
        center: vec(body.group.position),
        tilt: body.group.rotation.z,
        radius: body.surface.scale.x,
        rpm: body.rpm,
    }));
    const ring = sky.ringPlanet;
    return {
        sillageCommit: SILLAGE_COMMIT,
        layers,
        launches,
        planets,
        ringPlanet: ring && {
            center: vec(ring.center),
            radius: ring.radius,
            ringInner: ring.ringInner,
            ringOuter: ring.ringOuter,
            ringNormal: vec(ring.ringNormal),
        },
    };
}

async function main(): Promise<void> {
    const check = process.argv.includes("--check");
    await verifyVendored();
    // Quatre décimales (0,1 mm) : bien en deçà de la tolérance des tests, et un fichier bien plus court.
    const text = `${JSON.stringify(await data(), (_key, value) => (typeof value === "number" && !Number.isInteger(value) ? Math.round(value * 1e4) / 1e4 : value))}\n`;
    if (check) {
        const existing = await readFile(OUT, "utf8").catch(() => "");
        if (existing !== text) {
            console.error(`${OUT} diffère du recalcul`);
            process.exit(1);
        }
        console.log("sky.json à jour");
        return;
    }
    await mkdir(join(ROOT, "crates/cf-world-neon/tests/golden"), { recursive: true });
    await writeFile(OUT, text);
    console.log(`écrit ${OUT} (${text.length} octets)`);
}

await main();
