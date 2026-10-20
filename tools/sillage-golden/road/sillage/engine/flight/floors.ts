/**
 * Les étages de vol : des altitudes fixes au-dessus de la route (0 = au sol, roues sur la chaussée).
 *
 * Elles sont choisies pour laisser passer la voiture entre les obstacles les plus courants :
 * - le haut des portiques néon est à 11,5 m (étages 2 et 3 de part et d'autre) ;
 * - le tablier des immeubles-arches occupe 24–35 m (seul l'étage 5 le heurte : la voiture l'esquive) ;
 * - l'arche de voxels passe à 32 m et plus, la route croisée surélevée à 19 m.
 */
export const FLOORS = [3.5, 8, 15, 21, 29.5, 38.5] as const;
export const FLOOR_COUNT = FLOORS.length;

/** Altitude de l'étage `floor` (1 à 6), 0 pour le sol. */
export function floorAltitude(floor: number): number {
  if (floor <= 0) return 0;
  return FLOORS[Math.min(FLOOR_COUNT, Math.floor(floor)) - 1]!;
}

/** Étendue verticale de la voiture autour de son altitude : sous la coque (patins compris), au-dessus du toit. */
export const CAR_BELOW = 0.7;
export const CAR_ABOVE = 1.9;
/** Marge de sécurité visuelle entre la voiture et ce qu'elle esquive. */
export const CLEARANCE = 1;

/** Une tranche d'altitude occupée par quelque chose (véhicule, nuée), en mètres. */
export interface Band {
  lo: number;
  hi: number;
}

/** Vrai si la voiture, à l'altitude `altitude`, partagerait de la place avec la tranche `band` (marge `extra` en plus). */
export function overlaps(altitude: number, band: Band, extra = CLEARANCE): boolean {
  return altitude + CAR_ABOVE + extra > band.lo && altitude - CAR_BELOW - extra < band.hi;
}
