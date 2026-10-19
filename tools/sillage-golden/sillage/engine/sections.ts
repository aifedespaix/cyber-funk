import { Color } from "three";
import type { BeatState } from "./beat";
import { followed } from "./live-palette";
import type { ScenePalette } from "./palette";

const FAST_SECONDS = 1.6;
const SLOW_SECONDS = 14;
const MOOD_SECONDS = 2.5;
const MIN_BETWEEN_CHANGES = 6;
/** Seuils des trois sections, avec une marge pour ne pas osciller à la frontière. */
const LOW_EDGE = 0.34;
const HIGH_EDGE = 0.66;
const HYSTERESIS = 0.06;
/** Mémoire (s) du plus haut volume récent : un morceau peu compressé atteint quand même le sommet. */
const CEILING_SECONDS = 45;
const DEFAULT_CEILING = 0.52;
const MIN_SPAN = 0.2;
const MAX_SPAN = 0.4;

export type Section = "calm" | "groove" | "peak";

/**
 * Lit la forme du morceau : l'énergie récente comparée à l'énergie des
 * dernières secondes distingue intro/break (calme), corps du morceau (groove)
 * et refrain/drop (sommet). La teinte du décor glisse du cyan (calme) au
 * violet (groove) puis au corail (sommet), sans à-coup.
 */
export class SectionTracker {
  /** Teinte ambiante courante, à injecter dans la brume et les bords de route. */
  readonly mood: Color;
  /** 0 (calme) → 1 (sommet), lissé. */
  intensity = 0.4;
  section: Section = "groove";

  private fast = 0;
  private slow = 0;
  private ceiling = DEFAULT_CEILING;
  private sinceChange = MIN_BETWEEN_CHANGES;
  private readonly calm: Color;
  private readonly groove: Color;
  private readonly peak: Color;

  constructor(palette: ScenePalette) {
    this.calm = followed(palette, "azure");
    this.groove = followed(palette, "violet");
    this.peak = followed(palette, "rose");
    this.mood = this.groove.clone();
  }

  /** Renvoie la nouvelle section si elle vient de changer, sinon `null`. */
  update(b: BeatState, dt: number): Section | null {
    const loud = b.loud;
    this.fast += (loud - this.fast) * (1 - Math.exp(-dt / FAST_SECONDS));
    this.slow += (loud - this.slow) * (1 - Math.exp(-dt / SLOW_SECONDS));

    // Le sommet de l'échelle suit le plus haut volume récent : 0,4 pour un morceau très fort (comme avant), moins pour un doux.
    this.ceiling = Math.max(this.fast, this.ceiling * Math.exp(-dt / CEILING_SECONDS));
    const span = clamp(this.ceiling - 0.12, MIN_SPAN, MAX_SPAN);
    const absolute = clamp((this.fast - 0.12) / span);
    const relative = clamp((this.fast - this.slow) / 0.15, -1, 1) * 0.5 + 0.5;
    const target = absolute * 0.6 + relative * 0.4;
    this.intensity += (target - this.intensity) * (1 - Math.exp(-dt / MOOD_SECONDS));

    const t = this.intensity;
    if (t < 0.5) this.mood.copy(this.calm).lerp(this.groove, t * 2);
    else this.mood.copy(this.groove).lerp(this.peak, (t - 0.5) * 2);

    this.sinceChange += dt;
    const next = this.classify(t);
    if (next === this.section || this.sinceChange < MIN_BETWEEN_CHANGES) return null;
    this.section = next;
    this.sinceChange = 0;
    return next;
  }

  /** Après un changement de titre : on repart d'une ambiance neutre. */
  reset(): void {
    this.fast = 0;
    this.slow = 0;
    this.ceiling = DEFAULT_CEILING;
    this.intensity = 0.4;
    this.section = "groove";
    this.sinceChange = MIN_BETWEEN_CHANGES;
  }

  private classify(t: number): Section {
    const h = HYSTERESIS;
    if (this.section === "calm") return t > LOW_EDGE + h ? (t > HIGH_EDGE + h ? "peak" : "groove") : "calm";
    if (this.section === "peak") return t < HIGH_EDGE - h ? (t < LOW_EDGE - h ? "calm" : "groove") : "peak";
    if (t < LOW_EDGE - h) return "calm";
    if (t > HIGH_EDGE + h) return "peak";
    return "groove";
  }
}

function clamp(x: number, min = 0, max = 1): number {
  return Math.max(min, Math.min(max, x));
}
