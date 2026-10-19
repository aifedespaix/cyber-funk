/**
 * Remplaçant minimal de `three` (BOUCHON, pas une copie de Three.js).
 *
 * `sections.ts` n'utilise `Color` que pour la teinte d'ambiance `mood`, qui ne fait pas partie des données de
 * référence (elle sera portée en J4-08). On évite ainsi une dépendance npm de plusieurs dizaines de Mo, et surtout
 * l'installation automatique de Bun qui irait chercher une version arbitraire de `three` sur le réseau.
 * La résolution passe par `paths` dans `tsconfig.json`.
 */
export class Color {
    r = 1;
    g = 1;
    b = 1;

    clone(): Color {
        return new Color().copy(this);
    }

    copy(other: Color): this {
        this.r = other.r;
        this.g = other.g;
        this.b = other.b;
        return this;
    }

    lerp(other: Color, alpha: number): this {
        this.r += (other.r - this.r) * alpha;
        this.g += (other.g - this.g) * alpha;
        this.b += (other.b - this.b) * alpha;
        return this;
    }
}
