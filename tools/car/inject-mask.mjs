// Embeds the paint mask in a GLB as the occlusion texture (R channel) of the 'body_paint' material.
// CyberCar.ts reads it back (material.aoMap), removes it from the PBR pipeline and uses it as the paint mask.
// Usage: node inject-mask.mjs <in.glb> <mask.png> <out.glb> [material name, default body_paint]
import { NodeIO } from '@gltf-transform/core';
import { ALL_EXTENSIONS } from '@gltf-transform/extensions';
import { readFileSync } from 'node:fs';

const [, , input, maskPath, output, matName = 'body_paint'] = process.argv;
// ALL_EXTENSIONS is required: without it NodeIO silently DROPS KHR_materials_emissive_strength (neons would be 4-10x too dim).
const io = new NodeIO().registerExtensions(ALL_EXTENSIONS);
const doc = await io.read(input);
const mat = doc.getRoot().listMaterials().find((m) => m.getName() === matName);
if (!mat) throw new Error(`material '${matName}' not found`);
const tex = doc.createTexture('paint_mask').setMimeType('image/png').setImage(readFileSync(maskPath));
mat.setOcclusionTexture(tex).setOcclusionStrength(1);
mat.getOcclusionTextureInfo().setTexCoord(0);
await io.write(output, doc);
console.log('mask injected ->', output);
