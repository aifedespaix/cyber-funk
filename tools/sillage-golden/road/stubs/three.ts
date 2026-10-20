/**
 * Remplaçant minimal de `three` (BOUCHON, pas une copie de Three.js) pour `bridges.ts`.
 *
 * Le générateur n'appelle que les fonctions pures de `bridges.ts` (`bridgeZ`, `bridgeSpan`, `isNearBridge`) : les
 * classes ci-dessous n'ont qu'à exister pour que les imports se résolvent, `new Bridges(...)` n'est jamais appelé.
 * La résolution passe par `paths` dans `tsconfig.json`.
 */
export class BoxGeometry {}
export class Color {}
export class Group {}
export class InstancedBufferAttribute {}
export class InstancedMesh {}
export class Matrix4 {}
export class Quaternion {}
export class Vector3 {}
export type ShaderMaterial = object;
