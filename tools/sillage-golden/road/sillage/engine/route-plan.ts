import type { PickupView } from "./autopilot";
import { FLOOR_COUNT, FLOORS } from "./flight/floors";

/**
 * Plan de route du pilote automatique : parmi les bonus à venir, quel chemin (abscisse + étage, dans le temps)
 * en ramasse le plus sans toucher de malus. Le volant (`Autopilot`) et la verticale (`AutoFlight`) lisent le
 * même plan, calculé une seule fois par image (cache) : ils ne se contredisent jamais.
 *
 * Modèle : les objets arrivent à `vitesse × SCROLL` m/s. Un bonus B est atteignable depuis A si la voiture a le
 * temps de rejoindre sa ligne (retard du volant, du premier ordre) et de changer d'étage (~0,9 s par étage). On
 * cherche le meilleur chemin dans ce graphe orienté (programmation dynamique) ; un malus inévitable sur un
 * trajet coûte très cher. Les constantes sont volontairement prudentes.
 */

/** Défilement de la route par rapport à la vitesse (voir SCROLL dans la scène). */
const SCROLL = 0.85;
const LIMIT = 4.6;
/** Distance (m) jusqu'où le volant et la verticale regardent : la même pour les deux, sans quoi leurs plans divergeraient. */
export const LOOKAHEAD_PLAN = 110;
/**
 * Vitesse de rattrapage latéral (1/s) de la voiture à pleine adhérence. Le vrai taux, publié à chaque image par la scène
 * (`pilotHint.rate`), baisse quand la voiture roule lentement (peu d'adhérence) ou très vite (sous-virage) : le plan
 * en prend une part, prudente pour la planification, plus proche du réel pour la commande.
 */
export const FULL_RATE = 5;
const PLAN_SHARE = 0.72;
const CTRL_SHARE = 0.88;
/** Plancher du taux : à l'arrêt presque complet, le plan ne doit pas diviser par zéro. */
const MIN_RATE = 0.8;
let planRate = FULL_RATE * PLAN_SHARE;
/** Taux de rattrapage utilisé par la commande du volant pour l'image en cours. */
export function ctrlRate(): number {
  return Math.max(MIN_RATE, pilotHint.rate) * CTRL_SHARE;
}
/** Écart latéral toléré à l'arrivée sur un bonus (le contact réel est à 1,5 m). */
const ARRIVE_TOL = 0.8;
/** Distance minimale à garder d'un malus (contact à 1,5 m) et décalage maximal de visée pour le fuir. */
export const SAFE = 1.9;
const SHIFT_MAX = 0.6;
/** Temps pour changer d'étage : un ordre toutes les 0,9 s, la caisse se pose ~0,55 s après le dernier. */
const FLOOR_STEP = 0.9;
const FLOOR_SETTLE = 0.55;
/** Décollage : déploiement des roues puis montée au premier étage. */
const TAKEOFF = 2.2;
/** Marge (s) avec laquelle on lance un changement d'étage avant le dernier moment. */
export const DEPART_SLACK = 0.45;
const MALUS_PENALTY = 5;
/** Autant que d'objets que la route peut porter à la fois (voir POOL dans pickups.ts) : plus n'a pas de sens. */
const MAX_NODES = 16;
const MAX_DODGES = 6;
const SAME_TIME = 0.06;
/** Demi-longueur de contact le long de la route (HIT_DEPTH de pickups.ts, 2,2 m, plus la marge d'une image). */
const CONTACT_DEPTH = 3;

/** État partagé entre le volant et la verticale : chacun publie ce que l'autre ne voit pas. */
export const pilotHint = {
  carX: 0,
  speed: 40,
  /** Taux de rattrapage latéral réel de la voiture (1/s), selon sa vitesse. */
  rate: FULL_RATE,
  /** Étage visé par la verticale, son délai avant un nouvel ordre (s), et la position Z de la dernière mise à jour. */
  floor: 1,
  delay: 0,
  floorZ: Number.NaN,
};

export interface Route {
  /** Nombre de bonus retenus ; 0 = rien à viser. */
  count: number;
  /** Pour chaque bonus du chemin : abscisse visée, instant d'arrivée (s) et étage (0 = sol). */
  aim: Float64Array;
  time: Float64Array;
  floor: Int8Array;
  /** Malus à fuir avant le premier bonus : abscisse et instant où l'on en est quitte. */
  dodgeCount: number;
  dodgeX: Float64Array;
  dodgeTime: Float64Array;
}

export interface RouteInput {
  carX: number;
  carZ: number;
  speed: number;
  /** Étage de départ : 0 = sol. */
  floor: number;
  /** Étage où la voiture se trouve vraiment (altitude) : après un ordre, elle met un moment à quitter l'ancien. Par défaut `floor`. */
  physical?: number;
  /** Secondes avant que le prochain ordre d'étage puisse partir. */
  delay: number;
  /** Faux si la verticale n'est pas pilotée par le plan : seuls les bonus de l'étage courant comptent. */
  canChange: boolean;
  /** Distance (m) jusqu'où l'on regarde. */
  lookahead: number;
}

/** Étage d'un objet d'après sa hauteur : 0 au sol, 1 à 6 en vol, -1 s'il n'est à aucun étage. */
export function floorOf(y: number): number {
  if (y < 2.5) return 0;
  for (let i = 0; i < FLOOR_COUNT; i++) if (Math.abs(FLOORS[i]! + 0.9 - y) < 2.2) return i + 1;
  return -1;
}

/** Étage le plus proche d'une altitude (m). */
export function floorAt(altitude: number): number {
  if (altitude < 1.5) return 0;
  let best = 1;
  for (let i = 1; i < FLOOR_COUNT; i++) if (Math.abs(FLOORS[i]! - altitude) < Math.abs(FLOORS[best - 1]! - altitude)) best = i + 1;
  return best;
}

/** Durée (s) pour aller de l'étage `from` à l'étage `to`, ordres compris. */
export function floorTime(from: number, to: number): number {
  const n = Math.abs(from - to);
  if (n === 0) return 0;
  if (from === 0) return TAKEOFF + (to - 1) * FLOOR_STEP;
  return (n - 1) * FLOOR_STEP + FLOOR_SETTLE;
}

const nodeX = new Float64Array(MAX_NODES);
const nodeAim = new Float64Array(MAX_NODES);
const nodeT = new Float64Array(MAX_NODES);
const nodeF = new Int8Array(MAX_NODES);
const nodeValue = new Float64Array(MAX_NODES);
const best = new Float64Array(MAX_NODES);
const prev = new Int8Array(MAX_NODES);
const path = new Int8Array(MAX_NODES);
const malusX = new Float64Array(MAX_NODES);
const malusT = new Float64Array(MAX_NODES);
const malusF = new Int8Array(MAX_NODES);
let nodes = 0;
let maluses = 0;
/** Demi-durée (s) pendant laquelle un objet est « en contact » : sa zone fait ±2,2 m le long de la route. */
let windowTime = 0;
let physicalFloor = 0;
/** Durée (s) pendant laquelle la voiture occupe encore l'ancien étage après un ordre de changement. */
const LEAVE_TIME = 0.4;

const route: Route = {
  count: 0,
  aim: new Float64Array(MAX_NODES),
  time: new Float64Array(MAX_NODES),
  floor: new Int8Array(MAX_NODES),
  dodgeCount: 0,
  dodgeX: new Float64Array(MAX_DODGES),
  dodgeTime: new Float64Array(MAX_DODGES),
};

let cached: readonly PickupView[] | null = null;
/** Les entrées du dernier plan : comparées une à une, sans fabriquer de clé à chaque appel. */
const cachedInput = { carZ: Number.NaN, floor: -1, physical: -1, speed: Number.NaN, delay: Number.NaN, rate: Number.NaN, lookahead: Number.NaN, canChange: false };

/** Le malus (étage `g`, instant `tm`) est-il sur le chemin de la voiture entre (étage `fa`, instant `ta`) et (étage `fb`, instant `tb`) ? */
function occupies(fa: number, ta: number, fb: number, tb: number, g: number, tm: number): boolean {
  if (fa === fb) return g === fa;
  const leave = Math.max(ta, tb - floorTime(fa, fb) - DEPART_SLACK);
  if (tm <= leave + 0.2) return g === fa;
  if (tm >= tb - 0.3) return g === fb;
  return g >= Math.min(fa, fb) && g <= Math.max(fa, fb);
}

/** Le malus est-il sur l'étage que la voiture est en train de quitter ? */
function lingers(g: number, tm: number): boolean {
  return g === physicalFloor && tm < LEAVE_TIME;
}

/** Existe-t-il une abscisse à l'instant `tm` qui garde ses distances au malus, en partant de (ax, at) et en rejoignant bx à `tb` ? */
function avoidable(ax: number, at: number, tm: number, bx: number, tb: number, xm: number): boolean {
  const forward = 1 - Math.exp(-planRate * (tm - at));
  const half = ARRIVE_TOL * Math.exp(planRate * (tb - tm));
  const lo = Math.max(ax - (ax + LIMIT) * forward, bx - half, -LIMIT);
  const hi = Math.min(ax + (LIMIT - ax) * forward, bx + half, LIMIT);
  if (hi < lo) return false;
  return lo <= xm - SAFE || hi >= xm + SAFE;
}

/** Valeur du trajet (ax, at, af) → bonus `j`, ou -Infinity s'il est hors de portée. `delay` ne compte que pour le départ. */
function edge(ax: number, at: number, af: number, j: number, delay: number, first: boolean): number {
  const tb = nodeT[j]!;
  const fb = nodeF[j]!;
  const bx = nodeAim[j]!;
  const dt = tb - at;
  const change = af === fb ? 0 : floorTime(af, fb) + (first ? delay : 0);
  if (dt < change) return Number.NEGATIVE_INFINITY;
  const gap = Math.abs(bx - ax);
  if (gap * Math.exp(-planRate * dt) > ARRIVE_TOL) return Number.NEGATIVE_INFINITY;
  let value = nodeValue[j]! - 0.02 * gap - 0.03 * Math.abs(af - fb);
  for (let m = 0; m < maluses; m++) {
    const tm = malusT[m]!;
    // Un malus ne se quitte qu'une fois sa zone de contact (±2,2 m) dépassée.
    const clear = tm + windowTime;
    if (clear <= 0 || (clear <= at + SAME_TIME && !first)) continue;
    if (tm > tb + SAME_TIME) continue;
    if (!occupies(af, at, fb, tb, malusF[m]!, Math.max(tm, 0)) && !(first && lingers(malusF[m]!, tm))) continue;
    if (!avoidable(ax, at, Math.min(clear, tb), bx, tb, malusX[m]!)) value -= MALUS_PENALTY;
  }
  return value;
}

function collect(pickups: readonly PickupView[], input: RouteInput): void {
  nodes = 0;
  maluses = 0;
  const v = Math.max(input.speed, 6) * SCROLL;
  windowTime = CONTACT_DEPTH / v + 0.02;
  for (const p of pickups) {
    const ahead = input.carZ - p.z;
    // Un malus qu'on vient de croiser touche encore tant que sa zone n'est pas dépassée ; un bonus, lui, ne se ramasse plus.
    if (ahead > input.lookahead || ahead < (p.kind === "malus" ? -CONTACT_DEPTH : 0)) continue;
    const floor = floorOf(p.y);
    if (floor < 0 || (!input.canChange && floor !== input.floor)) continue;
    const t = ahead / v;
    if (p.kind === "malus") {
      if (maluses === MAX_NODES) continue;
      malusX[maluses] = p.x;
      malusT[maluses] = t;
      malusF[maluses++] = floor;
      continue;
    }
    if (nodes === MAX_NODES) continue;
    // Insertion triée par instant d'arrivée.
    let i = nodes++;
    while (i > 0 && nodeT[i - 1]! > t) {
      nodeX[i] = nodeX[i - 1]!;
      nodeT[i] = nodeT[i - 1]!;
      nodeF[i] = nodeF[i - 1]!;
      i--;
    }
    nodeX[i] = p.x;
    nodeT[i] = t;
    nodeF[i] = floor;
  }
  for (let j = 0; j < nodes; j++) {
    let aim = nodeX[j]!;
    // Un malus de la même rangée : on vise le bord du bonus le plus éloigné de lui.
    for (let m = 0; m < maluses; m++) {
      if (malusF[m] !== nodeF[j] || Math.abs(malusT[m]! - nodeT[j]!) > SAME_TIME) continue;
      const gap = aim - malusX[m]!;
      if (Math.abs(gap) >= SAFE + 0.15) continue;
      const side = gap === 0 ? (aim > 0 ? -1 : 1) : Math.sign(gap);
      aim += side * Math.min(SHIFT_MAX, SAFE + 0.15 - Math.abs(gap));
    }
    nodeAim[j] = Math.max(-LIMIT, Math.min(LIMIT, aim));
    // Légère préférence pour les plus proches : à gain égal, on prend d'abord ce qui arrive.
    nodeValue[j] = 1 + 0.15 * (1 - Math.min(1, nodeT[j]! / 6));
  }
}

/** Oublie le dernier plan : les objets ont changé (nouvelle image). */
export function invalidateRoute(): void {
  cached = null;
}

/** Plan de route à cet instant. L'objet renvoyé est réutilisé : à lire dans l'image, sans le conserver. */
export function planRoute(pickups: readonly PickupView[], input: RouteInput): Route {
  physicalFloor = input.physical ?? input.floor;
  const rate = Math.max(MIN_RATE, pilotHint.rate);
  // Même image, mêmes données : le volant et la verticale partagent le résultat.
  const c = cachedInput;
  if (
    pickups === cached &&
    c.carZ === input.carZ &&
    c.floor === input.floor &&
    c.physical === physicalFloor &&
    c.speed === input.speed &&
    c.delay === input.delay &&
    c.rate === rate &&
    c.lookahead === input.lookahead &&
    c.canChange === input.canChange
  ) {
    return route;
  }
  cached = pickups;
  c.carZ = input.carZ;
  c.floor = input.floor;
  c.physical = physicalFloor;
  c.speed = input.speed;
  c.delay = input.delay;
  c.rate = rate;
  c.lookahead = input.lookahead;
  c.canChange = input.canChange;
  planRate = rate * PLAN_SHARE;
  route.count = 0;
  route.dodgeCount = 0;
  collect(pickups, input);
  if (nodes === 0) return route;

  let end = -1;
  let endValue = 0.05;
  for (let j = 0; j < nodes; j++) {
    let top = edge(input.carX, 0, input.floor, j, input.delay, true);
    prev[j] = -1;
    for (let i = 0; i < j; i++) {
      if (best[i] === Number.NEGATIVE_INFINITY || nodeT[i]! >= nodeT[j]! - SAME_TIME) continue;
      const total = best[i]! + edge(nodeAim[i]!, nodeT[i]!, nodeF[i]!, j, 0, false);
      if (total > top) {
        top = total;
        prev[j] = i;
      }
    }
    best[j] = top;
    if (top > endValue) {
      endValue = top;
      end = j;
    }
  }
  if (end < 0) return route;

  // Chemin de la fin au début, puis remis dans l'ordre.
  let n = 0;
  for (let j = end; j >= 0; j = prev[j]!) path[n++] = j;
  route.count = n;
  for (let k = 0; k < n; k++) {
    const j = path[n - 1 - k]!;
    route.aim[k] = nodeAim[j]!;
    route.time[k] = nodeT[j]!;
    route.floor[k] = nodeF[j]!;
  }
  // Malus à fuir avant le premier bonus, sur la rangée où la voiture se trouvera.
  const firstT = route.time[0]!;
  const firstF = route.floor[0]!;
  for (let m = 0; m < maluses && route.dodgeCount < MAX_DODGES; m++) {
    const tm = malusT[m]!;
    const clear = tm + windowTime;
    const here = occupies(input.floor, 0, firstF, firstT, malusF[m]!, Math.max(tm, 0)) || lingers(malusF[m]!, tm);
    if (clear <= 0 || tm >= firstT - SAME_TIME || !here) continue;
    route.dodgeX[route.dodgeCount] = malusX[m]!;
    route.dodgeTime[route.dodgeCount++] = clear;
  }
  return route;
}

/**
 * Étage que la verticale doit viser maintenant d'après le plan, ou null s'il n'y a rien à viser. Tant que le premier
 * bonus est sur l'étage courant, ou que le changement peut attendre, on reste où l'on est.
 */
export function routeFloor(plan: Route, current: number): number | null {
  if (plan.count === 0) return null;
  const target = plan.floor[0]!;
  if (target === current) return current;
  return plan.time[0]! - floorTime(current, target) > DEPART_SLACK ? current : target;
}
