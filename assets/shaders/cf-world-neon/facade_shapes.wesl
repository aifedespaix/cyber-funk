// Les formes de lumière d'une façade : le masque 0..1 de la zone allumée d'une cellule (une travée sur un étage).
//
// Ce texte est ajouté au bout de `facade.wesl` (voir `FACADE_SOURCE` dans `material.rs`) : il lit le bloc `facade` et
// `h21` de ce fichier. Les indices de forme sont ceux de `LightShape` (`temperament.rs`) :
//   0 fenêtres pleines, 1 bandeaux, 2 colonnes, 3 enseigne, 4 écran qui défile, 5 étage teinté.
//
// Toutes les formes se fondent en la même teinte moyenne quand `fade_out` monte à 1 (immeubles lointains, fenêtres
// sous-pixel) : rien ne scintille au loin.

// Part de lumière moyenne des fenêtres pleines, vers laquelle toutes les formes se fondent au loin.
const SHAPE_AVERAGE: f32 = 0.34;
// Période de l'écran qui défile, en secondes : elle divise 2 400 s, la durée au bout de laquelle le temps du shader
// reboucle, donc le défilement ne saute pas au rebouclage.
const SCREEN_PERIOD_S: f32 = 8.0;

// Un rectangle de `lo` à `hi` dans la cellule.
fn rect(f: vec2<f32>, lo: vec2<f32>, hi: vec2<f32>) -> f32 {
    return step(lo.x, f.x) * step(f.x, hi.x) * step(lo.y, f.y) * step(f.y, hi.y);
}

// `cell` : (travée, étage) entiers ; `f` : position dans la cellule, 0..1.
fn shape_mask(shape: f32, cell: vec2<f32>, f: vec2<f32>, seed: f32, fade_out: f32) -> f32 {
    var m = rect(f, vec2<f32>(0.16, 0.2), vec2<f32>(0.84, 0.8));
    if shape > 4.5 {
        // Étage teinté : la bande de l'étage d'un seul tenant, sans montant entre les travées.
        m = rect(f, vec2<f32>(0.0, 0.12), vec2<f32>(1.0, 0.88));
    } else if shape > 3.5 {
        // Écran qui défile : des bandes verticales qui avancent le long de l'étage, décalées d'un étage à l'autre.
        let phase = fract((cell.x + f.x) / 6.0 - facade.frame.x / SCREEN_PERIOD_S + h21(vec2<f32>(cell.y, seed)));
        m = rect(f, vec2<f32>(0.0, 0.1), vec2<f32>(1.0, 0.9)) * smoothstep(0.0, 0.08, phase) * (1.0 - smoothstep(0.42, 0.5, phase));
    } else if shape > 2.5 {
        // Enseigne : un bloc de 2×2 travées, cadré comme une seule grande vitre.
        let block = floor(cell * 0.5);
        let g = (cell - block * 2.0 + f) * 0.5;
        m = rect(g, vec2<f32>(0.08, 0.1), vec2<f32>(0.92, 0.9));
    } else if shape > 1.5 {
        // Colonnes : un trait vertical qui traverse l'étage de bord à bord.
        m = rect(f, vec2<f32>(0.38, 0.0), vec2<f32>(0.62, 1.0));
    } else if shape > 0.5 {
        // Bandeaux : une travée sur deux d'un étage, en bande basse et large.
        let odd = step(0.25, fract((cell.x + cell.y + floor(seed)) * 0.5));
        m = rect(f, vec2<f32>(0.0, 0.32), vec2<f32>(1.0, 0.68)) * (1.0 - odd);
    }
    return mix(m, SHAPE_AVERAGE, fade_out);
}
