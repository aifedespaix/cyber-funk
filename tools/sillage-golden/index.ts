/**
 * Données de référence (golden) de l'analyse audio de Sillage, pour les tests de `cf-analysis` (J1-04, J1-05).
 *
 * Pour chaque piste CC0 de `assets/music/` : décodage en PCM mono 48 kHz, émulation de l'`AnalyserNode` de Sillage,
 * puis passage, image par image à 60 i/s, dans les fichiers TypeScript d'origine de Sillage (`beat.ts`,
 * `sections.ts`, `show/build.ts`, copiés tels quels dans `sillage/`), dans l'ordre de la boucle de
 * `sillage-scene.ts` : `beat.update` → `sections.update` → `BuildDetector.update` (via `DropConductor`).
 *
 *   bun run tools/sillage-golden/index.ts           régénère crates/cf-analysis/tests/golden/*.json
 *   bun run tools/sillage-golden/index.ts --check   recalcule et compare aux fichiers existants (code 1 si écart)
 *
 * Variable facultative `SILLAGE_DIR` : chemin de `apps/main/src/components/sillage` d'un clone de Sillage ; les copies
 * de `sillage/` sont alors comparées octet par octet aux originaux.
 */
import { createHash } from "node:crypto";
import { readdir, readFile, mkdir, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { AnalyserEmulation, BIN_COUNT, FFT_SIZE, MAGNITUDE_SCALE, MAX_DECIBELS, MIN_DECIBELS, SMOOTHING_TIME_CONSTANT } from "./analyser.ts";
import { decodeMono48k, SAMPLE_RATE } from "./decode.ts";
import { formatJson, type Json } from "./json.ts";
import { BeatDetector, type BeatState } from "./sillage/engine/beat.ts";
import { SectionTracker, type Section } from "./sillage/engine/sections.ts";
import { BuildDetector } from "./sillage/engine/show/build.ts";
import type { ScenePalette } from "./sillage/engine/palette.ts";

/** Version de Sillage dont les fichiers de `sillage/` sont copiés (branche `main`). */
const SILLAGE_COMMIT = "fd344819121f292dd0646289a898c049b2e9d67f";
/** Empreintes git (blob SHA-1) des copies, telles qu'au commit ci-dessus : une copie modifiée arrête le script. */
const VENDORED: Readonly<Record<string, string>> = {
    "engine/beat.ts": "d5bae734eb7066ad726ac2aaceb4c06242efda3b",
    "engine/sections.ts": "ac63a6b01b06b5a5166d2750748deb3ff708768b",
    "engine/show/build.ts": "d2d9946dc6d52a6e952d08b672bb698fd457c86d",
};

/** Pas fixe : 800 échantillons à 48 kHz = exactement 1/60 s, la cadence d'images de référence de Sillage. */
const STEP_RATE = 60;
const STEP_SAMPLES = SAMPLE_RATE / STEP_RATE;
const DT = 1 / STEP_RATE;
/** Échantillonnage des grandeurs continues (intensité, bandes) : toutes les 0,25 s. */
const SAMPLE_EVERY = 15;
/** Instantanés du spectre en octets (pas d'analyse) : de quoi valider l'émulation de l'analyseur bin par bin. */
const SPECTRUM_STEPS = [600, 3600, 7200, 10800, 14400];
const DECIMALS = 5;

const TOOL_DIR = import.meta.dir;
const ROOT = resolve(TOOL_DIR, "../..");
const MUSIC_DIR = join(ROOT, "assets/music");
const OUT_DIR = join(ROOT, "crates/cf-analysis/tests/golden");

async function main(): Promise<void> {
    const check = process.argv.includes("--check");
    await verifyVendored();

    const tracks = (await readdir(MUSIC_DIR)).filter((f) => f.toLowerCase().endsWith(".mp3")).sort();
    if (tracks.length === 0) throw new Error(`aucune piste MP3 dans ${MUSIC_DIR}`);
    await mkdir(OUT_DIR, { recursive: true });

    let mismatches = 0;
    for (const file of tracks) {
        const result = await analyseTrack(file);
        const text = formatJson(result.json);
        const out = join(OUT_DIR, `${slug(file)}.json`);
        if (check) {
            const previous = await readFile(out, "utf8").catch(() => "");
            const same = previous === text;
            if (!same) mismatches++;
            console.log(`${same ? "identique" : "DIFFÉRENT"}  ${out}`);
        } else {
            await writeFile(out, text);
            console.log(`écrit      ${out}`);
        }
        console.log(`           ${result.line}`);
    }
    if (mismatches > 0) {
        console.error(`${mismatches} fichier(s) ne correspondent pas : relancer sans --check pour régénérer.`);
        process.exit(1);
    }
}

interface TrackResult {
    json: Json;
    line: string;
}

async function analyseTrack(file: string): Promise<TrackResult> {
    const path = join(MUSIC_DIR, file);
    const source = await readFile(path);
    const decoded = await decodeMono48k(path);
    const pcm = decoded.pcm;
    const steps = Math.floor(pcm.length / STEP_SAMPLES);

    const analyser = new AnalyserEmulation();
    const beat = new BeatDetector();
    // La palette ne sert qu'à la teinte `mood` (hors données de référence) : voir `sillage/engine/live-palette.ts`.
    const sections = new SectionTracker({} as ScenePalette);
    const build = new BuildDetector();

    const kicks: number[] = [];
    const bpmChanges: Json[] = [];
    const sectionChanges: Json[] = [{ step: 0, section: sections.section }];
    const builds: number[] = [];
    const sampled = { intensity: [] as number[], bass: [] as number[], mid: [] as number[], high: [] as number[], loud: [] as number[] };
    const spectra: Json[] = [];
    const bpmSteps = new Map<number, number>();
    const sectionSteps: Record<Section, number> = { calm: 0, groove: 0, peak: 0 };
    let lastBpm: number | null = null;

    // Pas i : la fenêtre de l'analyseur se termine à l'échantillon i·800 (exclu), soit t = i/60 s dans la piste.
    for (let step = 1; step <= steps; step++) {
        const now = step / STEP_RATE;
        const spectrum = analyser.getByteFrequencyData(pcm, step * STEP_SAMPLES);
        const b: BeatState = beat.update(spectrum, SAMPLE_RATE, FFT_SIZE, now, DT);
        if (b.kick) kicks.push(step);
        if (b.bpm !== lastBpm) {
            lastBpm = b.bpm;
            bpmChanges.push({ step, bpm: b.bpm });
        }
        if (b.bpm !== null) bpmSteps.set(b.bpm, (bpmSteps.get(b.bpm) ?? 0) + 1);

        const changed = sections.update(b, DT);
        if (changed) sectionChanges.push({ step, section: changed });
        sectionSteps[sections.section]++;
        // Le chef du drop lit `sections.intensity` après `sections.update`, à la même image (voir `sillage-scene.ts`).
        if (build.update(sections.intensity, DT)) builds.push(step);

        if (step % SAMPLE_EVERY === 0) {
            sampled.intensity.push(round(sections.intensity));
            sampled.bass.push(round(b.bass));
            sampled.mid.push(round(b.mid));
            sampled.high.push(round(b.high));
            sampled.loud.push(round(b.loud));
        }
        if (SPECTRUM_STEPS.includes(step)) spectra.push({ step, bytes: Array.from(spectrum) });
    }

    let steadyBpm: number | null = null;
    let steadySteps = 0;
    for (const [bpm, count] of [...bpmSteps].sort((a, b) => a[0] - b[0])) {
        if (count > steadySteps) {
            steadyBpm = bpm;
            steadySteps = count;
        }
    }
    const firstBpm = bpmChanges.find((c) => (c as { bpm: number | null }).bpm !== null) as { step: number } | undefined;
    const seconds = (n: number) => round(n / STEP_RATE, 3);

    const json: Json = {
        schema: 1,
        generator: "tools/sillage-golden/index.ts",
        sillage: {
            repository: "aifedespaix/aifedespaix.com",
            commit: SILLAGE_COMMIT,
            files: Object.fromEntries(Object.entries(VENDORED).map(([f, sha]) => [`apps/main/src/components/sillage/${f}`, sha])),
        },
        track: {
            file: file,
            sha256: createHash("sha256").update(source).digest("hex"),
            sourceSampleRate: decoded.sourceRate,
            sourceChannels: decoded.channels,
            pcmSamples: pcm.length,
            pcmSha256: createHash("sha256").update(new Uint8Array(pcm.buffer, pcm.byteOffset, pcm.byteLength)).digest("hex"),
            durationSeconds: round(pcm.length / SAMPLE_RATE, 3),
        },
        analysis: {
            sampleRate: SAMPLE_RATE,
            stepRate: STEP_RATE,
            stepSamples: STEP_SAMPLES,
            stepNote: "pas i (1..steps) : fenêtre terminée à l'échantillon i*stepSamples (exclu), now = i/stepRate, dt = 1/stepRate",
            steps,
            analyser: {
                fftSize: FFT_SIZE,
                frequencyBinCount: BIN_COUNT,
                smoothingTimeConstant: SMOOTHING_TIME_CONSTANT,
                minDecibels: MIN_DECIBELS,
                maxDecibels: MAX_DECIBELS,
                window: "blackman, alpha 0.16",
                magnitudeScale: MAGNITUDE_SCALE,
                downmix: "mono = (gauche + droite) / 2",
            },
            sampleEvery: SAMPLE_EVERY,
            sampleNote: "samples.*[k] correspond au pas (k + 1) * sampleEvery, valeur après la mise à jour de ce pas",
            decimals: DECIMALS,
        },
        summary: {
            kickCount: kicks.length,
            firstBpmStep: firstBpm?.step ?? null,
            steadyBpm,
            steadyBpmSeconds: seconds(steadySteps),
            // Les cinq valeurs de BPM tenues le plus longtemps (à égalité : la plus basse d'abord).
            bpmSeconds: [...bpmSteps]
                .sort((a, b) => b[1] - a[1] || a[0] - b[0])
                .slice(0, 5)
                .map(([bpm, count]) => ({ bpm, seconds: seconds(count) })),
            finalBpm: lastBpm,
            sectionSeconds: { calm: seconds(sectionSteps.calm), groove: seconds(sectionSteps.groove), peak: seconds(sectionSteps.peak) },
            sectionChanges: sectionChanges.length - 1,
            buildCount: builds.length,
        },
        kicks,
        bpm: bpmChanges,
        sections: sectionChanges,
        builds,
        samples: sampled,
        spectra,
    };

    const line = `${kicks.length} kicks, BPM stable ${steadyBpm ?? "-"} (${seconds(steadySteps)} s), final ${lastBpm ?? "-"}, `
        + `${sectionChanges.length - 1} changements de section, ${builds.length} montée(s)`;
    return { json, line };
}

/** Les copies de `sillage/` doivent rester conformes au commit de référence (et à `SILLAGE_DIR` si fourni). */
async function verifyVendored(): Promise<void> {
    const sillageDir = process.env.SILLAGE_DIR;
    for (const [file, expected] of Object.entries(VENDORED)) {
        const copy = await readFile(join(TOOL_DIR, "sillage", file));
        const actual = gitBlobSha(copy);
        if (actual !== expected) {
            throw new Error(`sillage/${file} ne correspond plus à Sillage ${SILLAGE_COMMIT.slice(0, 7)} (blob ${actual}, attendu ${expected})`);
        }
        if (sillageDir) {
            const original = await readFile(join(sillageDir, file));
            if (!original.equals(copy)) throw new Error(`sillage/${file} diffère de ${join(sillageDir, file)}`);
        }
    }
}

function gitBlobSha(content: Buffer): string {
    return createHash("sha1").update(`blob ${content.length}\0`).update(content).digest("hex");
}

function slug(file: string): string {
    return file
        .replace(/\.[^.]+$/, "")
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "-")
        .replace(/^-|-$/g, "");
}

function round(x: number, decimals = DECIMALS): number {
    const f = 10 ** decimals;
    const r = Math.round(x * f) / f;
    // Évite « -0 » dans le JSON.
    return r === 0 ? 0 : r;
}

await main();
