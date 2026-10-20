// Rend la référence Sillage de chaque scène de `road_demo` et la colle à côté de la capture de cyber-funk.
//
//   node render.mjs <dossier>     # <dossier> = CF_SNAPSHOT_DIR de l'exemple (scenes.json + captures)
//
// Écrit dans <dossier> : `sillage-<scène>.png`, `cote-a-cote-<scène>.png` (Sillage à gauche, cyber-funk à droite) et
// affiche, par scène, l'écart moyen entre les deux rendus sur quelques zones (0-255, sRGB), plus l'écart sur l'image
// entière : moyenne, et part des pixels à plus de 8/255.
//
// `node` et non `bun` pour lancer Playwright (voir le skill `visual-verification` du dépôt de Sillage).

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const here = dirname(fileURLToPath(import.meta.url));
const dir = resolve(process.argv[2] ?? ".");
const scenes = JSON.parse(readFileSync(join(dir, "scenes.json"), "utf8"));

// Zones de comparaison (px, image 960×540) : route proche, dashes centraux, liseré gauche, liseré droit, horizon.
const ZONES = {
  proche: [300, 400, 660, 530],
  tirets: [440, 290, 520, 345],
  liseréG: [20, 330, 160, 375],
  liseréD: [800, 330, 940, 375],
  horizon: [380, 238, 580, 275],
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
page.on("pageerror", (error) => console.error("page :", error.message));
await page.goto(`file://${join(here, "index.html")}`);
await page.waitForFunction(() => typeof window.renderScene === "function");

const rows = [];
for (const scene of scenes) {
  const sillage = await page.evaluate((s) => window.renderScene(s), scene);
  writeFileSync(join(dir, `sillage-${scene.name}.png`), Buffer.from(sillage.split(",")[1], "base64"));
  const ours = `data:image/png;base64,${readFileSync(join(dir, `${scene.name}.png`)).toString("base64")}`;
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
      const measure = (x0, y0, x1, y1) => {
        let sum = 0;
        let far = 0;
        let n = 0;
        const meanA = [0, 0, 0];
        const meanB = [0, 0, 0];
        for (let y = y0; y < y1; y++) {
          for (let x = x0; x < x1; x++) {
            const i = 4 * (y * pa.width + x);
            let worst = 0;
            for (let c = 0; c < 3; c++) {
              const d = Math.abs(pa.data[i + c] - pb.data[i + c]);
              sum += d;
              worst = Math.max(worst, d);
              meanA[c] += pa.data[i + c];
              meanB[c] += pb.data[i + c];
            }
            if (worst > 8) far++;
            n++;
          }
        }
        return {
          ecart: +(sum / (3 * n)).toFixed(2),
          loin: +((100 * far) / n).toFixed(2),
          sillage: meanA.map((v) => Math.round(v / n)),
          cyberFunk: meanB.map((v) => Math.round(v / n)),
        };
      };
      const diff = {};
      for (const [zone, [x0, y0, x1, y1]] of Object.entries(zones)) diff[zone] = measure(x0, y0, x1, y1);
      diff["image"] = measure(0, 0, pa.width, pa.height);
      return { image: out.toDataURL("image/png"), diff };
    },
    { sillage, ours, name: scene.name, zones: ZONES },
  );
  writeFileSync(join(dir, `cote-a-cote-${scene.name}.png`), Buffer.from(result.image.split(",")[1], "base64"));
  rows.push({ scene: scene.name, ...result.diff });
}
await browser.close();

for (const row of rows) {
  const cells = [...Object.keys(ZONES), "image"]
    .map((z) => `${z} ${row[z].ecart} (S ${row[z].sillage.join("/")} · CF ${row[z].cyberFunk.join("/")}, ${row[z].loin} % >8)`)
    .join(" | ");
  console.log(`${row.scene.padEnd(20)} ${cells}`);
}
