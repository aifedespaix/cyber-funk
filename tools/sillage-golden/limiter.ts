// Données de référence du limiteur photosensible : rejoue un scénario déterministe sur le vrai `show-fx-limiter.ts` de
// Sillage et écrit les sorties en JSON. Utilisé par `crates/cf-show/tests/limiter_golden.rs`.
//
//   SILLAGE_DIR=/chemin/aifedespaix.com bun tools/sillage-golden/limiter.ts > crates/cf-show/tests/golden/limiter.json
//
// Le scénario (opérations + sorties) est entièrement dans le JSON : le test Rust le rejoue tel quel, sans réimplémenter
// le générateur pseudo-aléatoire.
import { join } from "node:path";

interface Rgb {
  r: number;
  g: number;
  b: number;
}
interface Limiter {
  setPolicy(flashes: boolean, breathing: boolean): void;
  trigger(channel: number, peak: number, attack: number, decay: number, now: number): boolean;
  step(dt: number): void;
  value(channel: number): number;
}
interface LimiterModule {
  FlashLimiter: new () => Limiter;
  defuseRed(color: Rgb): Rgb;
  FX_FLASH: number;
  FX_SWEEP: number;
}

const root = process.env.SILLAGE_DIR;
if (!root) throw new Error("SILLAGE_DIR doit pointer vers le dépôt aifedespaix.com");
const engine = join(root, "apps/main/src/components/sillage/engine/show-fx-limiter.ts");
const { FlashLimiter, defuseRed, FX_FLASH, FX_SWEEP } = (await import(engine)) as LimiterModule;

// LCG 32 bits (Numerical Recipes) : même suite à chaque exécution.
let seed = 0x2f6e2b1;
const rnd = (): number => {
  seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
  return seed / 4294967296;
};
const pick = <T>(values: readonly T[]): T => values[Math.floor(rnd() * values.length)]!;

const limiter = new FlashLimiter();
let now = 0;
const ops: unknown[] = [];
const outs: unknown[] = [];
const snap = (granted: boolean | null) => ({ granted, flash: limiter.value(FX_FLASH), sweep: limiter.value(FX_SWEEP) });

for (let i = 0; i < 1500; i++) {
  const r = rnd();
  if (r < 0.7) {
    // Pas de temps : parfois un trou d'image très long (borné par MAX_STEP dans le limiteur).
    const dt = pick([0.004, 0.008, 0.016, 0.016, 0.033, 0.05, 0.12, 0.6]) * (0.5 + rnd());
    now += dt;
    limiter.step(dt);
    ops.push({ op: "step", dt });
    outs.push(snap(null));
  } else if (r < 0.98) {
    const channel = rnd() < 0.7 ? FX_FLASH : FX_SWEEP;
    const peak = pick([0.05, 0.14, 0.2, 0.4, 0.7, 1, 3]) * (0.8 + 0.4 * rnd());
    const attack = pick([0, 0.02, 0.08, 0.09, 0.3, 0.5, 2]);
    const decay = pick([0, 0.05, 0.25, 0.5, 1.4, 3]);
    const granted = limiter.trigger(channel, peak, attack, decay, now);
    ops.push({ op: "trigger", channel, peak, attack, decay, now });
    outs.push(snap(granted));
  } else {
    const flashes = rnd() < 0.75;
    const breathing = rnd() < 0.3;
    limiter.setPolicy(flashes, breathing);
    ops.push({ op: "policy", flashes, breathing });
    outs.push(snap(null));
  }
}

const samples: [number, number, number][] = [
  [1, 0, 0],
  [0.8, 0.1, 0.1],
  [0.5, 0.3, 0.3],
  [0.3, 0.2, 0.2],
  [0.15, 0, 0],
  [0.14, 0, 0],
  [1, 1, 1],
  [0.2, 0.9, 1],
  [0.6, 0.45, 0.4],
  [0.9, 0.5, 0.5],
  [0.62, 0.4, 0.2],
  [1, 0, 1],
  [2.5, 0.2, 0],
];
for (let i = 0; i < 60; i++) samples.push([rnd(), rnd() * rnd(), rnd() * rnd()]);
const colors = samples.map(([r, g, b]) => {
  const c = defuseRed({ r, g, b });
  return { in: [r, g, b], out: [c.r, c.g, c.b] };
});

process.stdout.write(`${JSON.stringify({ ops, outs, colors })}\n`);
