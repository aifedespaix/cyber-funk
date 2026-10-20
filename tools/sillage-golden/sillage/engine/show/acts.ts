import type { ShotTier } from "../cinema/types";
import type { StuntKind, StuntTier } from "../stunts/types";
import type { EventName } from "../traffic/director";
import type { CutGrid, ShowGates, ShowLevel } from "./types";

/**
 * La table des six niveaux (actes) du spectacle. Chaque acte décrit ce qu'il autorise ; `gates.ts` en tire les
 * `ShowGates` de l'image (listes de l'acte, nombres qui glissent vers l'acte suivant).
 */

/** Début de chaque niveau, en fraction du set. Le niveau 0 (pré-intro) n'est pas sur l'horloge du set. Avec 180 min : 11, 32, 63, 99 min. */
export const LEVEL_STARTS: readonly number[] = [0, 0, 0.06, 0.18, 0.35, 0.55];

/** Les nombres de l'acte : ils sont atteints au début de l'acte et glissent vers ceux de l'acte suivant jusqu'à sa fin. */
export interface ActNumbers {
  flightRate: number;
  stuntGapScale: number;
  eventRate: number;
  meteorRate: number;
  cruise: number;
  groundTraffic: number;
  rgbSplit: number;
  flash: number;
  cityPulse: number;
}

export interface ActSpec {
  readonly level: ShowLevel;
  readonly maxFloor: number;
  readonly flightOnDropOnly: boolean;
  readonly tricks: boolean;
  readonly stuntTiers: readonly StuntTier[];
  readonly stuntKinds: readonly StuntKind[];
  readonly shotTiers: readonly ShotTier[];
  readonly shotBlock: readonly string[];
  readonly cutGrid: CutGrid;
  readonly salvo: boolean;
  readonly events: readonly EventName[];
  readonly meteorShower: boolean;
  readonly ascension: boolean;
  readonly numbers: Readonly<ActNumbers>;
}

/** Tous les événements du trafic. */
export const ALL_EVENTS: readonly EventName[] = [
  "chase", "convoy", "crossroad", "tower", "airship", "swarm", "escort", "landmark", "bikes", "adship",
  "storm", "aurora", "fireworks", "race", "lanterns", "dancer", "blackout", "fish", "toll", "refuel",
  "tunnel", "city-pulse", "holo-title",
];

const ALL_STUNTS: readonly StuntKind[] = [
  "off-road", "slalom", "near-miss", "head-on", "threading", "wall-ride", "sign-hop", "loop", "inverted", "rooftop", "big-jump",
];
const GROUND_STUNTS: readonly StuntKind[] = ["off-road", "slalom", "near-miss", "head-on"];
const FLIGHT_STUNTS: readonly StuntKind[] = [...GROUND_STUNTS, "threading", "wall-ride", "sign-hop"];

const CALM_EVENTS: readonly EventName[] = ["lanterns", "fish", "refuel", "airship"];
const CITY_EVENTS: readonly EventName[] = [...CALM_EVENTS, "escort", "chase", "convoy", "bikes", "toll", "adship", "holo-title"];
const FLIGHT_EVENTS: readonly EventName[] = [...CITY_EVENTS, "landmark", "aurora", "swarm", "crossroad", "tower", "race", "dancer", "tunnel"];

/** Plans qui montent haut, tournent vite, jouent le vol ou le ciel : ils arrivent avec les niveaux. */
const AERIAL = ["top-down", "god-view", "sky-tilt", "crane", "vertigo", "spiral"];
const FAST = ["strobe", "flyby", "dutch", "wheel"];
const FLIGHT_SHOTS = ["takeoff", "touchdown", "dodge-pass", "barrel"];
const SPACE = ["meteor-ride", "meteor-impact-watch", "moon-pass", "ringworld"];
const GROUND_ONLY = [...AERIAL, ...FAST, ...FLIGHT_SHOTS, ...SPACE, "stunt-cam"];

/** Les actes. `numbers` : voir `ActNumbers`. Les grilles de coupe sont en temps musicaux (lot caméra). */
export const ACTS: readonly ActSpec[] = [
  {
    level: 0,
    maxFloor: 0, flightOnDropOnly: false, tricks: false,
    stuntTiers: [], stuntKinds: [],
    shotTiers: ["common"], shotBlock: GROUND_ONLY, cutGrid: { calm: 16, groove: 16, peak: 16 },
    salvo: false, events: [], meteorShower: false, ascension: false,
    numbers: { flightRate: 0, stuntGapScale: 1.6, eventRate: 0, meteorRate: 0, cruise: 1, groundTraffic: 0, rgbSplit: 0, flash: 0, cityPulse: 0.1 },
  },
  {
    level: 1,
    maxFloor: 0, flightOnDropOnly: false, tricks: false,
    stuntTiers: [], stuntKinds: [],
    shotTiers: ["common", "uncommon"], shotBlock: GROUND_ONLY, cutGrid: { calm: 16, groove: 16, peak: 8 },
    salvo: false, events: CALM_EVENTS, meteorShower: false, ascension: false,
    numbers: { flightRate: 0, stuntGapScale: 1.6, eventRate: 0.5, meteorRate: 0, cruise: 1, groundTraffic: 0.5, rgbSplit: 0.05, flash: 0, cityPulse: 0.2 },
  },
  {
    level: 2,
    maxFloor: 2, flightOnDropOnly: true, tricks: false,
    stuntTiers: ["common", "uncommon"], stuntKinds: GROUND_STUNTS,
    shotTiers: ["common", "uncommon"], shotBlock: [...AERIAL, ...SPACE, "strobe", "dutch"], cutGrid: { calm: 16, groove: 8, peak: 4 },
    salvo: false, events: CITY_EVENTS, meteorShower: false, ascension: false,
    numbers: { flightRate: 0.5, stuntGapScale: 1.3, eventRate: 0.8, meteorRate: 0, cruise: 1.03, groundTraffic: 0.8, rgbSplit: 0.15, flash: 0.15, cityPulse: 0.4 },
  },
  {
    level: 3,
    maxFloor: 4, flightOnDropOnly: false, tricks: true,
    stuntTiers: ["common", "uncommon"], stuntKinds: FLIGHT_STUNTS,
    shotTiers: ["common", "uncommon", "rare"], shotBlock: ["meteor-ride", "moon-pass", "ringworld"], cutGrid: { calm: 8, groove: 4, peak: 4 },
    salvo: false, events: FLIGHT_EVENTS, meteorShower: false, ascension: false,
    numbers: { flightRate: 1, stuntGapScale: 1, eventRate: 1, meteorRate: 1.5, cruise: 1.07, groundTraffic: 1, rgbSplit: 0.3, flash: 0.3, cityPulse: 0.6 },
  },
  {
    level: 4,
    maxFloor: 6, flightOnDropOnly: false, tricks: true,
    stuntTiers: ["common", "uncommon", "rare"], stuntKinds: ALL_STUNTS,
    shotTiers: ["common", "uncommon", "rare"], shotBlock: ["ringworld"], cutGrid: { calm: 8, groove: 4, peak: 2 },
    salvo: true, events: ALL_EVENTS, meteorShower: true, ascension: false,
    numbers: { flightRate: 1.3, stuntGapScale: 0.8, eventRate: 1.3, meteorRate: 5, cruise: 1.1, groundTraffic: 1, rgbSplit: 0.55, flash: 0.6, cityPulse: 0.8 },
  },
  {
    level: 5,
    maxFloor: 6, flightOnDropOnly: false, tricks: true,
    stuntTiers: ["common", "uncommon", "rare"], stuntKinds: ALL_STUNTS,
    shotTiers: ["common", "uncommon", "rare", "epic"], shotBlock: [], cutGrid: { calm: 4, groove: 2, peak: 2 },
    salvo: true, events: ALL_EVENTS, meteorShower: true, ascension: true,
    numbers: { flightRate: 1.6, stuntGapScale: 0.65, eventRate: 1.5, meteorRate: 9, cruise: 1.15, groundTraffic: 1, rgbSplit: 0.8, flash: 0.85, cityPulse: 1 },
  },
];

/**
 * Portes du spectacle inactif : tout est permis, rien de nouveau n'est ajouté. Les effets propres au spectacle sont
 * à zéro, le trafic et les astéroïdes gardent leur comportement d'avant (≈ 14 lancers par minute).
 */
export const OPEN_GATES: ShowGates = {
  maxFloor: 6,
  flightRate: 1,
  flightOnDropOnly: false,
  tricks: true,
  stuntTiers: ["common", "uncommon", "rare"],
  stuntKinds: ALL_STUNTS,
  stuntGapScale: 1,
  shotTiers: ["common", "uncommon", "rare", "epic"],
  shotBlock: [],
  cutGrid: { calm: 8, groove: 4, peak: 2 },
  salvo: true,
  events: ALL_EVENTS,
  eventRate: 1,
  meteorRate: 14,
  meteorShower: true,
  cruise: 1,
  groundTraffic: 1,
  fx: { rgbSplit: 0, flash: 0, cityPulse: 0 },
  ascension: true,
};

/** Seconde à laquelle un niveau commence, pour un set de `duration` secondes. */
export function levelStart(level: ShowLevel, duration: number): number {
  return LEVEL_STARTS[level]! * duration;
}

/** Le niveau que l'horloge réclame à `elapsed` secondes (1 au minimum : la pré-intro se quitte autrement). */
export function dueLevel(elapsed: number, duration: number): ShowLevel {
  let level: ShowLevel = 1;
  for (let l = 2; l <= 5; l++) if (elapsed >= levelStart(l as ShowLevel, duration)) level = l as ShowLevel;
  return level;
}
