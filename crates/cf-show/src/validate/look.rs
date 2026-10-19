//! Règles d'un look, de ses portes, de ses macros, et des surcharges qu'un cue lui applique.

use super::{Issue, Report, index, join};
use crate::model::look::{MAX_CUT_BEATS, MAX_FLOOR, MAX_LOOK_NAME};
use crate::model::params::{MAX_DRIVES, MAX_MACRO_NAME, MAX_MACROS};
use crate::model::{CutGrid, FORMAT_VERSION, FxGates, LayerMix, Look, LookOverrides, Macro, ParamPath, ShowGates};

/// Vérifie un look seul (fichier de la bibliothèque) ; renvoie tous les problèmes, vide si le look est valide.
#[must_use]
pub fn validate_look(look: &Look) -> Vec<Issue> {
    let mut report = Report::default();
    check_look(&mut report, "", look);
    report.into_issues()
}

/// Vérifie un look sous le chemin `prefix` (`""` pour un fichier de look, `looks[2]` dans un événement).
pub(crate) fn check_look(r: &mut Report, prefix: &str, look: &Look) {
    if look.version != FORMAT_VERSION {
        r.push(
            join(prefix, "version"),
            format!(
                "version {}, attendu {FORMAT_VERSION} (version du fichier qui le contient)",
                look.version
            ),
        );
    }
    r.text(join(prefix, "name"), &look.name, 1, MAX_LOOK_NAME);
    check_layers(r, &join(prefix, "layers"), &look.layers);
    r.param(join(prefix, "hue_shift_deg"), look.hue_shift_deg, ParamPath::HueShift);
    check_gates(r, &join(prefix, "gates"), &look.gates);
    check_macros(r, &join(prefix, "macros"), &look.macros);
}

fn check_layers(r: &mut Report, p: &str, layers: &LayerMix) {
    r.param(join(p, "background"), layers.background, ParamPath::LayerBackground);
    r.param(join(p, "world"), layers.world, ParamPath::LayerWorld);
    r.param(join(p, "foreground"), layers.foreground, ParamPath::LayerForeground);
    r.param(join(p, "overlay"), layers.overlay, ParamPath::LayerOverlay);
}

fn check_gates(r: &mut Report, p: &str, g: &ShowGates) {
    r.int_in(join(p, "max_floor"), g.max_floor, 0..=MAX_FLOOR);
    r.param(join(p, "flight_rate"), g.flight_rate, ParamPath::FlightRate);
    r.unique(&join(p, "stunt_tiers"), g.stunt_tiers.iter());
    r.unique(&join(p, "stunt_kinds"), g.stunt_kinds.iter());
    r.param(join(p, "stunt_gap_scale"), g.stunt_gap_scale, ParamPath::StuntGapScale);
    r.unique(&join(p, "shot_tiers"), g.shot_tiers.iter());
    r.unique(&join(p, "shot_block"), g.shot_block.iter());
    check_cut_grid(r, &join(p, "cut_grid"), g.cut_grid);
    r.unique(&join(p, "events"), g.events.iter());
    r.param(join(p, "event_rate"), g.event_rate, ParamPath::EventRate);
    r.param(join(p, "meteor_rate"), g.meteor_rate, ParamPath::MeteorRate);
    r.param(join(p, "cruise"), g.cruise, ParamPath::Cruise);
    r.param(join(p, "ground_traffic"), g.ground_traffic, ParamPath::GroundTraffic);
    check_fx(r, &join(p, "fx"), g.fx);
}

fn check_cut_grid(r: &mut Report, p: &str, grid: CutGrid) {
    r.int_in(join(p, "calm"), grid.calm, 1..=MAX_CUT_BEATS);
    r.int_in(join(p, "groove"), grid.groove, 1..=MAX_CUT_BEATS);
    r.int_in(join(p, "peak"), grid.peak, 1..=MAX_CUT_BEATS);
}

fn check_fx(r: &mut Report, p: &str, fx: FxGates) {
    r.param(join(p, "rgb_split"), fx.rgb_split, ParamPath::FxRgbSplit);
    r.param(join(p, "flash"), fx.flash, ParamPath::FxFlash);
    r.param(join(p, "city_pulse"), fx.city_pulse, ParamPath::FxCityPulse);
}

fn check_macros(r: &mut Report, p: &str, macros: &[Macro]) {
    r.count(p.to_owned(), macros.len(), 0, MAX_MACROS);
    r.unique(p, macros.iter().map(|m| &m.name));
    for (i, m) in macros.iter().enumerate() {
        let mp = index(p, i);
        r.text(join(&mp, "name"), &m.name, 1, MAX_MACRO_NAME);
        let dp = join(&mp, "drives");
        r.count(dp.clone(), m.drives.len(), 1, MAX_DRIVES);
        r.unique(&dp, m.drives.iter().map(|d| &d.param));
        for (j, drive) in m.drives.iter().enumerate() {
            let range = drive.param.range();
            // Chemins `[1]` et `[2]` : 2ᵉ et 3ᵉ éléments du triplet `("chemin", potard à 0, potard à 1)`.
            for (k, label, value) in [(1, "potard à 0", drive.at_zero), (2, "potard à 1", drive.at_one)] {
                if !value.is_finite() || !range.contains(&value) {
                    r.push(
                        format!("{}[{k}]", index(&dp, j)),
                        format!(
                            "{label} : {value} hors de la plage de {} [{}, {}]",
                            drive.param,
                            range.start(),
                            range.end()
                        ),
                    );
                }
            }
        }
    }
}

/// Vérifie les surcharges d'un cue : chaque champ présent suit la même règle que dans un look.
pub(crate) fn check_overrides(r: &mut Report, p: &str, o: &LookOverrides) {
    if let Some(layers) = &o.layers {
        check_layers(r, &join(p, "layers"), layers);
    }
    if let Some(hue) = o.hue_shift_deg {
        r.param(join(p, "hue_shift_deg"), hue, ParamPath::HueShift);
    }
    let g = &o.gates;
    let gp = join(p, "gates");
    let numbers = [
        ("flight_rate", g.flight_rate, ParamPath::FlightRate),
        ("stunt_gap_scale", g.stunt_gap_scale, ParamPath::StuntGapScale),
        ("event_rate", g.event_rate, ParamPath::EventRate),
        ("meteor_rate", g.meteor_rate, ParamPath::MeteorRate),
        ("cruise", g.cruise, ParamPath::Cruise),
        ("ground_traffic", g.ground_traffic, ParamPath::GroundTraffic),
        ("fx.rgb_split", g.fx.rgb_split, ParamPath::FxRgbSplit),
        ("fx.flash", g.fx.flash, ParamPath::FxFlash),
        ("fx.city_pulse", g.fx.city_pulse, ParamPath::FxCityPulse),
    ];
    for (field, value, param) in numbers {
        if let Some(value) = value {
            r.param(join(&gp, field), value, param);
        }
    }
    if let Some(floor) = g.max_floor {
        r.int_in(join(&gp, "max_floor"), floor, 0..=MAX_FLOOR);
    }
    if let Some(grid) = g.cut_grid {
        check_cut_grid(r, &join(&gp, "cut_grid"), grid);
    }
    if let Some(list) = &g.stunt_tiers {
        r.unique(&join(&gp, "stunt_tiers"), list.iter());
    }
    if let Some(list) = &g.stunt_kinds {
        r.unique(&join(&gp, "stunt_kinds"), list.iter());
    }
    if let Some(list) = &g.shot_tiers {
        r.unique(&join(&gp, "shot_tiers"), list.iter());
    }
    if let Some(list) = &g.shot_block {
        r.unique(&join(&gp, "shot_block"), list.iter());
    }
    if let Some(list) = &g.events {
        r.unique(&join(&gp, "events"), list.iter());
    }
}
