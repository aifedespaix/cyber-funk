import { AdditiveBlending, BackSide, Color, DoubleSide, ShaderMaterial, Vector3 } from "three";
import type { ScenePalette } from "./palette";

/** Axe du plan de la Voie lactée : partagé entre le dôme et la répartition des étoiles. */
export const GALAXY_AXIS = new Vector3(0.32, 0.82, -0.47).normalize();

const NOISE = /* glsl */ `
  float h3(vec3 p) {
    p = fract(p * 0.3183099 + 0.1);
    p *= 17.0;
    return fract(p.x * p.y * p.z * (p.x + p.y + p.z));
  }
  float vnoise(vec3 x) {
    vec3 i = floor(x);
    vec3 f = fract(x);
    f = f * f * (3.0 - 2.0 * f);
    return mix(
      mix(mix(h3(i), h3(i + vec3(1, 0, 0)), f.x), mix(h3(i + vec3(0, 1, 0)), h3(i + vec3(1, 1, 0)), f.x), f.y),
      mix(mix(h3(i + vec3(0, 0, 1)), h3(i + vec3(1, 0, 1)), f.x), mix(h3(i + vec3(0, 1, 1)), h3(i + vec3(1, 1, 1)), f.x), f.y),
      f.z
    );
  }
  float fbm3(vec3 p) {
    float v = 0.0;
    float a = 0.5;
    for (int i = 0; i < 5; i++) {
      v += a * vnoise(p);
      p = p * 2.03 + 11.7;
      a *= 0.5;
    }
    return v;
  }
`;

/** Fond de l'espace : noir profond, nébuleuses diffuses et bande de Voie lactée. */
export function createDome(palette: ScenePalette): ShaderMaterial {
  return new ShaderMaterial({
    side: BackSide,
    depthWrite: false,
    uniforms: {
      uTime: { value: 0 },
      uPulse: { value: 0 },
      uInk: { value: palette.ink },
      uViolet: { value: palette.violet },
      uAzure: { value: palette.azure },
      uRose: { value: palette.rose },
      uAxis: { value: GALAXY_AXIS },
    },
    vertexShader: /* glsl */ `
      varying vec3 vDir;
      void main() {
        vDir = normalize(position);
        gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
      }
    `,
    fragmentShader: /* glsl */ `
      uniform float uTime;
      uniform float uPulse;
      uniform vec3 uInk;
      uniform vec3 uViolet;
      uniform vec3 uAzure;
      uniform vec3 uRose;
      uniform vec3 uAxis;
      varying vec3 vDir;
      ${NOISE}
      void main() {
        vec3 d = normalize(vDir);
        float h = d.y;
        // Noir très légèrement bleuté ; le bas du ciel reste sombre, l'horizon un peu plus chaud.
        vec3 col = uInk * 0.55;
        col += uViolet * 0.05 * smoothstep(0.1, -0.5, h);
        col += mix(uViolet, uRose, 0.4) * 0.1 * smoothstep(0.18, 0.0, abs(h));

        // Nébuleuses : deux couches de bruit à grande échelle, teintées différemment.
        float cloudA = fbm3(d * 2.2 + vec3(0.0, uTime * 0.004, 0.0));
        float cloudB = fbm3(d * 4.5 + 7.3);
        col += uViolet * smoothstep(0.5, 0.85, cloudA) * 0.16;
        col += uAzure * smoothstep(0.55, 0.9, cloudB) * 0.1;
        col += uRose * smoothstep(0.62, 0.95, fbm3(d * 3.0 - 3.1)) * 0.07;

        // Voie lactée : bande dense, striée de poussière sombre.
        float band = exp(-pow(dot(d, uAxis) * 5.5, 2.0));
        float dust = smoothstep(0.35, 0.75, fbm3(d * 9.0));
        col += mix(uAzure, uViolet, 0.5 + 0.5 * cloudA) * band * (0.2 - 0.12 * dust) * (1.0 + uPulse * 0.25);
        gl_FragColor = vec4(col, 1.0);
      }
    `,
  });
}

/** Champ d'étoiles : taille en pixels, scintillement, éclat additionnel sur les kicks. */
export function createStars(pixelRatio: number): ShaderMaterial {
  return new ShaderMaterial({
    transparent: true,
    depthWrite: false,
    blending: AdditiveBlending,
    uniforms: { uTime: { value: 0 }, uPulse: { value: 0 }, uPx: { value: pixelRatio } },
    vertexShader: /* glsl */ `
      attribute float aSize;
      attribute float aPhase;
      attribute vec3 aColor;
      uniform float uTime;
      uniform float uPulse;
      uniform float uPx;
      varying float vAlpha;
      varying float vBig;
      varying vec3 vColor;
      void main() {
        gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
        float twinkle = 0.65 + 0.35 * sin(uTime * (0.6 + aPhase * 2.0) + aPhase * 60.0);
        vAlpha = twinkle * (0.55 + uPulse * 0.35 * step(0.6, aPhase));
        vBig = smoothstep(3.0, 6.0, aSize);
        gl_PointSize = aSize * uPx * (1.0 + uPulse * 0.25 * vBig);
        vColor = aColor;
      }
    `,
    fragmentShader: /* glsl */ `
      varying float vAlpha;
      varying float vBig;
      varying vec3 vColor;
      void main() {
        vec2 p = gl_PointCoord - 0.5;
        float r = length(p);
        float core = smoothstep(0.5, 0.0, r);
        core *= core;
        // Les plus grosses étoiles portent une croix de diffraction.
        float cross = (smoothstep(0.03, 0.0, abs(p.x)) + smoothstep(0.03, 0.0, abs(p.y))) * smoothstep(0.5, 0.0, r) * vBig;
        float a = (core + cross * 0.6) * vAlpha;
        gl_FragColor = vec4(vColor * a * 2.0, a);
      }
    `,
  });
}

/** Rideau d'aurore boréale : un plan vertical très lointain, en bruit animé. */
export function createAurora(colors: [Color, Color, Color], seed: number, motion: number): ShaderMaterial {
  const [a, b, c] = colors;
  return new ShaderMaterial({
    transparent: true,
    depthWrite: false,
    blending: AdditiveBlending,
    side: DoubleSide,
    uniforms: {
      uTime: { value: 0 },
      uPulse: { value: 0 },
      uSeed: { value: seed },
      uMotion: { value: motion },
      uA: { value: a },
      uB: { value: b },
      uC: { value: c },
    },
    vertexShader: /* glsl */ `
      varying vec2 vUv;
      void main() {
        vUv = uv;
        gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
      }
    `,
    fragmentShader: /* glsl */ `
      uniform float uTime;
      uniform float uPulse;
      uniform float uSeed;
      uniform float uMotion;
      uniform vec3 uA;
      uniform vec3 uB;
      uniform vec3 uC;
      varying vec2 vUv;

      float hash(float n) { return fract(sin(n) * 43758.5453); }
      float noise(float x) {
        float i = floor(x);
        float f = fract(x);
        return mix(hash(i), hash(i + 1.0), f * f * (3.0 - 2.0 * f));
      }
      float fbm(float x) {
        float v = 0.0;
        float amp = 0.5;
        for (int i = 0; i < 4; i++) {
          v += amp * noise(x);
          x *= 2.1;
          amp *= 0.5;
        }
        return v;
      }

      void main() {
        float t = uTime * 0.1 * uMotion + uSeed;
        // Le bas du rideau est net et lumineux, le haut se dissout en rayons verticaux.
        float wave = fbm(vUv.x * 6.0 + t) * 0.8 + fbm(vUv.x * 2.0 - t * 0.6) * 0.5;
        float base = 0.12 + wave * 0.28;
        float rise = vUv.y - base;
        float curtain = smoothstep(-0.02, 0.03, rise) * exp(-max(rise, 0.0) * 4.2);
        float rays = 0.5 + 0.5 * sin(vUv.x * 140.0 + wave * 18.0 + t * 3.0);
        rays = mix(0.55, rays, smoothstep(0.0, 0.5, rise));
        float fadeX = smoothstep(0.0, 0.2, vUv.x) * smoothstep(1.0, 0.8, vUv.x);
        vec3 tint = mix(uA, uB, smoothstep(0.2, 0.7, wave));
        tint = mix(tint, uC, smoothstep(0.1, 0.6, rise));
        float intensity = curtain * rays * fadeX * (0.2 + uPulse * 0.22);
        gl_FragColor = vec4(tint * intensity, intensity);
      }
    `,
  });
}
