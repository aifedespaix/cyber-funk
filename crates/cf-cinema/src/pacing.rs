//! Rythme de coupe (`cinema/pacing.ts`). Hors spectacle : le rythme de clip historique (durées étirées par
//! [`LEGACY_PACE`], coupes sur la phrase). Pendant le spectacle : la grille `gates.cut_grid` (en temps musicaux, selon
//! la section du morceau) règle à la fois l'étirement des durées et les temps où une coupe peut tomber.

use crate::shot::ShotSpec;
use crate::world::CinemaWorld;

/// Étirement des durées hors spectacle.
pub const LEGACY_PACE: f32 = 1.9;
/// Aucun plan non écrit ne dure moins que cela (s) : le public doit avoir le temps de le lire.
const FLOOR_SECONDS: f32 = 2.5;
/// Respiration : on laisse les plans durer plus longtemps.
const BREATH_STRETCH: f32 = 1.25;
/// Le tour de l'habitacle reste court : sombre et serré, il fatigue sur un vidéoprojecteur.
const SHORT_SHOTS: &[(&str, f32)] = &[("cockpit", 0.55)];
/// Grille de la respiration (temps).
pub const BREATH_GRID: u32 = 16;

/// Les repères d'une image, ramenés à quelques booléens.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CueFlags {
    /// Le drop part.
    pub drop: bool,
    /// Début de phrase.
    pub phrase: bool,
    /// Début de mesure.
    pub bar: bool,
    /// Un temps.
    pub beat: bool,
}

/// (grille en temps, étirement) : pas de coupe en deux temps pour un plan de dix secondes, ni de plan fixe au sommet.
const STEPS: [(f32, f32); 4] = [(2.0, 0.65), (4.0, 0.95), (8.0, 1.35), (16.0, LEGACY_PACE)];

/// Étirement des durées pour une grille de `grid` temps (interpolé entre les paliers, en échelle logarithmique).
#[must_use]
pub fn pace_for_grid(grid: u32) -> f32 {
    let grid = grid as f32;
    let (first, last) = (STEPS[0], STEPS[STEPS.len() - 1]);
    if grid <= first.0 {
        return first.1;
    }
    if grid >= last.0 {
        return last.1;
    }
    for pair in STEPS.windows(2) {
        let [(g0, p0), (g1, p1)] = [pair[0], pair[1]];
        if grid > g1 {
            continue;
        }
        return p0 + (p1 - p0) * ((grid / g0).log2() / (g1 / g0).log2());
    }
    last.1
}

/// Temps musicaux entre deux coupes : la grille de la section en spectacle (16 en respiration), 0 hors spectacle.
#[must_use]
pub fn grid_beats(world: &CinemaWorld) -> u32 {
    let show = &world.show;
    if !show.active {
        return 0;
    }
    if show.breathing {
        return BREATH_GRID;
    }
    u32::from(show.grid_for(world.section)).max(1)
}

/// Durée minimale d'un plan (s) dans le rythme courant ; les plans écrits gardent leurs durées.
#[must_use]
pub fn min_seconds_of(world: &CinemaWorld, spec: &dyn ShotSpec) -> f32 {
    if spec.scripted() {
        spec.min_seconds()
    } else {
        FLOOR_SECONDS.max(spec.min_seconds() * stretch(world, spec))
    }
}

/// Durée maximale d'un plan (s) dans le rythme courant.
#[must_use]
pub fn max_seconds_of(world: &CinemaWorld, spec: &dyn ShotSpec) -> f32 {
    if spec.scripted() {
        spec.max_seconds()
    } else {
        (FLOOR_SECONDS + 1.0).max(spec.max_seconds() * stretch(world, spec))
    }
}

fn stretch(world: &CinemaWorld, spec: &dyn ShotSpec) -> f32 {
    let grid = grid_beats(world);
    if grid == 0 {
        return LEGACY_PACE;
    }
    let breath = if world.show.breathing { BREATH_STRETCH } else { 1.0 };
    let short = SHORT_SHOTS
        .iter()
        .find(|(id, _)| *id == spec.id())
        .map_or(1.0, |(_, k)| *k);
    pace_for_grid(grid) * breath * short
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paliers_de_la_grille() {
        assert!((pace_for_grid(1) - 0.65).abs() < 1e-6);
        assert!((pace_for_grid(4) - 0.95).abs() < 1e-6);
        assert!((pace_for_grid(8) - 1.35).abs() < 1e-6);
        assert!((pace_for_grid(32) - LEGACY_PACE).abs() < 1e-6);
        // 12 temps : à mi-chemin (log) entre 8 et 16 ⇒ log2(1,5) ≈ 0,585 de la marche.
        let expected = 1.35 + (LEGACY_PACE - 1.35) * 1.5_f32.log2();
        assert!((pace_for_grid(12) - expected).abs() < 1e-5);
    }

    #[test]
    fn grille_selon_le_spectacle() {
        let mut world = CinemaWorld::default();
        assert_eq!(grid_beats(&world), 0);
        world.show.active = true;
        world.section = crate::types::Section::Peak;
        assert_eq!(grid_beats(&world), 4);
        world.show.breathing = true;
        assert_eq!(grid_beats(&world), 16);
    }
}
