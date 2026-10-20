/** Passe d'étalonnage finale : aberration, gouttes sur l'objectif, vignette, grain, et les effets du spectacle. */
export const GRADE = {
  uniforms: {
    tDiffuse: { value: null },
    uAberration: { value: 0 },
    uTime: { value: 0 },
    uGrain: { value: 0.035 },
    uLens: { value: 0 },
    uRainTime: { value: 0 },
    uAspect: { value: 1.78 },
    uFlash: { value: 0 },
    uFlashColor: { value: null },
    uSweep: { value: 0 },
    uSweepPos: { value: -1 },
    uSweepColor: { value: null },
    uContrast: { value: 1 },
    uSaturation: { value: 1 },
    uSlow: { value: 0 },
    uBlur: { value: 0 },
  },
  vertexShader: /* glsl */ `
    varying vec2 vUv;
    void main() {
      vUv = uv;
      gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
    }
  `,
  fragmentShader: /* glsl */ `
    uniform sampler2D tDiffuse;
    uniform float uAberration;
    uniform float uTime;
    uniform float uGrain;
    uniform float uLens;
    uniform float uRainTime;
    uniform float uAspect;
    uniform float uFlash;
    uniform vec3 uFlashColor;
    uniform float uSweep;
    uniform float uSweepPos;
    uniform vec3 uSweepColor;
    uniform float uContrast;
    uniform float uSaturation;
    uniform float uSlow;
    uniform float uBlur;
    varying vec2 vUv;

    float hh(vec2 p, float k) {
      return fract(sin(dot(p, vec2(127.1, 311.7)) + k * 91.7) * 43758.5453);
    }

    // Une couche de gouttes sur l'objectif : une grille de cellules, une goutte au plus par cellule, qui glisse
    // par à-coups du haut vers le bas de sa case en laissant un filet. Renvoie xy = décalage de réfraction,
    // z = opacité de la goutte, w = reflet.
    vec4 dropLayer(vec2 uv, float scale, float amount, float seed, float trailAmount) {
      vec2 p = vec2(uv.x * uAspect, uv.y) * scale;
      vec2 id = floor(p);
      vec2 f = fract(p) - 0.5;
      float h0 = hh(id, seed);
      if (h0 > amount) return vec4(0.0);
      float h1 = hh(id, seed + 1.0);
      float h2 = hh(id, seed + 2.0);
      float h3 = hh(id, seed + 3.0);
      float s = fract(uRainTime * (0.05 + 0.1 * h1) + h2);
      float ease = s * s * (3.0 - 2.0 * s);
      vec2 c = vec2((h1 - 0.5) * 0.5, mix(0.32, -0.32, ease));
      float fade = smoothstep(0.0, 0.08, s) * (1.0 - smoothstep(0.9, 1.0, s));
      float r = 0.08 + h3 * 0.08;
      vec2 d = (f - c) * vec2(1.0, 0.85);
      float body = (1.0 - smoothstep(r * 0.55, r, length(d))) * fade;
      // Le filet laissé derrière la goutte : fin, et de plus en plus pâle vers le haut.
      float t = clamp((f.y - c.y) / (0.32 - c.y + 0.001), 0.0, 1.0);
      float trail = trailAmount * step(c.y, f.y) * (1.0 - smoothstep(0.0, 0.035, abs(f.x - c.x))) * (1.0 - t) * 0.5 * fade;
      vec2 refract = -d / r * body * 0.35 / scale;
      float glint = body * (1.0 - smoothstep(0.0, 0.5, length(d / r - vec2(-0.3, 0.4))));
      return vec4(refract + vec2(0.0, -trail * 0.01), max(body, trail * 0.6), glint);
    }

    void main() {
      vec2 uv = vUv;
      float wet = 0.0;
      float glint = 0.0;
      if (uLens > 0.001) {
        vec4 big = dropLayer(uv, 5.0, 0.2 * uLens, 1.0, 1.0);
        vec4 small = dropLayer(uv + 0.37, 15.0, 0.22 * uLens, 7.0, 0.0);
        uv += big.xy + small.xy;
        wet = max(big.z, small.z * 0.8);
        glint = big.w + small.w * 0.6;
      }
      vec2 c = vUv - 0.5;
      // Aberration chromatique : elle s'accentue vers les bords et sur les kicks
      vec2 shift = c * uAberration * (0.4 + dot(c, c) * 2.0);
      float r = texture2D(tDiffuse, uv + shift).r;
      float g = texture2D(tDiffuse, uv).g;
      float b = texture2D(tDiffuse, uv - shift).b;
      vec3 col = vec3(r, g, b);
      // Bullet-time : flou radial léger (quelques prises vers le centre), plus fort vers les bords.
      if (uBlur > 0.001) {
        vec3 acc = col;
        for (int i = 1; i <= 4; i++) acc += texture2D(tDiffuse, uv - c * (float(i) * 0.014 * uBlur)).rgb;
        col = acc / 5.0;
      }
      // Étalonnage du niveau : saturation et contraste ≥ 1, pivot bas pour ne pas boucher les noirs.
      float luma = dot(col, vec3(0.2126, 0.7152, 0.0722));
      col = max(vec3(0.0), mix(vec3(luma), col, uSaturation));
      col = max(vec3(0.0), 0.1 + (col - 0.1) * uContrast);
      // Goutte : un peu plus sombre en son centre, un reflet clair sur le haut.
      col = mix(col, col * 0.82, wet) + glint * 0.22;
      // Vignette et lignes de balayage très discrètes
      float vignette = smoothstep(0.95, 0.25, length(c) * 1.25);
      col *= mix(0.5, 1.0, vignette);
      col *= 0.97 + 0.03 * sin(vUv.y * 1100.0);
      if (uSlow > 0.001) {
        col = mix(col, vec3(dot(col, vec3(0.2126, 0.7152, 0.0722))), 0.35 * uSlow);
        col *= 1.0 - 0.3 * uSlow * smoothstep(0.25, 0.85, length(c) * 1.4);
      }
      // Vague de lumière d'un changement de niveau, puis flash : leurs niveaux viennent du limiteur photosensible.
      float band = exp(-pow((vUv.x - uSweepPos) / 0.18, 2.0));
      col += uSweepColor * (uSweep * band);
      col += uFlashColor * uFlash;
      float grain = fract(sin(dot(vUv + fract(uTime), vec2(12.9898, 78.233))) * 43758.5453);
      col += (grain - 0.5) * uGrain;
      gl_FragColor = vec4(col, 1.0);
    }
  `,
};
