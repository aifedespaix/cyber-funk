//! `phases.json` : les uniformes envoyés au shader à chaque étape de `CF_CHECK`, lus par
//! `tools/sillage-golden/postfx/render.mjs` pour rendre la référence Sillage avec les mêmes valeurs.

use std::fmt::Write as _;

use cf_render::postfx::GradeUniforms;

/// Les uniformes de chaque étape, pour rendre la référence Sillage avec exactement les mêmes valeurs.
pub(crate) fn phases_json(rows: &[(&'static str, GradeUniforms, f32)]) -> String {
    let mut out = String::from("[\n");
    for (i, (name, g, bloom)) in rows.iter().enumerate() {
        let _ = writeln!(
            out,
            "  {{\"name\": \"{name}\", \"bloom\": {bloom}, \"aberration\": {}, \"time\": {}, \"grain\": {}, \
             \"lens\": {}, \"rainTime\": {}, \"aspect\": {}, \"flash\": {}, \"sweep\": {}, \"sweepPos\": {}, \
             \"contrast\": {}, \"saturation\": {}, \"slow\": {}, \"blur\": {}, \"flashColor\": [{}, {}, {}], \
             \"sweepColor\": [{}, {}, {}]}}{}",
            g.aberration,
            g.time,
            g.grain,
            g.lens,
            g.rain_time,
            g.aspect,
            g.flash,
            g.sweep,
            g.sweep_pos,
            g.contrast,
            g.saturation,
            g.slow,
            g.blur,
            g.flash_color.x,
            g.flash_color.y,
            g.flash_color.z,
            g.sweep_color.x,
            g.sweep_color.y,
            g.sweep_color.z,
            if i + 1 < rows.len() { "," } else { "" },
        );
    }
    out.push(']');
    out
}
