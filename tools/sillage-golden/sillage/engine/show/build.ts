/**
 * Détecte la montée d'un morceau : l'énergie dépasse sa moyenne lente depuis quelques secondes. C'est la même logique
 * que celle de `cinema/cues.ts` (cue « build »), reprise ici pour que le spectacle n'en dépende pas ; les seuils sont
 * identiques et à garder alignés.
 */
const BUILD_GAP = 0.1;
const BUILD_SECONDS = 3;
const BUILD_REARM = 25;
const REFERENCE_SECONDS = 6;
const CEILING = 0.9;

export class BuildDetector {
  private reference = 0.4;
  private rising = 0;
  private since = BUILD_REARM;
  private armed = true;

  reset(): void {
    this.reference = 0.4;
    this.rising = 0;
    this.since = BUILD_REARM;
    this.armed = true;
  }

  /** Vrai pendant une seule image : une montée vient d'être reconnue (une fois par épisode). */
  update(intensity: number, dt: number): boolean {
    this.reference += (intensity - this.reference) * (1 - Math.exp(-dt / REFERENCE_SECONDS));
    const gap = intensity - this.reference;
    this.since += dt;
    if (this.armed && gap > BUILD_GAP && intensity < CEILING) this.rising += dt;
    else this.rising = Math.max(0, this.rising - dt * 2);
    if (this.armed && this.rising > BUILD_SECONDS) {
      this.armed = false;
      this.rising = 0;
      this.since = 0;
      return true;
    }
    if (!this.armed && this.since > BUILD_REARM && gap < 0.03) this.armed = true;
    return false;
  }
}
