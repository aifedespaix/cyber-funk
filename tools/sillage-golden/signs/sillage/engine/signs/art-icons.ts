import type { AdKind } from "./types";
import type { DrawContext } from "./types";

/** Visuels de remplacement : chacun illustre le slogan qui s'affiche, à la place de l'icône par défaut du produit. */
export type IconId = "cage" | "tag" | "needle" | "lock" | "chip" | "hourglass" | "pad" | "pause" | "power" | "ballot" | "chart" | "liar" | "house" | "spot";

type Rule = readonly [RegExp, IconId];

/** Pour chaque produit, les mots du slogan qui appellent une autre image que celle par défaut (cerveau, t-shirt, sourire...). */
const RULES: Partial<Record<AdKind, readonly Rule[]>> = {
  food: [
    [/POULET|ÉLEV|ANIMAL|BÊTE|PRAIRIE|VÉCU|VU L'EXT|ÉTIQUETTE|BIEN-ÊTRE/, "cage"],
    [/PRIX|KILO|COÛTE|PORTION|REFAIT/, "tag"],
  ],
  fashion: [
    [/SOLDE|STOCK|MARGE|PRIX|%|RECYCLEZ|VALEUR/, "tag"],
    [/ATELIER|FABRIQU|FENÊTRE|COLLECTION|BRODÉ|TAILLE|FIBRE/, "needle"],
  ],
  mind: [
    [/ABONNEMENT|PRÉLÈVEMENT|CRÉDITS?|MENSUALIT|LOUEZ|999/, "lock"],
    [/IA PENSE|NEURAL|APPLICATION|ATTENTION|GISEMENT|ZÉRO PENSÉE/, "chip"],
    [/TEMPS|MINUTES|BURN-OUT|ÉCHÉANCE|BIEN-ÊTRE|PRODUCTIVITÉ/, "hourglass"],
  ],
  escape: [
    [/NIVEAU|STATISTIQUES|GAGNEZ|HEURES DE JEU|TEMPS DE JEU|DLC|VICTOIRE|VIE PARALLÈLE/, "pad"],
    [/PAUSE|MAINTENANCE|FIN DU MONDE/, "pause"],
    [/DÉCONNECT|CONNECTEZ|OUVREZ UN COMPTE|FUIR|FUYEZ/, "power"],
  ],
  politics: [
    [/POUR MOI|MARTINE/, "liar"],
    [/VOTEZ DANS|VOTEZ UTILE|PEUPLE|DÉMOCRATIE|SONDAGE|ÉGAUX/, "ballot"],
    [/CROISSANCE|CAPITAUX|SALAIRE|BANQUIER|AUSTÉRITÉ|MILLIARDAIRE|SPONSOR/, "chart"],
    [/PROMESSE|MENTI|ÉVOLUÉ|CONTINUITÉ/, "liar"],
  ],
  estate: [
    [/CONCERT|DAFT|STADE|FESTIVAL|AFTER|BILLET/, "spot"],
    [/LOYER|STUDIO|LOGEMENT|PROPRI|PIERRE|DOMICILE|CHEZ-SOI|VUE|IMMEUBLE/, "house"],
  ],
};

/** L'icône que le slogan appelle, ou `null` pour garder celle du produit. */
export function iconFor(kind: AdKind, text: string): IconId | null {
  for (const [pattern, icon] of RULES[kind] ?? []) if (pattern.test(text)) return icon;
  return null;
}

/** Dessine l'icône dans une boîte d'environ 110 × 110 px centrée sur (cx, cy), animée par la musique. */
export function drawIcon(d: DrawContext, icon: IconId, cx: number, cy: number): void {
  const { ctx, colors, audio, state } = d;
  const accent = colors[state.ad.accent];
  const { pulse, bass, mid } = audio.beat;
  const t = audio.time + state.seed;
  ctx.save();
  ctx.translate(cx, cy);
  ctx.strokeStyle = accent;
  ctx.fillStyle = accent;
  ctx.shadowColor = accent;
  ctx.shadowBlur = 10 + pulse * 12;
  ctx.lineWidth = 3;
  ctx.lineJoin = "round";
  ctx.lineCap = "round";
  switch (icon) {
    case "cage": {
      // Un œuf derrière des barreaux qui vibrent sur les basses ; il se fissure au kick.
      const shake = Math.sin(t * 40) * bass * 1.5;
      ctx.fillStyle = colors.ice;
      ctx.beginPath();
      ctx.ellipse(shake, 8, 17, 22, 0, 0, Math.PI * 2);
      ctx.fill();
      if (pulse > 0.4) {
        ctx.strokeStyle = colors.alarm;
        ctx.beginPath();
        ctx.moveTo(-10 + shake, 2);
        ctx.lineTo(-3 + shake, 8);
        ctx.lineTo(3 + shake, 0);
        ctx.lineTo(10 + shake, 8);
        ctx.stroke();
      }
      ctx.strokeStyle = accent;
      ctx.strokeRect(-46, -46, 92, 92);
      for (let x = -32; x <= 32; x += 16) {
        ctx.beginPath();
        ctx.moveTo(x, -46);
        ctx.lineTo(x, 46);
        ctx.stroke();
      }
      break;
    }
    case "tag": {
      // Étiquette de prix qui se balance au bout de son fil.
      ctx.translate(0, -44);
      ctx.rotate(Math.sin(t * 2.2) * 0.22);
      ctx.translate(0, 44);
      ctx.beginPath();
      ctx.moveTo(-32, -42);
      ctx.lineTo(32, -42);
      ctx.lineTo(32, 28);
      ctx.lineTo(0, 50);
      ctx.lineTo(-32, 28);
      ctx.closePath();
      ctx.fill();
      ctx.shadowBlur = 0;
      ctx.fillStyle = colors.ink;
      ctx.beginPath();
      ctx.arc(0, -30, 5, 0, Math.PI * 2);
      ctx.fill();
      ctx.font = `900 ${34 + pulse * 6}px Impact, 'Arial Black', sans-serif`;
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText("%", 0, 4);
      for (let i = 0; i < 7; i++) ctx.fillRect(-20 + i * 6, 20, i % 3 === 0 ? 4 : 2, 12);
      break;
    }
    case "needle": {
      // Bobine de fil et aiguille qui pique au rythme du tempo.
      ctx.fillRect(-24, -42, 48, 7);
      ctx.fillRect(-24, 35, 48, 7);
      ctx.globalAlpha = 0.65;
      ctx.fillRect(-18, -35, 36, 70);
      ctx.globalAlpha = 1;
      ctx.strokeStyle = colors.ink;
      for (let y = -30; y < 34; y += 7) {
        ctx.beginPath();
        ctx.moveTo(-18, y);
        ctx.lineTo(18, y + 3);
        ctx.stroke();
      }
      const dip = Math.sin(t * 6) * 4 * (0.4 + pulse);
      ctx.strokeStyle = colors.ice;
      ctx.beginPath();
      ctx.moveTo(40, -46 + dip);
      ctx.lineTo(54, 40 + dip);
      ctx.stroke();
      ctx.strokeStyle = accent;
      ctx.beginPath();
      ctx.moveTo(18, -20);
      for (let i = 1; i <= 8; i++) ctx.lineTo(18 + i * 4.5, -20 + i * 7 + Math.sin(t * 5 + i) * 4);
      ctx.stroke();
      break;
    }
    case "lock": {
      // Cadenas qui tremble avec les basses ; l'anse se soulève sur chaque kick, juste pour narguer.
      ctx.translate(Math.sin(t * 45) * bass * 2, 0);
      ctx.beginPath();
      ctx.arc(0, -10 - pulse * 6, 22, Math.PI, Math.PI * 2);
      ctx.stroke();
      ctx.beginPath();
      ctx.roundRect(-34, -8, 68, 52, 8);
      ctx.fill();
      ctx.shadowBlur = 0;
      ctx.fillStyle = colors.ink;
      ctx.beginPath();
      ctx.arc(0, 14, 7, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillRect(-3, 16, 6, 16);
      break;
    }
    case "chip": {
      // Puce dont les broches s'allument l'une après l'autre.
      ctx.strokeRect(-28, -28, 56, 56);
      ctx.strokeRect(-14, -14, 28, 28);
      for (let i = 0; i < 5; i++) {
        const lit = (Math.floor(t * 8) + i) % 5 === 0 || pulse > 0.6;
        ctx.globalAlpha = lit ? 1 : 0.35;
        const p = -20 + i * 10;
        for (const [x0, y0, x1, y1] of [[p, -28, p, -42], [p, 28, p, 42], [-28, p, -42, p], [28, p, 42, p]] as const) {
          ctx.beginPath();
          ctx.moveTo(x0, y0);
          ctx.lineTo(x1, y1);
          ctx.stroke();
        }
      }
      ctx.globalAlpha = 0.5 + mid * 0.5;
      ctx.fillRect(-8, -8, 16, 16);
      break;
    }
    case "hourglass": {
      // Le sable file, le sablier se retourne tout seul.
      const f = (t * 0.18) % 1;
      ctx.beginPath();
      ctx.moveTo(-26, -46);
      ctx.lineTo(26, -46);
      ctx.lineTo(3, 0);
      ctx.lineTo(26, 46);
      ctx.lineTo(-26, 46);
      ctx.lineTo(-3, 0);
      ctx.closePath();
      ctx.stroke();
      ctx.globalAlpha = 0.75;
      ctx.fillRect(-20 * (1 - f), -40 + f * 30, 40 * (1 - f), 30 * (1 - f) + 1);
      ctx.fillRect(-22 * f - 2, 44 - 34 * f, 44 * f + 4, 34 * f);
      ctx.globalAlpha = 1;
      ctx.fillRect(-1, -2, 2, 44 - 34 * f);
      break;
    }
    case "pad": {
      // Manette : croix directionnelle, deux boutons qui s'enfoncent sur les temps.
      ctx.beginPath();
      ctx.roundRect(-50, -22, 100, 50, 18);
      ctx.stroke();
      ctx.fillRect(-36, -4, 22, 6);
      ctx.fillRect(-28, -12, 6, 22);
      ctx.fillStyle = colors.rose;
      ctx.beginPath();
      ctx.arc(20, 4, 6 + pulse * 3, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = colors.mint;
      ctx.beginPath();
      ctx.arc(36, -6, 6 + bass * 3, 0, Math.PI * 2);
      ctx.fill();
      // Barre de vie qui se vide.
      ctx.fillStyle = colors.alarm;
      ctx.fillRect(-46, -44, 92 * (1 - ((t * 0.12) % 1)), 8);
      ctx.strokeRect(-46, -44, 92, 8);
      break;
    }
    case "pause": {
      ctx.beginPath();
      ctx.arc(0, 0, 44 + pulse * 3, 0, Math.PI * 2);
      ctx.stroke();
      ctx.fillRect(-16, -22, 10, 44);
      ctx.fillRect(6, -22, 10, 44);
      break;
    }
    case "power": {
      ctx.lineWidth = 6;
      ctx.beginPath();
      ctx.arc(0, 4, 36 + pulse * 3, -Math.PI / 2 + 0.6, -Math.PI / 2 - 0.6 + Math.PI * 2);
      ctx.stroke();
      ctx.beginPath();
      ctx.moveTo(0, -44);
      ctx.lineTo(0, -6);
      ctx.stroke();
      break;
    }
    case "ballot": {
      // Urne et bulletin qui s'y glisse en boucle.
      ctx.strokeRect(-40, -6, 80, 52);
      ctx.fillRect(-22, -8, 44, 5);
      const drop = (t * 0.6) % 1;
      ctx.save();
      ctx.beginPath();
      ctx.rect(-40, -60, 80, 54);
      ctx.clip();
      ctx.fillStyle = colors.ice;
      ctx.fillRect(-14, -56 + drop * 52, 28, 34);
      ctx.strokeStyle = colors.ink;
      ctx.beginPath();
      ctx.moveTo(-7, -36 + drop * 52);
      ctx.lineTo(-2, -30 + drop * 52);
      ctx.lineTo(8, -44 + drop * 52);
      ctx.stroke();
      ctx.restore();
      break;
    }
    case "chart": {
      // Histogramme qui grimpe avec le son, flèche qui dépasse du cadre.
      const levels = [0.3 + bass * 0.2, 0.45 + mid * 0.3, 0.6 + pulse * 0.2, 0.8 + bass * 0.2];
      levels.forEach((level, i) => ctx.fillRect(-42 + i * 22, 44 - level * 80, 16, level * 80));
      ctx.strokeStyle = colors.ice;
      ctx.beginPath();
      ctx.moveTo(-46, 20);
      ctx.lineTo(-10, -4);
      ctx.lineTo(10, 6);
      ctx.lineTo(46, -40);
      ctx.moveTo(46, -40);
      ctx.lineTo(32, -38);
      ctx.moveTo(46, -40);
      ctx.lineTo(44, -26);
      ctx.stroke();
      break;
    }
    case "liar": {
      // Le sourire du candidat, et un nez qui s'allonge avec l'intensité du morceau.
      const nose = 8 + audio.drive.intensity * 46 + pulse * 6;
      ctx.fillStyle = colors.amber;
      ctx.beginPath();
      ctx.arc(-8, 0, 46, 0, Math.PI * 2);
      ctx.fill();
      ctx.shadowBlur = 0;
      ctx.strokeStyle = colors.amber;
      ctx.lineWidth = 7;
      ctx.beginPath();
      ctx.moveTo(-8, 4);
      ctx.lineTo(-8 + 40 + nose * 0.6, 8 + nose * 0.15);
      ctx.stroke();
      ctx.fillStyle = colors.ink;
      ctx.beginPath();
      ctx.arc(-26, -14, 5, 0, Math.PI * 2);
      ctx.arc(8, -14, 5, 0, Math.PI * 2);
      ctx.fill();
      ctx.strokeStyle = colors.ink;
      ctx.lineWidth = 4;
      ctx.beginPath();
      ctx.arc(-8, 14, 20, 0.2, Math.PI - 0.2);
      ctx.stroke();
      break;
    }
    case "house": {
      // Maison dont une fenêtre clignote, et une bande « VENDU » qui s'allume aux temps forts.
      ctx.beginPath();
      ctx.moveTo(-42, 6);
      ctx.lineTo(0, -40);
      ctx.lineTo(42, 6);
      ctx.moveTo(-32, 0);
      ctx.lineTo(-32, 44);
      ctx.lineTo(32, 44);
      ctx.lineTo(32, 0);
      ctx.stroke();
      ctx.fillRect(-8, 20, 16, 24);
      ctx.globalAlpha = Math.sin(t * 3) > 0 ? 1 : 0.3;
      ctx.fillRect(14, 8, 12, 10);
      ctx.fillRect(-26, 8, 12, 10);
      ctx.globalAlpha = 1;
      if (pulse > 0.3 || audio.calm) {
        ctx.save();
        ctx.rotate(-0.28);
        ctx.fillStyle = colors.alarm;
        ctx.shadowColor = colors.alarm;
        ctx.fillRect(-44, -6, 88, 20);
        ctx.fillStyle = colors.ink;
        ctx.font = "bold 15px 'Courier New', monospace";
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillText("VENDU", 0, 4);
        ctx.restore();
      }
      break;
    }
    case "spot": {
      // Scène, deux projecteurs qui balaient et une foule qui rebondit sur les basses.
      ctx.globalAlpha = 0.28;
      for (const side of [-1, 1]) {
        const sweep = Math.sin(t * 1.4 + side) * 22;
        ctx.beginPath();
        ctx.moveTo(side * 40, -46);
        ctx.lineTo(sweep - 14, 22);
        ctx.lineTo(sweep + 14, 22);
        ctx.closePath();
        ctx.fill();
      }
      ctx.globalAlpha = 1;
      ctx.fillRect(-46, 22, 92, 5);
      ctx.fillStyle = colors.rose;
      for (let i = 0; i < 9; i++) {
        const hop = Math.abs(Math.sin(t * 5 + i * 1.3)) * (4 + bass * 12);
        ctx.beginPath();
        ctx.arc(-40 + i * 10, 42 - hop, 4, 0, Math.PI * 2);
        ctx.fill();
      }
      break;
    }
  }
  ctx.restore();
}
