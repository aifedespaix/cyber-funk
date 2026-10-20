/**
 * Kicks que Sillage détecte sur un bruit blanc stationnaire (J1-04, test `white_noise_matches_sillage` de
 * `crates/cf-analysis/tests/synthetic.rs`).
 *
 * Le bruit est celui de `mulberry32(12345)`, amplitude crête 0,3, 30 s à 48 kHz ; le test Rust le régénère à
 * l'identique avec le même générateur. Le pas est celui du golden (800 échantillons, 60 i/s). Un bruit stationnaire
 * n'a pas de tempo, mais `beat.ts` y voit des kicks : le portage doit en voir les mêmes.
 *
 *   bun run tools/sillage-golden/noise.ts
 */

import { AnalyserEmulation, FFT_SIZE } from "./analyser.ts";
import { BeatDetector } from "./sillage/engine/beat.ts";

const SAMPLE_RATE = 48_000;
const STEP_SAMPLES = 800;
const STEP_RATE = 60;

function mulberry32(seed: number): () => number {
    let a = seed;
    return () => {
        a |= 0;
        a = (a + 0x6d2b79f5) | 0;
        let t = Math.imul(a ^ (a >>> 15), 1 | a);
        t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
        return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
}

const random = mulberry32(12345);
const pcm = new Float32Array(30 * SAMPLE_RATE);
for (let i = 0; i < pcm.length; i++) pcm[i] = 0.3 * (random() * 2 - 1);

const analyser = new AnalyserEmulation();
const beat = new BeatDetector();
const kicks: number[] = [];
for (let step = 1; step <= Math.floor(pcm.length / STEP_SAMPLES); step++) {
    const spectrum = analyser.getByteFrequencyData(pcm, step * STEP_SAMPLES);
    if (beat.update(spectrum, SAMPLE_RATE, FFT_SIZE, step / STEP_RATE, 1 / STEP_RATE).kick) kicks.push(step);
}
console.log(JSON.stringify({ kicks, bpm: beat.state.bpm }));
