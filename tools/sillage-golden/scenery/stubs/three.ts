/**
 * Remplaçant minimal de `three` (BOUCHON, pas une copie de Three.js) pour `scenery.ts` et `bridges.ts`.
 *
 * Il fait juste assez pour que `Scenery.update` calcule et range sa ville : `Matrix4.compose` retient la position et
 * l'échelle (la rotation est toujours l'identité dans `scenery.ts`), `InstancedMesh.setMatrixAt` et `setColorAt` copient
 * ce qu'on leur donne (le code d'origine réutilise un seul objet). Aucun rendu. La résolution passe par `paths` dans
 * `tsconfig.json`.
 */
export class Vector3 {
    x = 0;
    y = 0;
    z = 0;
    constructor(x = 0, y = 0, z = 0) {
        this.set(x, y, z);
    }
    set(x: number, y: number, z: number): this {
        this.x = x;
        this.y = y;
        this.z = z;
        return this;
    }
}

export class Quaternion {}

/** Une couleur n'a qu'un identifiant : le test regarde quelle teinte du cycle une tour a tirée. */
export class Color {
    constructor(readonly id = "") {}
}

export class Matrix4 {
    px = 0;
    py = 0;
    pz = 0;
    sx = 1;
    sy = 1;
    sz = 1;
    compose(position: Vector3, _rotation: Quaternion, scale: Vector3): this {
        this.px = position.x;
        this.py = position.y;
        this.pz = position.z;
        this.sx = scale.x;
        this.sy = scale.y;
        this.sz = scale.z;
        return this;
    }
}

export class InstancedBufferAttribute {
    needsUpdate = false;
    constructor(
        readonly array: Float32Array,
        readonly itemSize: number,
    ) {}
}

export class BoxGeometry {
    private readonly attributes = new Map<string, InstancedBufferAttribute>();
    translate(): this {
        return this;
    }
    setAttribute(name: string, attribute: InstancedBufferAttribute): this {
        this.attributes.set(name, attribute);
        return this;
    }
    getAttribute(name: string): InstancedBufferAttribute | undefined {
        return this.attributes.get(name);
    }
    dispose(): void {}
}

export class Group {
    readonly children: unknown[] = [];
    add(child: unknown): this {
        this.children.push(child);
        return this;
    }
}

export class InstancedMesh {
    frustumCulled = true;
    readonly instanceMatrix = { needsUpdate: false };
    instanceColor: { needsUpdate: boolean } | null = null;
    readonly matrices: Matrix4[];
    readonly colors: (Color | undefined)[];
    constructor(
        readonly geometry: BoxGeometry,
        readonly material: unknown,
        readonly count: number,
    ) {
        this.matrices = Array.from({ length: count }, () => new Matrix4());
        this.colors = Array.from({ length: count }, () => undefined);
    }
    setMatrixAt(index: number, matrix: Matrix4): void {
        Object.assign(this.matrices[index]!, matrix);
    }
    setColorAt(index: number, color: Color): void {
        this.colors[index] = color;
        this.instanceColor ??= { needsUpdate: false };
    }
    dispose(): void {}
}

export type ShaderMaterial = { uniforms: Record<string, { value: number }>; dispose(): void };
export const SRGBColorSpace = "srgb";
