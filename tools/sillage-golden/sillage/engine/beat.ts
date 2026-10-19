/**
 * Détection de rythme en temps réel, sans dépendance.
 *
 * Principe : on suit le flux d'énergie positif dans la bande des basses (kick).
 * Un coup est retenu quand le flux dépasse sa moyenne glissante d'un facteur,
 * avec une période réfractaire. Le tempo vient de la médiane des intervalles
 * entre kicks, repliée dans 70–180 BPM.
 */

export interface BeatState {
  /** Vrai pendant une seule image, au moment d'un kick. */
  kick: boolean;
  /** Énergie des basses, lissée 0–1. */
  bass: number;
  /** Énergie des médiums (voix, synthés), lissée 0–1. */
  mid: number;
  /** Énergie des aigus (charleston, brillance), lissée 0–1. */
  high: number;
  /** Volume global pondéré (basses 50 %, médiums 35 %, aigus 15 %), 0–1 : la mesure commune des sections et de l'allure. */
  loud: number;
  /** Impulsion décroissante après chaque kick (1 → 0). */
  pulse: number;
  /** Nombre de kicks depuis le début. */
  count: number;
  /**
   * Compteur continu : il vaut `count` + une montée douce 0 → 1 pendant la
   * décroissance de `pulse`. Les effets qui avancent « d'un cran par temps »
   * s'y accrochent sans à-coup visible.
   */
  beat: number;
  bpm: number | null;
}

/** Mémoire (s) de la moyenne glissante du flux : indépendante de la fréquence d'images. */
const FLUX_MEMORY = 0.7;
/** Seuil absolu du flux, en unités par seconde (0,05 par image à 60 i/s). */
const MIN_FLUX_RATE = 3;
/** Plus petit pas de temps pris en compte : évite une division par un dt nul. */
const MIN_DT = 1 / 240;
const REFRACTORY = 0.24;
const MIN_INTERVAL = 0.28; // ~214 BPM
const MAX_INTERVAL = 1.2; // 50 BPM
const TEMPO_WINDOW = 12;
/** Silence (s) entre deux kicks au-delà duquel l'historique de tempo est oublié. */
const STALE_AFTER = 8;
// Le spectre s'affaiblit avec la fréquence : on rééquilibre pour viser 0–1.
const MID_GAIN = 1.7;
const HIGH_GAIN = 4.5;

type Range = readonly [number, number];

interface Band {
  from: number;
  to: number;
  previous: number;
  /** Moyenne glissante du flux positif, en unités par seconde. */
  mean: number;
  lastHit: number;
}

function makeBand(from: number, to: number): Band {
  return { from, to, previous: 0, mean: 0, lastHit: -Infinity };
}

export class BeatDetector {
  readonly state: BeatState = { kick: false, bass: 0, mid: 0, high: 0, loud: 0, pulse: 0, count: 0, beat: 0, bpm: null };

  private readonly bass = makeBand(40, 150);
  private readonly intervals: number[] = [];
  private lastKickTime: number | undefined;
  private binsReady = false;
  private bassRange: Range = [0, 0];
  private midRange: Range = [0, 0];
  private highRange: Range = [0, 0];

  /** Spectre 0–255 issu de l'AnalyserNode ; `now` et `dt` en secondes. */
  update(spectrum: Uint8Array | null, sampleRate: number, fftSize: number, now: number, dt: number): BeatState {
    const s = this.state;
    s.kick = false;
    s.pulse = Math.max(0, s.pulse - dt * 4.5);

    if (!spectrum) {
      const decay = Math.exp(-dt * 3);
      s.bass *= decay;
      s.mid *= decay;
      s.high *= decay;
      s.loud = loudness(s);
      s.beat = beatPosition(s);
      return s;
    }

    if (!this.binsReady) this.computeRanges(sampleRate, fftSize, spectrum.length);

    const bassNow = bandAverage(spectrum, this.bassRange);
    s.bass = smooth(s.bass, bassNow, dt, 0.04, 0.25);
    s.mid = smooth(s.mid, Math.min(1, bandAverage(spectrum, this.midRange) * MID_GAIN), dt, 0.05, 0.2);
    s.high = smooth(s.high, Math.min(1, bandAverage(spectrum, this.highRange) * HIGH_GAIN), dt, 0.03, 0.12);

    s.loud = loudness(s);

    if (this.detect(bassNow, now, dt)) {
      s.kick = true;
      s.pulse = 1;
      s.count++;
      this.registerKick(now);
    }
    s.beat = beatPosition(s);
    return s;
  }

  /**
   * Vrai si le flux positif des basses dépasse sa moyenne glissante. Le flux est ramené à une vitesse (unités par
   * seconde) : à 30 comme à 144 images/s, un même kick franchit le même seuil.
   */
  private detect(level: number, now: number, dt: number): boolean {
    const band = this.bass;
    const step = Math.max(dt, MIN_DT);
    const rate = Math.max(0, level - band.previous) / step;
    band.previous = level;

    const mean = band.mean;
    band.mean += (rate - mean) * (1 - Math.exp(-step / FLUX_MEMORY));

    const hit = rate > mean * 1.55 && rate > MIN_FLUX_RATE && now - band.lastHit > REFRACTORY;
    if (hit) band.lastHit = now;
    return hit;
  }

  private registerKick(now: number): void {
    const last = this.lastKickTime;
    this.lastKickTime = now;
    if (last === undefined) return;
    const interval = now - last;
    // Longue coupure (changement de DJ) : les intervalles de l'ancien tempo ne pèsent plus sur la médiane du nouveau.
    if (interval > STALE_AFTER) this.intervals.length = 0;
    if (interval < MIN_INTERVAL || interval > MAX_INTERVAL) return;
    this.intervals.push(interval);
    if (this.intervals.length > TEMPO_WINDOW) this.intervals.shift();
    if (this.intervals.length < 4) return;

    const sorted = [...this.intervals].sort((a, b) => a - b);
    let bpm = 60 / sorted[Math.floor(sorted.length / 2)]!;
    while (bpm < 70) bpm *= 2;
    while (bpm > 180) bpm /= 2;
    this.state.bpm = Math.round(bpm);
  }

  /** Convertit les bornes de fréquence en indices de bins, une seule fois. */
  private computeRanges(sampleRate: number, fftSize: number, binCount: number): void {
    const hz = sampleRate / fftSize;
    const toBin = (f: number) => Math.min(binCount - 1, Math.round(f / hz));
    this.bassRange = [toBin(this.bass.from), toBin(this.bass.to)];
    this.midRange = [toBin(300), toBin(2500)];
    this.highRange = [toBin(5000), toBin(12000)];
    this.binsReady = true;
  }

  /** Repart de zéro quand on change de piste : évite un faux tempo. */
  reset(): void {
    this.intervals.length = 0;
    this.lastKickTime = undefined;
    this.state.bpm = null;
  }
}

function bandAverage(spectrum: Uint8Array, [lo, hi]: Range): number {
  let sum = 0;
  for (let i = lo; i <= hi; i++) sum += spectrum[i] ?? 0;
  return sum / Math.max(1, hi - lo + 1) / 255;
}

function smooth(current: number, target: number, dt: number, attack: number, release: number): number {
  const tau = target > current ? attack : release;
  return current + (target - current) * (1 - Math.exp(-dt / tau));
}


/** Volume global : la même pondération pour les sections du morceau et pour l'allure. */
function loudness(s: BeatState): number {
  return s.bass * 0.5 + s.mid * 0.35 + s.high * 0.15;
}

function beatPosition(s: BeatState): number {
  const t = 1 - s.pulse;
  return s.count + t * t * (3 - 2 * t);
}
