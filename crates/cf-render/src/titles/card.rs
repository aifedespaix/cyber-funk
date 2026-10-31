//! Les cartons-titres (`SillageShowTitle.svelte`) : leur modèle, leurs durées et leurs courbes, sans Bevy.
//!
//! Sillage les animait en CSS ; ici les mêmes courbes sont des fonctions du temps écoulé depuis l'apparition du carton,
//! pour qu'un test puisse les vérifier et que la couche 2D n'ait qu'à les lire.

/// Les trois familles de cartons : le niveau (sobre), le DJ (le plus gros, couleur d'accent), le titre de drop (bref).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TitleKind {
    /// Un niveau d'intensité : fin, très espacé, un filet qui se trace.
    Level,
    /// Le DJ qui prend les platines : le plus gros, à la couleur d'accent.
    Dj,
    /// Un titre de drop ou de cue : court et percutant, il claque puis part.
    Drop,
}

/// Un carton-titre.
#[derive(Debug, Clone, PartialEq)]
pub struct TitleCard {
    /// Change à chaque nouveau carton (rejoue l'animation).
    pub id: u32,
    /// Famille.
    pub kind: TitleKind,
    /// Le titre.
    pub text: String,
    /// Petite ligne au-dessus du titre (« Niveau 3 », « Aux platines »).
    pub sub: Option<String>,
    /// Tempo au moment du carton : l'entrée et la sortie durent un temps. `None` : environ une demi-seconde.
    pub bpm: Option<f32>,
}

/// Un temps de la musique en secondes, borné pour qu'un tempo extrême reste lisible ; 0,5 s sans tempo.
#[must_use]
pub fn beat_seconds(bpm: Option<f32>) -> f32 {
    bpm.filter(|b| b.is_finite() && *b > 0.0)
        .map_or(0.5, |b| (60.0 / b).clamp(0.3, 0.8))
}

/// Durées d'un carton (s) : entrée et sortie d'un temps, plateau plus long pour le DJ, bref pour un drop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timing {
    /// Entrée.
    pub enter_s: f32,
    /// Plateau.
    pub hold_s: f32,
    /// Sortie.
    pub leave_s: f32,
    /// Un temps de la musique : le filet du niveau attend ce délai avant de se tracer.
    pub beat_s: f32,
}

impl Timing {
    /// Les durées du carton `kind` à `bpm`.
    #[must_use]
    pub fn of(kind: TitleKind, bpm: Option<f32>) -> Self {
        let beat = beat_seconds(bpm);
        let drop = kind == TitleKind::Drop;
        Self {
            enter_s: if drop { beat * 0.5 } else { beat },
            leave_s: if drop { beat } else { beat * 1.5 },
            hold_s: match kind {
                TitleKind::Drop => (beat * 3.0).max(1.4),
                TitleKind::Dj => (beat * 10.0).max(4.2),
                TitleKind::Level => (beat * 8.0).max(3.2),
            },
            beat_s: beat,
        }
    }

    /// Durée totale : le carton est à retirer ensuite.
    #[must_use]
    pub fn total_s(&self) -> f32 {
        self.enter_s + self.hold_s + self.leave_s
    }
}

/// Ce qu'on voit d'un carton à un instant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    /// Opacité, 0 à 1.
    pub alpha: f32,
    /// Échelle du texte.
    pub scale: f32,
    /// Part du filet tracée, 0 à 1 (carton de niveau seulement).
    pub rule: f32,
}

/// Décélération (`--ease-out`).
fn ease_out(x: f32) -> f32 {
    1.0 - (1.0 - x).powi(3)
}

/// Accélération (`ease-in`).
fn ease_in(x: f32) -> f32 {
    x * x
}

/// Avancement 0..1 de `t` entre `start` et `start + len`.
fn progress(t: f32, start: f32, len: f32) -> f32 {
    ((t - start) / len.max(1e-3)).clamp(0.0, 1.0)
}

/// L'aspect du carton `kind` à `t` secondes de son apparition. `reduce_motion` : seulement des fondus, sans échelle ni
/// filet qui se trace (`prefers-reduced-motion`).
#[must_use]
pub fn look_at(kind: TitleKind, timing: &Timing, t: f32, reduce_motion: bool) -> Look {
    let enter = progress(t, 0.0, timing.enter_s);
    let leave = progress(t, timing.enter_s + timing.hold_s, timing.leave_s);
    if reduce_motion {
        // Fondus linéaires, filet entier d'emblée.
        return Look {
            alpha: enter * (1.0 - leave),
            scale: 1.0,
            rule: 1.0,
        };
    }
    let (from, to) = if kind == TitleKind::Drop {
        (1.35, 1.0)
    } else {
        (0.94, 1.0)
    };
    let entering = from + (to - from) * ease_out(enter);
    // La sortie grossit un peu en s'effaçant : 1 → 1,05.
    let scale = entering * (1.0 + 0.05 * ease_in(leave));
    Look {
        alpha: ease_out(enter) * (1.0 - ease_in(leave)),
        scale,
        rule: ease_out(progress(t, timing.beat_s, timing.enter_s)),
    }
}

/// Corps du titre et de la ligne du dessus (px) pour une fenêtre de `width_px` : les `clamp(min, vw, max)` de Sillage,
/// avec 1 rem = 16 px.
#[must_use]
pub fn font_sizes(kind: TitleKind, width_px: f32) -> (f32, f32) {
    let vw =
        |percent: f32, min_rem: f32, max_rem: f32| (width_px * percent / 100.0).clamp(min_rem * 16.0, max_rem * 16.0);
    match kind {
        TitleKind::Level => (vw(8.0, 2.2, 7.0), vw(2.4, 1.0, 2.0)),
        TitleKind::Dj => (vw(14.0, 3.2, 13.0), vw(2.8, 1.1, 2.4)),
        TitleKind::Drop => (vw(12.0, 3.0, 11.0), vw(2.4, 1.0, 2.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_beat_is_bounded_and_defaults_to_half_a_second() {
        assert!((beat_seconds(None) - 0.5).abs() < f32::EPSILON);
        assert!((beat_seconds(Some(120.0)) - 0.5).abs() < f32::EPSILON);
        assert!(
            (beat_seconds(Some(300.0)) - 0.3).abs() < f32::EPSILON,
            "tempo extrême borné"
        );
        assert!((beat_seconds(Some(40.0)) - 0.8).abs() < f32::EPSILON);
        assert!((beat_seconds(Some(0.0)) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn the_dj_card_holds_longest_and_the_drop_card_is_brief() {
        let level = Timing::of(TitleKind::Level, Some(120.0));
        let dj = Timing::of(TitleKind::Dj, Some(120.0));
        let drop = Timing::of(TitleKind::Drop, Some(120.0));
        assert!((level.hold_s - 4.0).abs() < 1e-6 && (dj.hold_s - 5.0).abs() < 1e-6);
        assert!((drop.hold_s - 1.5).abs() < 1e-6);
        assert!(drop.enter_s < level.enter_s && drop.total_s() < level.total_s());
        // Plancher du plateau quand le tempo est rapide.
        assert!((Timing::of(TitleKind::Dj, Some(300.0)).hold_s - 4.2).abs() < 1e-6);
    }

    #[test]
    fn a_card_fades_in_holds_and_fades_out() {
        let timing = Timing::of(TitleKind::Level, Some(120.0));
        let start = look_at(TitleKind::Level, &timing, 0.0, false);
        let held = look_at(TitleKind::Level, &timing, timing.enter_s + 1.0, false);
        let gone = look_at(TitleKind::Level, &timing, timing.total_s(), false);
        assert!(start.alpha.abs() < 1e-6 && (start.scale - 0.94).abs() < 1e-6);
        assert!((held.alpha - 1.0).abs() < 1e-6 && (held.scale - 1.0).abs() < 1e-6);
        assert!(gone.alpha.abs() < 1e-6 && (gone.scale - 1.05).abs() < 1e-6);
    }

    #[test]
    fn the_drop_card_punches_in_from_larger() {
        let timing = Timing::of(TitleKind::Drop, None);
        assert!((look_at(TitleKind::Drop, &timing, 0.0, false).scale - 1.35).abs() < 1e-6);
    }

    #[test]
    fn the_level_rule_waits_a_beat_then_draws() {
        let timing = Timing::of(TitleKind::Level, Some(120.0));
        assert!(
            look_at(TitleKind::Level, &timing, timing.beat_s * 0.5, false)
                .rule
                .abs()
                < 1e-6
        );
        assert!((look_at(TitleKind::Level, &timing, timing.beat_s + timing.enter_s, false).rule - 1.0).abs() < 1e-6);
    }

    #[test]
    fn reduced_motion_only_fades() {
        let timing = Timing::of(TitleKind::Drop, None);
        for t in [0.0, 0.1, 1.0, timing.total_s()] {
            let look = look_at(TitleKind::Drop, &timing, t, true);
            assert!((look.scale - 1.0).abs() < f32::EPSILON && (look.rule - 1.0).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn font_sizes_follow_the_clamps_of_sillage() {
        // 1920 px : le DJ plafonne à 13 rem, le niveau à 7 rem.
        assert!((font_sizes(TitleKind::Dj, 1920.0).0 - 208.0).abs() < 1e-3);
        assert!((font_sizes(TitleKind::Level, 1920.0).0 - 112.0).abs() < 1e-3);
        // Petite fenêtre : le plancher tient.
        assert!((font_sizes(TitleKind::Dj, 200.0).0 - 51.2).abs() < 1e-3);
    }
}
