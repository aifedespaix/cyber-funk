import { hash } from "../random";

/** Toutes les enseignes partagent le même canvas, en 2:1 ; seule la taille dans le monde change. */
export const CANVAS_W = 512;
export const CANVAS_H = 256;
export const TICKER_H = 40;
export const IMAGE_H = CANVAS_H - TICKER_H;

export type FormatId = "holo" | "gantry" | "arm" | "roof" | "jumbo" | "ghost" | "neon" | "cam" | "hangCam" | "hangAd";

/** Le rendu du visuel : hologramme cadré, caisson opaque, projection sans support, tubes de néon, écran vidéo. */
export type Look = "holo" | "metal" | "ghost" | "neon" | "camera";

/** Ce qui porte le panneau : rien, portique (deux pieds), potence (un pied), pylône de toit, mât, ou des câbles sous un vaisseau. */
export type Mount = "none" | "gantry" | "arm" | "tower" | "pole" | "hang";

export interface Layout {
  x: number;
  /** Hauteur du centre du panneau. */
  y: number;
  yaw: number;
}

export interface Format {
  look: Look;
  mount: Mount;
  /** Largeur dans le monde, en mètres (la hauteur vaut la moitié). */
  width: number;
  /** Distance devant la voiture à laquelle le panneau apparaît, et derrière laquelle il disparaît. */
  range: number;
  behind: number;
  /** Au-delà de cette distance, le canvas est redessiné une image sur trois. */
  paint: number;
  /** Place du panneau sur la route : `r` donne des tirages stables propres à l'emplacement. */
  layout(side: -1 | 1, r: (salt: number) => number): Layout;
}

const HALF_PI = Math.PI / 2;

export const FORMATS: Record<FormatId, Format> = {
  // L'hologramme flottant au bord de la route.
  holo: {
    look: "holo",
    mount: "none",
    width: 16,
    range: 340,
    behind: 40,
    paint: 300,
    layout: (side, r) => ({ x: side * 12, y: 15 + (r(5) - 0.5) * 4, yaw: -side * (HALF_PI - 0.45) }),
  },
  // Le portique d'autoroute : on passe dessous, le caisson est face à la voiture.
  gantry: {
    look: "metal",
    mount: "gantry",
    width: 20,
    range: 360,
    behind: 25,
    paint: 300,
    layout: (_side, r) => ({ x: 0, y: 12.5 + r(5) * 1.5, yaw: 0 }),
  },
  // La potence : un seul pied sur le bas-côté, le caisson déborde au-dessus des voies.
  arm: {
    look: "metal",
    mount: "arm",
    width: 15,
    range: 340,
    behind: 25,
    paint: 280,
    layout: (side, r) => ({ x: side * 3, y: 11 + r(5) * 2, yaw: side * 0.12 }),
  },
  // Le panneau de toit : très haut, très loin, donc très grand.
  roof: {
    look: "metal",
    mount: "tower",
    width: 44,
    range: 560,
    behind: 60,
    paint: 400,
    layout: (side, r) => ({ x: side * (34 + r(5) * 10), y: 62 + r(6) * 28, yaw: -side * 0.42 }),
  },
  // L'écran géant de toit, relié à une caméra.
  jumbo: {
    look: "camera",
    mount: "tower",
    width: 52,
    range: 560,
    behind: 20,
    paint: 400,
    layout: (side, r) => ({ x: side * (30 + r(5) * 8), y: 58 + r(6) * 20, yaw: -side * 0.38 }),
  },
  // La projection : des lettres dans le ciel, sans support ni cadre.
  ghost: {
    look: "ghost",
    mount: "none",
    width: 42,
    range: 560,
    behind: 50,
    paint: 400,
    layout: (side, r) => ({ x: side * (26 + r(5) * 16), y: 45 + r(6) * 35, yaw: -side * 0.5 }),
  },
  // L'enseigne en tubes de néon, en drapeau sur un mât.
  neon: {
    look: "neon",
    mount: "pole",
    width: 11,
    range: 300,
    behind: 30,
    paint: 260,
    layout: (side, r) => ({ x: side * 10.5, y: 12 + r(5) * 8, yaw: -side * 0.95 }),
  },
  // Les panneaux transportés par le vaisseau publicitaire : le vaisseau les place, ils sont toujours peints.
  hangCam: {
    look: "camera",
    mount: "hang",
    width: 17,
    range: 340,
    behind: 340,
    paint: 340,
    layout: () => ({ x: 0, y: 20, yaw: 0 }),
  },
  hangAd: {
    look: "metal",
    mount: "hang",
    width: 15,
    range: 340,
    behind: 340,
    paint: 340,
    layout: () => ({ x: 0, y: 20, yaw: 0 }),
  },
  // Le portique-caméra : il filme la voiture et la montre sur son écran.
  cam: {
    look: "camera",
    mount: "gantry",
    width: 20,
    range: 360,
    behind: 18,
    paint: 300,
    layout: () => ({ x: 0, y: 12, yaw: 0 }),
  },
};

export const MAX_RANGE = Math.max(...Object.values(FORMATS).map((f) => f.range));
export const MAX_BEHIND = Math.max(...Object.values(FORMATS).map((f) => f.behind));

/**
 * Le jeu de cartes de la route : huit emplacements consécutifs forment une
 * donne, battue au hasard (de façon déterministe) à chaque tour. Chaque donne
 * garantit de la variété : deux caissons de toit, un portique, une potence,
 * un néon, une projection, un hologramme... et, une donne sur deux, une
 * caméra à la place d'un hologramme ; une sur trois, un écran géant de toit
 * à la place d'un panneau de toit.
 */
const DECK: readonly FormatId[] = ["holo", "holo", "gantry", "arm", "roof", "roof", "ghost", "neon"];
const DECK_SIZE = DECK.length;
const CAM_CHANCE = 0.4;
const JUMBO_CHANCE = 0.5;

function deal(round: number): FormatId[] {
  const cards = [...DECK];
  const holo = cards.indexOf("holo");
  if (holo >= 0 && hash(round, 12) > CAM_CHANCE) cards[holo] = "cam";
  const roof = cards.lastIndexOf("roof");
  if (roof >= 0 && hash(round, 13) > JUMBO_CHANCE) cards[roof] = "jumbo";
  const keyed = cards.map((card, i) => ({ card, key: hash(round * DECK_SIZE + i, 11) }));
  return keyed.sort((a, b) => a.key - b.key).map((c) => c.card);
}

const dealt = new Map<number, FormatId[]>();
function card(k: number): FormatId {
  const round = Math.floor(k / DECK_SIZE);
  let cards = dealt.get(round);
  if (!cards) {
    cards = deal(round);
    if (dealt.size > 64) dealt.clear();
    dealt.set(round, cards);
  }
  return cards[((k % DECK_SIZE) + DECK_SIZE) % DECK_SIZE]!;
}

const feeds = (id: FormatId) => FORMATS[id].look === "camera";

/** Le format de l'emplacement `k` : la carte tirée, sauf si elle répète sa voisine (deux caméras de suite sont aussi écartées). */
export function formatAt(k: number): FormatId {
  const now = card(k);
  const before = card(k - 1);
  if (feeds(now) && feeds(before)) return "ghost";
  if (now === before) return now === "holo" ? "neon" : "holo";
  return now;
}
