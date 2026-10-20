import { type Color, Mesh, PlaneGeometry, ShaderMaterial, Vector3 } from "three";
import { type CityField, FIELD_GLSL } from "./landmarks/field";
import { GLYPH_GLSL, NOISE_GLSL } from "./landmarks/glsl";
import type { ScenePalette } from "./palette";
import { originFor } from "./world-origin";

const LENGTH = 1200;
const WIDTH = 18;

/**
 * Route infinie : un seul plan qui suit la voiture. Les motifs (bords, tirets,
 * joints) sont calculés en coordonnées monde, donc ils défilent sans recyclage.
 */
export class Road {
  readonly mesh: Mesh;
  private readonly material: ShaderMaterial;

  constructor(palette: ScenePalette, field: CityField) {
    const geometry = new PlaneGeometry(WIDTH, LENGTH, 1, 1);
    geometry.rotateX(-Math.PI / 2);

    this.material = new ShaderMaterial({
      uniforms: {
        /** Voiture relative à l'origine des motifs (voir `world-origin.ts`). */
        uCar: { value: new Vector3() },
        /** Origine des motifs (z) : `vWorld` reste la position monde, `vRel` en est la version précise. */
        uOrigin: { value: new Vector3() },
        /** Position du maillage moins l'origine, calculée en double côté CPU. */
        uRel: { value: new Vector3() },
        uGlow: { value: palette.violet.clone() },
        uMood: { value: palette.violet.clone() },
        uTime: { value: 0 },
        uPulse: { value: 0 },
        uEnergy: { value: 0 },
        uBrake: { value: 0 },
        /** Pré-intro : part de la route dessinée depuis la voiture (1 : tout, sans effet). */
        uDraw: { value: 1 },
        uInk: { value: palette.ink },
        uMint: { value: palette.mint },
        uAzure: { value: palette.azure },
        uViolet: { value: palette.violet },
        uRose: { value: palette.rose },
        uIce: { value: palette.ice },
        ...field.uniforms,
      },
      vertexShader: /* glsl */ `
        uniform vec3 uOrigin;
        uniform vec3 uRel;
        varying vec3 vWorld;
        varying vec3 vRel;
        varying vec3 vView;
        void main() {
          // Le maillage n'est que translaté : position relative exacte, et vue via modelViewMatrix (composée en double).
          vRel = position + uRel;
          vWorld = vRel + uOrigin;
          vec4 view = modelViewMatrix * vec4(position, 1.0);
          vView = view.xyz;
          gl_Position = projectionMatrix * view;
        }
      `,
      fragmentShader: /* glsl */ `
        uniform vec3 uCar;
        uniform vec3 uGlow;
        uniform vec3 uMood;
        uniform float uTime;
        uniform float uPulse;
        uniform float uEnergy;
        uniform float uBrake;
        uniform float uDraw;
        uniform vec3 uInk;
        uniform vec3 uMint;
        uniform vec3 uAzure;
        uniform vec3 uViolet;
        uniform vec3 uRose;
        uniform vec3 uIce;
        uniform sampler2D uDecalMap;
        uniform vec4 uDecalBox;
        uniform vec4 uDecalWord;
        uniform float uDecalAmt;
        uniform float uDecalGlitch;
        varying vec3 vWorld;
        varying vec3 vRel;
        varying vec3 vView;
        ${FIELD_GLSL}
        ${NOISE_GLSL}
        ${GLYPH_GLSL}

        float hash(vec2 p) {
          return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
        }

        vec3 prism(float t) {
          float a = 0.5 + 0.5 * sin(t);
          float b = 0.5 + 0.5 * sin(t * 1.7 + 2.1);
          return mix(mix(uMint, uAzure, a), mix(uViolet, uRose, b), 0.5);
        }

        void main() {
          vec2 p = vRel.xz; // x = largeur, y = profondeur (z monde)
          vec3 up = vec3(0.0, 1.0, 0.0);

          // Asphalte presque noir, grain fin
          float grain = hash(floor(p * vec2(9.0, 4.0)));
          vec3 col = uInk * (0.82 + 0.3 * grain);

          // Pluie : des flaques irrégulières. Mouillé, l'asphalte s'assombrit et reflète ; sec, il reste mat.
          float pud = smoothstep(0.34, 0.66, vnoise(p * vec2(0.35, 0.12) + 7.0) * 0.6 + vnoise(p * vec2(1.3, 0.5)) * 0.4);
          float wetK = uWet * (0.55 + 0.45 * pud);
          col *= 1.0 - 0.35 * wetK;

          // Bords : liserés lumineux qui changent de teinte le long de la route
          // L'ambiance du morceau (calme, groove, sommet) teinte les liserés.
          vec3 edgeColor = mix(prism(p.y * 0.035 + uTime * 0.35), uMood, 0.55);
          float edge = smoothstep(0.16, 0.0, abs(abs(p.x) - 8.4));
          col += edgeColor * (edge * (1.1 + uPulse * 1.2) + wetK * smoothstep(1.2, 0.0, abs(abs(p.x) - 8.4)) * (0.3 + 0.14 * uPulse) * (0.6 + 0.4 * vnoise(vec2(p.y * 0.5, p.x * 2.0))));

          // Tirets centraux
          float dashD = abs(mod(p.y + 1000.0, 9.0) - 4.5) - 1.9;
          float dash = smoothstep(0.1, 0.0, dashD) * smoothstep(0.1, 0.0, abs(p.x) - 0.18);
          col += uAzure * (dash * (0.9 + uPulse * 1.4) + wetK * smoothstep(1.6, 0.0, dashD) * smoothstep(0.9, 0.0, abs(p.x)) * 0.35);

          // Joints transversaux : une vague de lumière à chaque kick
          float seamD = abs(mod(p.y + 1000.0, 24.0) - 12.0);
          float seam = smoothstep(0.2, 0.0, seamD) * smoothstep(8.6, 7.4, abs(p.x));
          col += prism(p.y * 0.05 - uTime) * seam * (0.2 + uPulse * 0.9 + uEnergy * 0.2);

          // Lueur colorée sous la voiture
          vec2 d = (p - uCar.xz) / vec2(2.6, 4.2);
          float glow = exp(-dot(d, d));
          col += mix(uMint, uGlow, 0.6) * glow * (0.1 + uPulse * 0.2 + uEnergy * 0.1) + uGlow * exp(-dot(d, d) * 1.6) * 0.22;

          // Sol mouillé : les lumières des immeubles-vitrines y laissent un reflet et une lueur.
          col += fieldSheen(vWorld) * (0.38 + wetK * 2.4) + uIce * 0.06 * fieldLight(vWorld, up) * (1.0 + wetK * 3.0);

          // Chaussée mouillée : les optiques de la voiture se reflètent dans le sol (phares devant, feux rouges derrière).
          vec2 hq = p - uCar.xz;
          float ahead = 0.0;
          float behind = 0.0;
          for (int s = 0; s < 2; s++) {
            float sx = s == 0 ? -0.75 : 0.75;
            vec2 d = (hq - vec2(sx, -9.0)) / vec2(0.8, 10.0);
            ahead += exp(-dot(d, d));
            vec2 r = (hq - vec2(sx * 1.05, 2.8)) / vec2(0.5, 2.4);
            behind += exp(-dot(r, r));
          }
          col += mix(uIce, uGlow, 0.25) * ahead * wetK * 0.8;
          col += uRose * behind * wetK * (0.5 + uBrake * 2.0);

          // Gouttes qui tombent : des ronds qui s'élargissent et s'éteignent, plus nets dans les flaques et sous les enseignes.
          if (uRain > 0.01) {
            float rip = 0.0;
            for (int l = 0; l < 2; l++) {
              float sc = l == 0 ? 1.6 : 2.5;
              vec2 q = p / sc + float(l) * 13.7;
              vec2 id = floor(q);
              vec2 f = fract(q) - 0.5;
              float h = hash(id);
              float ph = fract(uTime * (0.6 + 0.5 * hash(id + 3.1)) + h);
              vec2 c = (vec2(hash(id + 1.7), hash(id + 9.2)) - 0.5) * 0.5;
              float d = length(f - c) * sc;
              rip += smoothstep(0.08, 0.0, abs(d - ph * 0.6)) * (1.0 - ph) * step(h, 0.4 + uRain * 0.5);
            }
            float near = 1.0 - smoothstep(30.0, 130.0, length(vView));
            vec3 lit = mix(uIce, uMood, 0.4) * 0.45 + fieldLight(vWorld, up) * 1.4;
            col += lit * rip * uRain * (0.35 + 0.65 * pud) * near;
          }


          // Zone de glitch : scanlines qui défilent, bandes aux canaux inversés.
          float zw = fieldZone(vWorld);
          float roadBand = step(0.9, fract(vRel.z * 0.07 + uTime * 0.9));
          float roadScan = step(0.5, fract(vRel.z * 0.9 - uTime * 4.0));
          col = mix(col, col.brg * 1.4, zw * roadBand);
          col += uZoneTint * zw * roadScan * 0.3;

          // Décalque : le mot du panneau ERROR, projeté sur la route devant la voiture.
          if (uDecalAmt > 0.001) {
            vec2 q = (vWorld.xz - uDecalBox.xy) / uDecalBox.zw;
            if (abs(q.x) < 1.0 && abs(q.y) < 1.0) {
              // Le haut du mot pointe vers l'avant (-z), pour qu'il se lise depuis le siège.
              vec3 m = glyphWord(uDecalMap, vec2(q.x * 0.5 + 0.5, 0.5 - q.y * 0.5), uDecalWord, uDecalGlitch, uTime);
              col += (uZoneTint * (m.g * 1.6 + m.r) + uIce * m.b * uDecalGlitch) * uDecalAmt;
            }
          }

          // Pré-intro : les lignes néon se dessinent depuis la voiture vers l'horizon, avec un front lumineux.
          if (uDraw < 0.999) {
            float reach = uDraw * 1100.0;
            float ahead = uCar.z - vRel.z;
            float drawn = 1.0 - smoothstep(reach - 40.0, reach, ahead);
            col = mix(uInk * 0.8, col, drawn) + uMint * exp(-pow((ahead - reach + 6.0) / 5.0, 2.0)) * 0.9 * step(ahead, reach + 1.0) * smoothstep(0.0, 0.05, uDraw);
          }

          // Brouillard vers l'encre : la route se perd à l'horizon
          float fog = smoothstep(90.0 - 30.0 * uRain, 340.0 - 90.0 * uRain, length(vView));
          col = mix(col, uInk, fog);

          gl_FragColor = vec4(col, 1.0);
        }
      `,
    });

    this.mesh = new Mesh(geometry, this.material);
    this.mesh.frustumCulled = false;
  }

  update(car: Vector3, time: number, pulse: number, energy: number, glow: Color, mood: Color): void {
    const u = this.material.uniforms;
    // Motifs calculés autour d'une origine proche de la voiture (double côté CPU) : voir `world-origin.ts`.
    const origin = originFor(car.z);
    const meshZ = car.z - LENGTH * 0.25;
    u.uOrigin!.value.set(0, 0, origin);
    u.uRel!.value.set(0, 0, meshZ - origin);
    u.uCar!.value.set(car.x, car.y, car.z - origin);
    u.uGlow!.value.copy(glow);
    u.uMood!.value.copy(mood);
    u.uTime!.value = time;
    u.uPulse!.value = pulse;
    u.uEnergy!.value = energy;
    // Le plan suit la voiture en profondeur : la route paraît infinie.
    this.mesh.position.set(0, 0, meshZ);
  }

  /** Freinage (0–1) : les feux rouges se reflètent plus fort dans la chaussée mouillée. */
  /** Pré-intro : fraction de la route dessinée, 0 → 1. */
  setDraw(amount: number): void {
    this.material.uniforms.uDraw!.value = amount;
  }

  setBrake(brake: number): void {
    this.material.uniforms.uBrake!.value = brake;
  }

  dispose(): void {
    this.mesh.geometry.dispose();
    this.material.dispose();
  }
}
