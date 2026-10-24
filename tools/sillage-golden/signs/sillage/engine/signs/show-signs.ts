import { hash } from "../random";
import type { ShowState } from "../show/types";
import type { SceneColorName } from "../palette";
import type { FormatId } from "./formats";
import type { Ad } from "./types";

/**
 * Textes des panneaux en mode spectacle : slogans courts près de la route (lisibles au projecteur) et panneaux
 * qui annoncent le DJ. Hors spectacle, rien de tout cela ne s'applique.
 */

/** Formats posés au bord de la route ou au-dessus : on y lit court. Les toits, projections et bandeaux gardent les longues phrases. */
const NEAR: ReadonlySet<FormatId> = new Set<FormatId>(["holo", "gantry", "arm", "neon"]);
/** Un slogan « court » tient en 5 mots au plus. */
const SHORT_WORDS = 5;
/** Part des panneaux proches qui prennent un slogan court en mode spectacle. */
const SHORT_SHARE = 0.85;
/** Part des emplacements éligibles qui annoncent le DJ ; plafonnée à un panneau visible sur 4,5 (voir `liveAllowed`). */
const LIVE_SHARE = 0.22;
const LIVE_RATIO = 4.5;
const NAME_MAX = 24;

/** Formats qui peuvent porter le nom du DJ, avec le rendu choisi. */
const LIVE_FORMATS: Partial<Record<FormatId, "poster" | "neon" | "screen" | "ghost">> = {
  holo: "poster",
  gantry: "poster",
  arm: "poster",
  roof: "poster",
  neon: "neon",
  jumbo: "screen",
  ghost: "ghost",
};
const NEON_TAGS = ["LIVE", "NOW", "ON AIR"] as const;
const LIVE_ACCENTS: readonly SceneColorName[] = ["violet", "rose", "mint", "azure", "amber"];

export function isShort(text: string): boolean {
  return text.trim().split(/\s+/).length <= SHORT_WORDS;
}

/**
 * Le slogan de départ d'un panneau. En spectacle, près de la route : un slogan court, tiré de façon déterministe ;
 * l'immobilier prend un rang impair (l'affiche de concert, qui n'a pas de longue phrase). Sinon : le tirage habituel.
 */
export function sloganIndex(ad: Ad, k: number, format: FormatId, show: ShowState | undefined): number {
  const usual = Math.floor(hash(k, 4) * ad.slogans.length);
  if (!show?.active || !NEAR.has(format) || hash(k, 43) >= SHORT_SHARE) return usual;
  const pool: number[] = [];
  ad.slogans.forEach((text, i) => {
    if (ad.kind === "estate" ? i % 2 === 1 : isShort(text)) pool.push(i);
  });
  return pool.length ? pool[Math.floor(hash(k, 42) * pool.length)]! : usual;
}

/** Le nom du DJ se met à l'échelle d'un panneau : majuscules, borné. */
function tidy(text: string): string {
  const t = text.trim().replace(/\s+/g, " ").toUpperCase();
  return t.length > NAME_MAX ? `${t.slice(0, NAME_MAX - 1).trimEnd()}…` : t;
}

/** Le spectacle demande-t-il le nom du DJ sur les pubs (cinéma, option active, nom renseigné) ? */
export function djOnAds(show: ShowState | undefined): boolean {
  return !!show && show.active && show.djOnAds && show.dj.trim().length > 0;
}

/**
 * Faut-il annoncer le DJ sur l'emplacement `k` ? Tirage stable par emplacement, et jamais plus d'un panneau sur 4,5
 * parmi ceux déjà posés (`placed` panneaux, dont `live` annoncent le DJ).
 */
export function liveAllowed(k: number, format: FormatId, show: ShowState | undefined, placed: number, live: number): boolean {
  if (!djOnAds(show) || !LIVE_FORMATS[format] || hash(k, 41) >= LIVE_SHARE) return false;
  return (live + 1) * LIVE_RATIO <= placed + 1;
}

/**
 * Le panneau « DJ » de l'emplacement `k`, figé avec le nom du moment : un changement de DJ n'affecte que les panneaux
 * posés ensuite. Affiche de concert, néon court, écran géant (nom et tempo), ou le nom de l'événement en projection.
 */
export function liveAd(k: number, format: FormatId, show: ShowState): Ad {
  const name = tidy(show.dj);
  const event = tidy(show.eventName);
  const accent = LIVE_ACCENTS[Math.floor(hash(k, 44) * LIVE_ACCENTS.length)]!;
  const tag = NEON_TAGS[Math.floor(hash(k, 45) * NEON_TAGS.length)]!;
  const kind = LIVE_FORMATS[format];
  if (kind === "neon") return { kind: "neon", accent, slogans: [`${tag} ${name}`], live: { role: "text", name, event } };
  if (kind === "ghost") return { kind: "neon", accent, slogans: [event || `${tag} ${name}`], live: { role: "text", name, event } };
  if (kind === "screen") {
    return { kind: "watch", accent, slogans: [name, event || "EN DIRECT"], live: { role: "screen", name, event } };
  }
  // Rang impair : l'immobilier affiche alors l'affiche de concert.
  return { kind: "estate", accent: "violet", slogans: [name, `${name} LIVE`], live: { role: "poster", name, event } };
}
