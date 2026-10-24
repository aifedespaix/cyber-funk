/**
 * Données de référence (golden) des enseignes de Sillage, pour les tests de `cf-world-neon` (J7-01).
 *
 * Fait tourner les fichiers TypeScript d'origine, copiés tels quels dans `sillage/` : `signs/formats.ts` (le jeu de
 * cartes de la route), `signs/show-signs.ts` (slogans courts, annonces du DJ), `signs/ads.ts` (le catalogue),
 * `signs/art-icons.ts` (`iconFor`) et `random.ts`. Écrit `crates/cf-world-neon/tests/golden/signs.json`.
 *
 *   bun run tools/sillage-golden/signs/index.ts           régénère le fichier
 *   bun run tools/sillage-golden/signs/index.ts --check   recalcule et compare (code 1 si écart)
 *
 * Variable facultative `SILLAGE_DIR` : chemin de `apps/main/src/components/sillage` d'un clone de Sillage ; les copies
 * sont alors comparées octet par octet aux originaux.
 */
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { ADS, CAR_FRONT_AD, CAR_SIDE_AD, NEON_ADS, WATCH_AD } from "./sillage/engine/signs/ads";
import { iconFor } from "./sillage/engine/signs/art-icons";
import { formatAt, type FormatId } from "./sillage/engine/signs/formats";
import { isShort, liveAd, liveAllowed, sloganIndex } from "./sillage/engine/signs/show-signs";
import type { ShowState } from "./sillage/engine/show/types";

/** Version de Sillage dont les fichiers de `sillage/` sont copiés (branche `main`). */
const SILLAGE_COMMIT = "fd344819121f292dd0646289a898c049b2e9d67f";
/** Empreintes git (blob SHA-1) des copies conformes : une copie modifiée arrête le script. */
const VENDORED: Readonly<Record<string, string>> = {
    "engine/random.ts": "0a576ed902d7a42b25b1255166a5b6a68c7f20c8",
    "engine/signs/formats.ts": "451c0a963de6f97f05a956068fb272714114ef57",
    "engine/signs/show-signs.ts": "b10d2da43ca9215cde34e5bb7e034c4af83461c6",
};

const TOOL_DIR = import.meta.dir;
const ROOT = resolve(TOOL_DIR, "../../..");
const OUT = join(ROOT, "crates/cf-world-neon/tests/golden/signs.json");

async function blob(path: string): Promise<string> {
    const local = await readFile(path);
    const header = new TextEncoder().encode(`blob ${local.length}\0`);
    return createHash("sha1").update(header).update(local).digest("hex");
}

async function verifyVendored(): Promise<void> {
    const sillageDir = process.env.SILLAGE_DIR;
    for (const [file, expected] of Object.entries(VENDORED)) {
        const actual = await blob(join(TOOL_DIR, "sillage", file));
        if (actual !== expected) throw new Error(`${file} a été modifié : ${actual} ≠ ${expected} (commit ${SILLAGE_COMMIT})`);
        if (sillageDir) {
            const original = await readFile(join(sillageDir, file));
            const local = await readFile(join(TOOL_DIR, "sillage", file));
            if (!Buffer.from(original).equals(Buffer.from(local))) throw new Error(`${file} diffère de l'original dans ${sillageDir}`);
        }
    }
}

const FORMATS: FormatId[] = ["holo", "gantry", "arm", "roof", "jumbo", "ghost", "neon", "cam", "hangCam", "hangAd"];

function show(dj: string, eventName: string, djOnAds: boolean, active = true): ShowState {
    return { active, dj, eventName, djOnAds } as unknown as ShowState;
}

function build(): string {
    const catalogue = [...ADS, WATCH_AD, CAR_FRONT_AD, CAR_SIDE_AD, ...NEON_ADS];
    const formats: string[] = [];
    for (let k = -16; k < 1200; k++) formats.push(formatAt(k));

    const spectacle = show("KRMN", "Démesure Live", true);
    const shows = [
        { name: "actif", dj: "KRMN", event: "Démesure Live", djOnAds: true, active: true },
        { name: "sans-nom", dj: "  ", event: "", djOnAds: true, active: true },
        { name: "pas-d-annonce", dj: "KRMN", event: "X", djOnAds: false, active: true },
        { name: "inactif", dj: "KRMN", event: "X", djOnAds: true, active: false },
    ];
    // Indice de slogan de départ de chaque publicité du catalogue, avec et sans spectacle.
    const slogans: unknown[] = [];
    for (let a = 0; a < catalogue.length; a++) {
        const ad = catalogue[a]!;
        for (const id of FORMATS) {
            for (let k = 0; k < 24; k++) {
                slogans.push([a, FORMATS.indexOf(id), k, sloganIndex(ad, k, id, undefined), sloganIndex(ad, k, id, spectacle)]);
            }
        }
    }
    const allowed: unknown[] = [];
    for (const s of shows) {
        const state = show(s.dj, s.event, s.djOnAds, s.active);
        for (const id of FORMATS) {
            for (let k = 0; k < 60; k++) {
                for (const [placed, live] of [[0, 0], [8, 1], [9, 2]] as const) {
                    allowed.push([shows.indexOf(s), FORMATS.indexOf(id), k, placed, live, liveAllowed(k, id, state, placed, live) ? 1 : 0]);
                }
            }
        }
    }
    const lives: unknown[] = [];
    for (const id of FORMATS) {
        for (let k = 0; k < 60; k++) {
            const ad = liveAd(k, id, show("  krmn   le  très long nom de dj qui dépasse", "démesure live", true));
            lives.push({ format: id, k, kind: ad.kind, accent: ad.accent, slogans: ad.slogans, role: ad.live?.role, name: ad.live?.name, event: ad.live?.event });
        }
    }
    const icons: unknown[] = [];
    catalogue.forEach((ad, a) => ad.slogans.forEach((text, i) => icons.push({ ad: a, slogan: i, icon: iconFor(ad.kind, text), short: isShort(text) })));
    const ads = catalogue.map((ad) => ({ kind: ad.kind, accent: ad.accent, slogans: ad.slogans.length }));
    return `${JSON.stringify({ commit: SILLAGE_COMMIT, formats, ads, slogans, allowed, lives, icons })}\n`;
}

async function run(): Promise<void> {
    await verifyVendored();
    const text = build();
    if (process.argv.includes("--check")) {
        const existing = await readFile(OUT, "utf8");
        if (existing !== text) {
            console.error("signs.json diffère du résultat de Sillage");
            process.exit(1);
        }
        console.log("signs.json à jour");
        return;
    }
    await mkdir(join(ROOT, "crates/cf-world-neon/tests/golden"), { recursive: true });
    await writeFile(OUT, text);
    console.log(`écrit ${OUT} (${text.length} octets)`);
}

await run();
