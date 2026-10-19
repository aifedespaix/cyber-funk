import type { Color } from "three";

/**
 * Limiteur de sécurité photosensible : tout flash plein écran ou toute forte variation de luminance du spectacle passe par
 * ici. Garanties (WCAG 2.3.1) : au plus 3 flashs par seconde glissante, aucune variation plus rapide que `MIN_FADE`
 * (montée comme descente), amplitude plafonnée, jamais de flash rouge saturé, et rien du tout quand les flashs sont coupés
 * ou que le mode respiration est demandé.
 */

export const MAX_FLASHES_PER_SECOND = 3;
/** Durée minimale (s) d'une montée ou d'une descente complète à l'amplitude maximale. */
export const MIN_FADE = 0.08;
/** Plafond d'un flash plein écran (lumière ajoutée, 0 → 1). */
export const MAX_FLASH_PEAK = 0.4;
/** Plafond de la vague de lumière de changement de niveau. */
export const MAX_SWEEP_PEAK = 0.14;
/** Écart minimal (s) entre deux départs, pour ne pas les grouper. */
const MIN_GAP = 0.25;
/** Un trou d'image plus long ne raccourcit pas un fondu : le temps du limiteur est borné. */
const MAX_STEP = 1 / 30;
const WINDOW = 1;
const CHANNELS = 2;

/** Les deux voies : le flash bref, et la vague lente de changement de niveau. */
export const FX_FLASH = 0;
export const FX_SWEEP = 1;
const CAPS = [MAX_FLASH_PEAK, MAX_SWEEP_PEAK] as const;

/** Un rouge saturé plein écran est interdit : une teinte à dominante rouge est ramenée vers un blanc froid. */
export function defuseRed(color: Color): Color {
  const other = Math.max(color.g, color.b);
  if (color.r > 0.15 && color.r > other * 1.4) {
    const grey = color.r * 0.55;
    const k = Math.min(1, (color.r / Math.max(other, 1e-4) - 1.4) / 1.4 + 0.6);
    color.r += (grey - color.r) * k;
    color.g += (Math.max(other, grey) - color.g) * k;
    color.b += (Math.max(other, grey) - color.b) * k;
  }
  return color;
}

export class FlashLimiter {
  private allowed = true;
  private readonly starts = new Float64Array(MAX_FLASHES_PER_SECOND).fill(Number.NEGATIVE_INFINITY);
  private cursor = 0;
  private lastStart = Number.NEGATIVE_INFINITY;
  private readonly level = new Float64Array(CHANNELS);
  private readonly peak = new Float64Array(CHANNELS);
  private readonly rise = new Float64Array(CHANNELS);
  private readonly fall = new Float64Array(CHANNELS);
  private readonly rising = [false, false];

  /** Flashs permis seulement si le réglage est actif et que le mode respiration ne l'interdit pas. */
  setPolicy(flashes: boolean, breathing: boolean): void {
    const next = flashes && !breathing;
    if (next === this.allowed) return;
    this.allowed = next;
    if (next) return;
    for (let c = 0; c < CHANNELS; c++) {
      this.rising[c] = false;
      this.fall[c] = this.level[c]! / MIN_FADE;
    }
  }

  get permitted(): boolean {
    return this.allowed;
  }

  /** Demande un flash : renvoie vrai s'il est accordé. `attack` et `decay` sont relevés à `MIN_FADE` au moins. */
  trigger(channel: number, peak: number, attack: number, decay: number, now: number): boolean {
    if (!this.allowed || peak <= 0.001) return false;
    if (this.level[channel]! > 0.02 || this.rising[channel]) return false;
    if (now - this.lastStart < MIN_GAP) return false;
    if (now - this.starts[this.cursor]! < WINDOW) return false;
    this.starts[this.cursor] = now;
    this.cursor = (this.cursor + 1) % MAX_FLASHES_PER_SECOND;
    this.lastStart = now;
    const p = Math.min(peak, CAPS[channel]!);
    this.peak[channel] = p;
    this.rising[channel] = true;
    this.rise[channel] = p / Math.max(MIN_FADE, attack);
    this.fall[channel] = p / Math.max(MIN_FADE, decay);
    return true;
  }

  step(dt: number): void {
    const d = Math.min(dt, MAX_STEP);
    for (let c = 0; c < CHANNELS; c++) {
      if (this.rising[c]) {
        this.level[c] = this.level[c]! + this.rise[c]! * d;
        if (this.level[c]! >= this.peak[c]!) {
          this.level[c] = this.peak[c]!;
          this.rising[c] = false;
        }
      } else if (this.level[c]! > 0) {
        this.level[c] = Math.max(0, this.level[c]! - this.fall[c]! * d);
      }
    }
  }

  value(channel: number): number {
    return this.level[channel]!;
  }
}
