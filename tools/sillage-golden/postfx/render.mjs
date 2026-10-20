// Rend la référence Sillage de chaque étape de `postfx_demo` et la colle à côté de la capture de cyber-funk.
//
//   node render.mjs <dossier>     # <dossier> = CF_SNAPSHOT_DIR de l'exemple (phases.json + captures)
//
// Écrit dans <dossier> : `sillage-<étape>.png`, `cote-a-cote-<étape>.png` (Sillage à gauche, cyber-funk à droite) et
// affiche, par étape, l'écart moyen entre les deux rendus sur quelques zones de la scène (0-255, sRGB).
//
// `node` et non `bun` pour lancer Playwright (voir le skill `visual-verification` du dépôt de Sillage).

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const here = dirname(fileURLToPath(import.meta.url));
const dir = resolve(process.argv[2] ?? ".");
const phases = JSON.parse(readFileSync(join(dir, "phases.json"), "utf8"));

// Zones de comparaison (px, image 960×540), hors surcouche 2D (Sillage n'en a pas : ses titres sont du DOM).
const ZONES = {
  ciel: [465, 210, 495, 232],
  charte: [300, 244, 660, 296],
  neon: [268, 33, 300, 42],
  route: [440, 380, 520, 460],
};

const browser = await chromium.launch({
  headless: true,
  // WebGL 2 sans carte graphique : SwiftShader derrière ANGLE.
  args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"],
});
const page = await browser.newPage({ viewport: { width: 960, height: 540 } });
page.on("console", (msg) => {
  if (msg.type() === "error") console.error("navigateur :", msg.text());
});
await page.goto(`file://${join(here, "index.html")}`);
await page.waitForFunction(() => typeof window.renderPhase === "function");

const rows = [];
for (const phase of phases) {
  const sillage = await page.evaluate((p) => window.renderPhase(p), phase);
  writeFileSync(join(dir, `sillage-${phase.name}.png`), Buffer.from(sillage.split(",")[1], "base64"));
  const ours = `data:image/png;base64,${readFileSync(join(dir, `${phase.name}.png`)).toString("base64")}`;
  const result = await page.evaluate(
    async ({ sillage, ours, name, zones }) => {
      const load = (src) =>
        new Promise((ok, ko) => {
          const img = new Image();
          img.onload = () => ok(img);
          img.onerror = ko;
          img.src = src;
        });
      const [a, b] = await Promise.all([load(sillage), load(ours)]);
      const label = 28;
      const out = document.createElement("canvas");
      out.width = a.width + b.width;
      out.height = a.height + label;
      const ctx = out.getContext("2d");
      ctx.fillStyle = "#000";
      ctx.fillRect(0, 0, out.width, out.height);
      ctx.drawImage(a, 0, label);
      ctx.drawImage(b, a.width, label);
      ctx.fillStyle = "#fff";
      ctx.font = "16px sans-serif";
      ctx.fillText(`Sillage (three.js) · ${name}`, 8, 19);
      ctx.fillText(`cyber-funk (Bevy) · ${name}`, a.width + 8, 19);
      const px = (img) => {
        const c = document.createElement("canvas");
        c.width = img.width;
        c.height = img.height;
        const g = c.getContext("2d");
        g.drawImage(img, 0, 0);
        return g.getImageData(0, 0, img.width, img.height);
      };
      const [pa, pb] = [px(a), px(b)];
      const diff = {};
      for (const [zone, [x0, y0, x1, y1]] of Object.entries(zones)) {
        let sum = 0;
        let n = 0;
        let meanA = [0, 0, 0];
        let meanB = [0, 0, 0];
        for (let y = y0; y < y1; y++) {
          for (let x = x0; x < x1; x++) {
            const i = 4 * (y * pa.width + x);
            for (let c = 0; c < 3; c++) {
              sum += Math.abs(pa.data[i + c] - pb.data[i + c]);
              meanA[c] += pa.data[i + c];
              meanB[c] += pb.data[i + c];
            }
            n++;
          }
        }
        meanA = meanA.map((v) => Math.round(v / n));
        meanB = meanB.map((v) => Math.round(v / n));
        diff[zone] = { ecart: +(sum / (3 * n)).toFixed(1), sillage: meanA, cyberFunk: meanB };
      }
      return { image: out.toDataURL("image/png"), diff };
    },
    { sillage, ours, name: phase.name, zones: ZONES },
  );
  writeFileSync(join(dir, `cote-a-cote-${phase.name}.png`), Buffer.from(result.image.split(",")[1], "base64"));
  rows.push({ phase: phase.name, ...result.diff });
}
await browser.close();

for (const row of rows) {
  const cells = Object.keys(ZONES)
    .map((z) => `${z} ${row[z].ecart} (S ${row[z].sillage.join("/")} · CF ${row[z].cyberFunk.join("/")})`)
    .join(" | ");
  console.log(`${row.phase.padEnd(18)} ${cells}`);
}
