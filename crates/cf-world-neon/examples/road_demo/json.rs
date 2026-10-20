//! `scenes.json` : les réglages de chaque scène de `CF_CHECK`, lus par `tools/sillage-golden/road-render/render.mjs`
//! pour rendre la référence Sillage avec exactement les mêmes valeurs.

use std::fmt::Write as _;

/// Une scène telle que l'exemple l'a réellement rendue.
pub(crate) struct Row {
    pub name: &'static str,
    pub car_z: f64,
    pub time_s: f64,
    pub pulse: f32,
    pub bass: f32,
    pub intensity: f32,
    pub wet: f32,
    pub rain: f32,
    pub draw: f32,
    pub brake: f32,
    pub car: bool,
    /// Origine flottante au moment de la capture (z, m).
    pub origin_z: f64,
    pub fov_deg: f32,
    pub camera: [f64; 3],
    /// Direction du regard (unitaire).
    pub forward: [f32; 3],
}

pub(crate) fn scenes_json(rows: &[Row]) -> String {
    let mut out = String::from("[\n");
    for (i, r) in rows.iter().enumerate() {
        let _ = writeln!(
            out,
            "  {{\"name\": \"{}\", \"carZ\": {}, \"time\": {}, \"pulse\": {}, \"bass\": {}, \"intensity\": {}, \
             \"wet\": {}, \"rain\": {}, \"draw\": {}, \"brake\": {}, \"car\": {}, \"originZ\": {}, \"fov\": {}, \
             \"camera\": [{}, {}, {}], \"forward\": [{}, {}, {}]}}{}",
            r.name,
            r.car_z,
            r.time_s,
            r.pulse,
            r.bass,
            r.intensity,
            r.wet,
            r.rain,
            r.draw,
            r.brake,
            r.car,
            r.origin_z,
            r.fov_deg,
            r.camera[0],
            r.camera[1],
            r.camera[2],
            r.forward[0],
            r.forward[1],
            r.forward[2],
            if i + 1 < rows.len() { "," } else { "" },
        );
    }
    out.push(']');
    out
}
