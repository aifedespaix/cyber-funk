/**
 * Émulation de `AnalyserNode.getByteFrequencyData` (Web Audio), réglée comme Sillage (`engine/audio.ts`) :
 * `fftSize` 2048, `smoothingTimeConstant` 0,35 (`ANALYSER_SMOOTHING`), `minDecibels`/`maxDecibels` par défaut
 * (-100 / -30 dB).
 *
 * Le calcul suit la spécification Web Audio et, là où elle laisse de la marge (précision, arrondis), l'implémentation
 * de Chromium (`third_party/blink/renderer/modules/webaudio/realtime_analyser.cc`) :
 *
 * 1. les `fftSize` derniers échantillons mono ;
 * 2. fenêtre de Blackman (alpha = 0,16), coefficients calculés en double puis appliqués en float ;
 * 3. FFT réelle, module de chaque bin multiplié par `MAGNITUDE_SCALE` ;
 * 4. lissage temporel `X̂[k] = τ·X̂_prec[k] + (1 − τ)·|X[k]|`, stocké en float (le tampon de Chromium est en float) ;
 * 5. dB = 20·log10(X̂) en float, puis octet = `floor(255 · (dB − minDb) / (maxDb − minDb))` borné à [0, 255]
 *    (Chromium fait un `static_cast<unsigned char>`, donc une troncature).
 *
 * Écart connu : Chromium calcule sa FFT en float (f32), on la calcule ici en double. Un octet peut donc différer d'une
 * unité quand la valeur tombe tout près d'une frontière de troncature (d'où les tolérances des tests golden).
 *
 * Le lissage n'avance qu'à chaque lecture : un appel = une image de la scène, comme dans Sillage.
 */

export const FFT_SIZE = 2048;
export const BIN_COUNT = FFT_SIZE / 2;
/** `ANALYSER_SMOOTHING` de `engine/audio.ts`. */
export const SMOOTHING_TIME_CONSTANT = 0.35;
export const MIN_DECIBELS = -100;
export const MAX_DECIBELS = -30;
const BLACKMAN_ALPHA = 0.16;
/**
 * Échelle du module : `|X[k]| / fftSize`, avec X la DFT non normalisée. C'est la spécification, et c'est ce que fait
 * Chromium : `magnitude_scale = 1.0 / fft_size` dans `realtime_analyser.cc`, sur une FFT avant non mise à l'échelle
 * (PFFFT jusqu'en 2025 : `fft_frame_pffft.cc` ; RustFFT depuis : `rustfft_ffi.rs`, dont la reconstruction de la FFT
 * réelle donne la DFT standard). Vérifié sur les sources de Chromium le 2026-10-19 ; un sinus d'amplitude 0,01 centré
 * sur un bin lit donc 20·log10(0,01 · 0,42 / 2) ≈ -53,6 dB.
 */
export const MAGNITUDE_SCALE = 1 / FFT_SIZE;

export class AnalyserEmulation {
    /** Spectre en octets, réutilisé à chaque lecture (comme le `Uint8Array` de Sillage). */
    readonly bytes = new Uint8Array(BIN_COUNT);
    /** Modules lissés, en float comme dans Chromium. */
    private readonly smoothed = new Float32Array(BIN_COUNT);
    private readonly window = new Float32Array(FFT_SIZE);
    private readonly re = new Float64Array(FFT_SIZE);
    private readonly im = new Float64Array(FFT_SIZE);
    private readonly fft = new Radix2Fft(FFT_SIZE);

    constructor() {
        const a0 = 0.5 * (1 - BLACKMAN_ALPHA);
        const a1 = 0.5;
        const a2 = 0.5 * BLACKMAN_ALPHA;
        for (let i = 0; i < FFT_SIZE; i++) {
            const x = i / FFT_SIZE;
            this.window[i] = a0 - a1 * Math.cos(2 * Math.PI * x) + a2 * Math.cos(4 * Math.PI * x);
        }
    }

    /**
     * Spectre des `FFT_SIZE` échantillons qui précèdent l'indice `end` (exclu) de `pcm`. Avant le début du morceau,
     * le tampon de l'analyseur est rempli de zéros, comme à la création du nœud.
     */
    getByteFrequencyData(pcm: Float32Array, end: number): Uint8Array {
        const { re, im, window } = this;
        const start = end - FFT_SIZE;
        for (let i = 0; i < FFT_SIZE; i++) {
            const n = start + i;
            const sample = n >= 0 && n < pcm.length ? pcm[n]! : 0;
            // Produit en float, comme `p[i] *= float(window)` dans Chromium.
            re[i] = Math.fround(sample * window[i]!);
            im[i] = 0;
        }
        this.fft.forward(re, im);

        const k = SMOOTHING_TIME_CONSTANT;
        const rangeScale = 255 / (MAX_DECIBELS - MIN_DECIBELS);
        for (let i = 0; i < BIN_COUNT; i++) {
            const r = re[i]!;
            const j = im[i]!;
            const magnitude = Math.sqrt(r * r + j * j) * MAGNITUDE_SCALE;
            const value = k * this.smoothed[i]! + (1 - k) * magnitude;
            this.smoothed[i] = Number.isFinite(value) ? value : 0;
            const linear = this.smoothed[i]!;
            // `LinearToDecibels` de Chromium travaille en float : `20 * log10f(linear)`.
            const db = Math.fround(20 * Math.fround(Math.log10(linear)));
            const scaled = linear > 0 ? rangeScale * (db - MIN_DECIBELS) : 0;
            this.bytes[i] = scaled <= 0 ? 0 : scaled >= 255 ? 255 : Math.floor(scaled);
        }
        return this.bytes;
    }
}

/** FFT complexe radix 2 en place (Cooley-Tukey itérative), tables précalculées : sortie déterministe. */
class Radix2Fft {
    private readonly reverse: Uint32Array;
    private readonly cos: Float64Array;
    private readonly sin: Float64Array;

    constructor(private readonly size: number) {
        const bits = Math.log2(size);
        if (!Number.isInteger(bits)) throw new Error(`taille de FFT non puissance de 2 : ${size}`);
        this.reverse = new Uint32Array(size);
        for (let i = 0; i < size; i++) {
            let r = 0;
            for (let b = 0; b < bits; b++) r |= ((i >> b) & 1) << (bits - 1 - b);
            this.reverse[i] = r;
        }
        this.cos = new Float64Array(size / 2);
        this.sin = new Float64Array(size / 2);
        for (let i = 0; i < size / 2; i++) {
            this.cos[i] = Math.cos((2 * Math.PI * i) / size);
            this.sin[i] = -Math.sin((2 * Math.PI * i) / size);
        }
    }

    forward(re: Float64Array, im: Float64Array): void {
        const n = this.size;
        for (let i = 0; i < n; i++) {
            const j = this.reverse[i]!;
            if (j > i) {
                const tr = re[i]!;
                re[i] = re[j]!;
                re[j] = tr;
                const ti = im[i]!;
                im[i] = im[j]!;
                im[j] = ti;
            }
        }
        for (let len = 2; len <= n; len *= 2) {
            const half = len / 2;
            const stride = n / len;
            for (let i = 0; i < n; i += len) {
                for (let j = 0; j < half; j++) {
                    const wr = this.cos[j * stride]!;
                    const wi = this.sin[j * stride]!;
                    const a = i + j;
                    const b = a + half;
                    const xr = re[b]! * wr - im[b]! * wi;
                    const xi = re[b]! * wi + im[b]! * wr;
                    re[b] = re[a]! - xr;
                    im[b] = im[a]! - xi;
                    re[a] = re[a]! + xr;
                    im[a] = im[a]! + xi;
                }
            }
        }
    }
}
