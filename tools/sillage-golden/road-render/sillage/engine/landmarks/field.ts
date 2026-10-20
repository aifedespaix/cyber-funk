import { Color, DataTexture, Vector3, Vector4, type Texture } from "three";

/** Lumières dynamiques par image : assez pour un robot (tête, torse, deux mains) et un reflet de rue. */
export const MAX_LIGHTS = 6;

/**
 * Le décor est en matériaux custom, sans lumières three.js : les « vraies »
 * lumières des immeubles-vitrines sont donc un petit tableau d'uniformes que
 * lisent les façades de la ville et la route (diffus, plus reflet sur sol
 * mouillé). Les mêmes objets d'uniformes sont partagés par tous les matériaux.
 *
 * S'y ajoutent une zone de glitch (scanlines et inversions de canaux sur les
 * façades voisines et la route) et un décalque au sol (le mot projeté par
 * l'immeuble ERROR, qui suit la voiture).
 */
export class CityField {
  readonly uniforms = {
    uLightPos: { value: Array.from({ length: MAX_LIGHTS }, () => new Vector4(0, -999, 0, 1)) },
    uLightCol: { value: Array.from({ length: MAX_LIGHTS }, () => new Vector3()) },
    /** x = z du centre, y = demi-longueur, z = force 0–1. */
    uZone: { value: new Vector4(0, 1, 0, 0) },
    uZoneTint: { value: new Color() },
    /** Panne de courant : x = z du front, y = largeur du front (m), z = part de la ville éteinte derrière le front (0–1), w = sens (1 : éteint vers -z). */
    uPower: { value: new Vector4(0, 1, 0, 1) },
    /** Éclair d'orage : lumière froide ajoutée aux façades (0–1). */
    uFlash: { value: 0 },
    /** Pouls des fenêtres (événement « city-pulse ») : part des fenêtres éteintes qui s'allument sur le kick, 0–1. */
    uWinPulse: { value: 0 },
    uDecalMap: { value: placeholder() as Texture },
    /** Centre x, centre z, demi-largeur, demi-profondeur du décalque, en mètres monde. */
    uDecalBox: { value: new Vector4(0, 0, 1, 1) },
    uDecalAmt: { value: 0 },
    /** Rangée de l'atlas, début et fin du texte (0–1), nombre de lettres. */
    uDecalWord: { value: new Vector4(0, 0, 1, 1) },
    uDecalGlitch: { value: 0 },
    /** Humidité de la chaussée et des façades (0–1), et intensité de la pluie qui tombe (0–1). */
    uWet: { value: 0 },
    uRain: { value: 0 },
  };

  /** Règle la lumière `index` ; une intensité nulle l'éteint. */
  setLight(index: number, x: number, y: number, z: number, radius: number, color: Color, intensity: number): void {
    const u = this.uniforms;
    u.uLightPos.value[index]?.set(x, y, z, radius);
    u.uLightCol.value[index]?.set(color.r * intensity, color.g * intensity, color.b * intensity);
  }

  setZone(z: number, halfLength: number, strength: number, tint: Color): void {
    this.uniforms.uZone.value.set(z, halfLength, strength, 0);
    this.uniforms.uZoneTint.value.copy(tint);
  }

  /** Éteint tout : lumières, zone et décalque. */
  clear(): void {
    const u = this.uniforms;
    for (const p of u.uLightPos.value) p.set(0, -999, 0, 1);
    for (const c of u.uLightCol.value) c.set(0, 0, 0);
    u.uZone.value.z = 0;
    u.uWinPulse.value = 0;
    u.uDecalAmt.value = 0;
  }
}

/** Texture 1×1 qui évite un échantillonneur vide tant qu'aucun décalque n'est posé. */
function placeholder(): DataTexture {
  const texture = new DataTexture(new Uint8Array([0, 0, 0, 255]), 1, 1);
  texture.needsUpdate = true;
  return texture;
}

/** GLSL partagé par les gratte-ciel et la route : éclairage des lumières dynamiques et zone de glitch. */
export const FIELD_GLSL = /* glsl */ `
  uniform vec4 uLightPos[${MAX_LIGHTS}];
  uniform vec3 uLightCol[${MAX_LIGHTS}];
  uniform vec4 uZone;
  uniform vec3 uZoneTint;
  uniform float uWet;
  uniform float uRain;

  // Diffus : une lueur qui s'éteint avec la distance, un peu plus forte face à la source.
  vec3 fieldLight(vec3 p, vec3 n) {
    vec3 sum = vec3(0.0);
    for (int i = 0; i < ${MAX_LIGHTS}; i++) {
      vec3 L = uLightPos[i].xyz - p;
      float d = length(L);
      float att = clamp(1.0 - d / uLightPos[i].w, 0.0, 1.0);
      float facing = 0.35 + 0.65 * max(dot(n, L / max(d, 0.001)), 0.0);
      sum += uLightCol[i] * att * att * facing;
    }
    return sum;
  }

  // Sol mouillé : le reflet d'une source haute s'étire vers la caméra.
  vec3 fieldSheen(vec3 p) {
    vec3 V = normalize(cameraPosition - p);
    vec3 R = vec3(-V.x, V.y, -V.z);
    vec3 sum = vec3(0.0);
    for (int i = 0; i < ${MAX_LIGHTS}; i++) {
      vec3 L = uLightPos[i].xyz - p;
      float d = length(L);
      float att = clamp(1.0 - d / (uLightPos[i].w * 1.7), 0.0, 1.0);
      float spec = pow(max(dot(R, L / max(d, 0.001)), 0.0), 18.0);
      sum += uLightCol[i] * spec * att;
    }
    return sum;
  }

  uniform vec4 uPower;
  uniform float uFlash;
  uniform float uWinPulse;

  // Part de lumière qui reste aux fenêtres : 1 partout, moins derrière le front d'une panne de courant.
  float fieldPower(vec3 p) {
    float behind = smoothstep(-uPower.y, uPower.y, (p.z - uPower.x) * uPower.w);
    return 1.0 - uPower.z * behind;
  }

  float fieldZone(vec3 p) {
    return uZone.z * (1.0 - smoothstep(uZone.y * 0.55, uZone.y, abs(p.z - uZone.x)));
  }
`;
