import { ACTS, type ActNumbers, OPEN_GATES } from "./acts";
import type { ShowGates, ShowLevel } from "./types";

type Writable<T> = { -readonly [K in keyof T]: T[K] };
type MutableGates = Writable<Omit<ShowGates, "fx">> & { fx: Writable<ShowGates["fx"]> };

/** Constante de temps (s) du glissement des nombres : un changement de niveau ou de variation se fond, il ne saute pas. */
const SMOOTH_SECONDS = 3;
/** Ces nombres ne quittent pas zéro avant que le niveau suivant ne commence : pas de vol ni d'astéroïde en avance. */
const STAYS_ZERO: readonly (keyof ActNumbers)[] = ["flightRate", "meteorRate"];


/** Ce que le blender doit viser : le niveau dont on prend les listes, et la part du chemin vers le niveau suivant. */
function target(key: keyof ActNumbers, source: ShowLevel, mix: number): number {
  const own = ACTS[source]!.numbers[key];
  const next = ACTS[Math.min(5, source + 1)]!.numbers[key];
  if (own === 0 && STAYS_ZERO.includes(key)) return 0;
  return own + (next - own) * mix;
}

function glide(current: number, key: keyof ActNumbers, source: ShowLevel, mix: number, k: number): number {
  return current + (target(key, source, mix) - current) * k;
}

/**
 * Fabrique les portes de l'image : les listes et les booléens viennent de l'acte `source`, les nombres glissent vers
 * leur valeur cible. L'objet est unique et mis à jour sur place (aucune allocation par image).
 */
export class GateBlender {
  readonly gates: ShowGates;
  private readonly out: MutableGates;

  constructor() {
    this.out = { ...OPEN_GATES, fx: { ...OPEN_GATES.fx } };
    this.gates = this.out;
  }

  /** Portes « tout permis » (spectacle inactif). */
  open(): void {
    const o = this.out;
    Object.assign(o, OPEN_GATES, { fx: o.fx });
    Object.assign(o.fx, OPEN_GATES.fx);
  }

  /**
   * @param source niveau dont on prend les listes et les booléens
   * @param mix 0 → 1 : avancement vers les nombres du niveau suivant
   * @param snap vrai : saute à la cible (démarrage, saut de niveau), sinon glisse
   */
  update(source: ShowLevel, mix: number, dt: number, snap: boolean): void {
    const act = ACTS[source]!;
    const o = this.out;
    o.maxFloor = act.maxFloor;
    o.flightOnDropOnly = act.flightOnDropOnly;
    o.tricks = act.tricks;
    o.stuntTiers = act.stuntTiers;
    o.stuntKinds = act.stuntKinds;
    o.shotTiers = act.shotTiers;
    o.shotBlock = act.shotBlock;
    o.cutGrid = act.cutGrid;
    o.salvo = act.salvo;
    o.events = act.events;
    o.meteorShower = act.meteorShower;
    o.ascension = act.ascension;
    const k = snap ? 1 : 1 - Math.exp(-dt / SMOOTH_SECONDS);
    o.flightRate = glide(o.flightRate, "flightRate", source, mix, k);
    o.stuntGapScale = glide(o.stuntGapScale, "stuntGapScale", source, mix, k);
    o.eventRate = glide(o.eventRate, "eventRate", source, mix, k);
    o.meteorRate = glide(o.meteorRate, "meteorRate", source, mix, k);
    o.cruise = glide(o.cruise, "cruise", source, mix, k);
    o.groundTraffic = glide(o.groundTraffic, "groundTraffic", source, mix, k);
    o.fx.rgbSplit = glide(o.fx.rgbSplit, "rgbSplit", source, mix, k);
    o.fx.flash = glide(o.fx.flash, "flash", source, mix, k);
    o.fx.cityPulse = glide(o.fx.cityPulse, "cityPulse", source, mix, k);
  }
}

/**
 * Variations de l'après-set (niveau 5 au-delà de 100 %) : de temps en temps un tirage mêle les portes du niveau 4 à celles du
 * niveau 5, pour que le spectacle ne boucle jamais à l'identique.
 */
export class Variation {
  source: ShowLevel = 5;
  mix = 0;
  private left = 0;

  constructor(private readonly random: () => number) {}

  reset(): void {
    this.source = 5;
    this.mix = 0;
    this.left = 0;
  }

  /** Vrai quand les variations s'appliquent (`source` et `mix` sont alors à lire). */
  update(dt: number, on: boolean): boolean {
    if (!on) {
      this.reset();
      return false;
    }
    this.left -= dt;
    if (this.left <= 0) {
      this.source = this.random() < 0.4 ? 4 : 5;
      this.mix = 0.2 + 0.7 * this.random();
      this.left = 45 + 45 * this.random();
    }
    return true;
  }
}
