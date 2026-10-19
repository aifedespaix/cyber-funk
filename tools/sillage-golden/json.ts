/**
 * Sérialisation JSON stable et lisible : objets indentés, tableaux de valeurs simples sur une ligne, tableaux de petits
 * objets plats à raison d'un objet par ligne. Le but est un diff git lisible quand on régénère, sans exploser le nombre
 * de lignes (un nombre par ligne avec `JSON.stringify(_, null, 2)`).
 */

type Json = null | boolean | number | string | Json[] | { [key: string]: Json };

export function formatJson(value: Json): string {
    return `${format(value, "")}\n`;
}

function format(value: Json, indent: string): string {
    if (value === null || typeof value !== "object") return JSON.stringify(value);
    const inner = `${indent}  `;
    if (Array.isArray(value)) {
        if (value.length === 0) return "[]";
        if (value.every(isScalar)) return JSON.stringify(value);
        if (value.every(isFlatObject)) return `[\n${value.map((v) => inner + compact(v)).join(",\n")}\n${indent}]`;
        return `[\n${value.map((v) => inner + format(v, inner)).join(",\n")}\n${indent}]`;
    }
    const entries = Object.entries(value);
    if (entries.length === 0) return "{}";
    return `{\n${entries.map(([k, v]) => `${inner}${JSON.stringify(k)}: ${format(v, inner)}`).join(",\n")}\n${indent}}`;
}

function isScalar(value: Json): boolean {
    return value === null || typeof value !== "object";
}

function isFlatObject(value: Json): boolean {
    return value !== null && typeof value === "object" && !Array.isArray(value) && Object.values(value).every(isScalar);
}

/** `{ "step": 12, "bpm": 128 }` : un objet plat sur une ligne, avec des espaces pour la lecture. */
function compact(value: Json): string {
    const entries = Object.entries(value as Record<string, Json>);
    return `{ ${entries.map(([k, v]) => `${JSON.stringify(k)}: ${JSON.stringify(v)}`).join(", ")} }`;
}

export type { Json };
