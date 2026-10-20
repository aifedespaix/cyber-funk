/** Uniformes communs à tous les matériaux d'un immeuble-vitrine, remplis une fois par image. */
export const FRAME_GLSL = /* glsl */ `
  uniform float uTime;
  uniform float uPulse;
  uniform float uBass;
  uniform float uMid;
  uniform float uHigh;
  uniform float uBeat;
  uniform float uIntensity;
  uniform float uPhase;
  uniform vec3 uMood;
  uniform vec3 uInk;
  uniform vec3 uIce;
  uniform vec3 uHaze;
`;

export const NOISE_GLSL = /* glsl */ `
  float h11(float x) { return fract(sin(x * 127.1) * 43758.5453); }
  float h21(vec2 p) {
    p = fract(p * vec2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
  }
  float vnoise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(h21(i), h21(i + vec2(1.0, 0.0)), f.x), mix(h21(i + vec2(0.0, 1.0)), h21(i + vec2(1.0, 1.0)), f.x), f.y);
  }
  float fbm(vec2 p) {
    float a = 0.5;
    float s = 0.0;
    for (int i = 0; i < 4; i++) {
      s += a * vnoise(p);
      p = p * 2.03 + 7.1;
      a *= 0.5;
    }
    return s;
  }
`;

/**
 * Comme les gratte-ciel : les structures « poussent » du sol entre 300 et
 * 440 m (aucune apparition brutale), et le brouillard les avale au loin.
 */
export const CITY_GLSL = /* glsl */ `
  float cityGrow(vec3 wp) { return 1.0 - smoothstep(300.0, 440.0, distance(wp.xz, cameraPosition.xz)); }
  vec3 cityFog(vec3 col, vec3 wp) {
    float f = smoothstep(90.0, 380.0, distance(wp, cameraPosition));
    return mix(col, uInk + uHaze * 0.05, f);
  }
  float cityFade(vec3 wp) { return 1.0 - smoothstep(110.0, 400.0, distance(wp, cameraPosition)); }
`;

/** Rend un mot de l'atlas lettre par lettre : tranches décalées, aberration chromatique, lettres éteintes. */
export const GLYPH_GLSL = /* glsl */ `
  // word : x = rangée de l'atlas (4 rangées, 0 en haut), y/z = début et fin du texte, w = nombre de lettres.
  // Renvoie un masque par canal : le rouge et le bleu sont décalés, le vert est le cœur.
  vec3 glyphWord(sampler2D map, vec2 uv, vec4 word, float glitch, float t) {
    float slice = floor(uv.y * 16.0);
    float tick = floor(t * (6.0 + glitch * 18.0));
    float hit = step(1.0 - 0.12 - glitch * 0.5, h11(slice * 7.3 + tick));
    float jump = (h11(slice + tick * 1.7) - 0.5) * 0.3 * glitch * hit;
    float shift = 0.003 + glitch * 0.02;
    float v = 1.0 - (word.x + 1.0 - uv.y) / 4.0;
    vec3 m = vec3(0.0);
    for (int c = 0; c < 3; c++) {
      float x = uv.x + jump + (float(c) - 1.0) * shift;
      float letter = floor((x - word.y) / max(word.z - word.y, 0.001) * word.w);
      float off = step(h11(letter * 13.1 + floor(t * (4.0 + glitch * 26.0))), glitch * 0.55);
      m[c] = texture2D(map, vec2(x, v)).r * (1.0 - off);
    }
    return m;
  }
`;
