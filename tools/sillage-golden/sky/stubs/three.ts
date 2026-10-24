/**
 * Remplaçant minimal de `three` (BOUCHON, pas une copie de Three.js) pour `sky.ts`, `planets.ts` et `shooting-stars.ts`.
 *
 * Il fait juste assez pour que `new Sky(…)`, `Sky.load` et `Sky.shoot` calculent et rangent leurs données : les
 * `BufferAttribute` retiennent leurs tableaux, `Vector3` implémente les opérations que le code d'origine utilise (avec les
 * vraies formules), les groupes retiennent leurs enfants. Aucun rendu, aucune matrice. La résolution passe par `paths` dans
 * `tsconfig.json`.
 */
export class Vector3 {
    constructor(
        public x = 0,
        public y = 0,
        public z = 0,
    ) {}
    set(x: number, y: number, z: number): this {
        this.x = x;
        this.y = y;
        this.z = z;
        return this;
    }
    setScalar(s: number): this {
        return this.set(s, s, s);
    }
    copy(v: Vector3): this {
        return this.set(v.x, v.y, v.z);
    }
    clone(): Vector3 {
        return new Vector3(this.x, this.y, this.z);
    }
    add(v: Vector3): this {
        return this.set(this.x + v.x, this.y + v.y, this.z + v.z);
    }
    sub(v: Vector3): this {
        return this.set(this.x - v.x, this.y - v.y, this.z - v.z);
    }
    negate(): this {
        return this.set(-this.x, -this.y, -this.z);
    }
    multiplyScalar(s: number): this {
        return this.set(this.x * s, this.y * s, this.z * s);
    }
    addScaledVector(v: Vector3, s: number): this {
        return this.set(this.x + v.x * s, this.y + v.y * s, this.z + v.z * s);
    }
    dot(v: Vector3): number {
        return this.x * v.x + this.y * v.y + this.z * v.z;
    }
    length(): number {
        return Math.hypot(this.x, this.y, this.z);
    }
    normalize(): this {
        return this.multiplyScalar(1 / (this.length() || 1));
    }
    crossVectors(a: Vector3, b: Vector3): this {
        return this.set(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x);
    }
    /** Rotation de Rodrigues autour d'un axe unitaire, comme `Vector3.applyAxisAngle`. */
    applyAxisAngle(axis: Vector3, angle: number): this {
        const c = Math.cos(angle);
        const s = Math.sin(angle);
        const d = axis.dot(this);
        const cross = new Vector3().crossVectors(axis, this);
        return this.set(
            this.x * c + cross.x * s + axis.x * d * (1 - c),
            this.y * c + cross.y * s + axis.y * d * (1 - c),
            this.z * c + cross.z * s + axis.z * d * (1 - c),
        );
    }
}

/** Matrice de changement de base : jamais utilisée sans `ShootingStars.update`, que le script n'appelle pas. */
export class Matrix4 {
    makeBasis(): this {
        return this;
    }
}

/** Une couleur est un numéro de rôle (`id`) : le script lit quelle teinte une étoile a tirée. */
export class Color {
    id: number;
    r: number;
    g: number;
    b: number;
    constructor(id = 0) {
        this.id = id;
        // Un canal qui encode le numéro : `tint.r * dim` (étoiles) se relit en `Math.round(r / dim) - 1`.
        this.r = id + 1;
        this.g = id + 1;
        this.b = id + 1;
    }
    copy(other: Color): this {
        this.id = other.id;
        this.r = other.r;
        this.g = other.g;
        this.b = other.b;
        return this;
    }
}

export class BufferAttribute {
    constructor(
        readonly array: ArrayLike<number>,
        readonly itemSize: number,
    ) {}
    needsUpdate = false;
}

export class BufferGeometry {
    private readonly attributes = new Map<string, BufferAttribute>();
    setAttribute(name: string, attribute: BufferAttribute): this {
        this.attributes.set(name, attribute);
        return this;
    }
    getAttribute(name: string): BufferAttribute {
        return this.attributes.get(name)!;
    }
    rotateX(): this {
        return this;
    }
    dispose(): void {}
}

export class SphereGeometry extends BufferGeometry {
    constructor(..._args: number[]) {
        super();
    }
}
export class PlaneGeometry extends BufferGeometry {
    constructor(..._args: number[]) {
        super();
    }
}
export class RingGeometry extends BufferGeometry {
    constructor(..._args: number[]) {
        super();
    }
}

export class Group {
    readonly children: unknown[] = [];
    readonly position = new Vector3();
    readonly rotation = { x: 0, y: 0, z: 0 };
    readonly scale = new Vector3(1, 1, 1);
    add(...children: unknown[]): this {
        this.children.push(...children);
        return this;
    }
}

export class Mesh extends Group {
    visible = true;
    frustumCulled = true;
    renderOrder = 0;
    readonly quaternion = {};
    constructor(
        readonly geometry: BufferGeometry,
        readonly material: unknown,
    ) {
        super();
    }
}

export class Points extends Mesh {}

export class ShaderMaterial {
    uniforms: Record<string, { value: any }>;
    constructor(parameters: { uniforms?: Record<string, { value: unknown }> } = {}) {
        this.uniforms = (parameters.uniforms ?? {}) as Record<string, { value: any }>;
    }
    dispose(): void {}
}

export class Texture {
    colorSpace = "";
    anisotropy = 1;
    dispose(): void {}
}

export class TextureLoader {
    /** Toutes les textures « se chargent » : la géante à anneau n'existe qu'une fois sa texture arrivée. */
    async loadAsync(_url: string): Promise<Texture> {
        return new Texture();
    }
}

export const AdditiveBlending = 2;
export const BackSide = 1;
export const DoubleSide = 2;
export const SRGBColorSpace = "srgb";
