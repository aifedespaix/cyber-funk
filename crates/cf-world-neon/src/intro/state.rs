//! La machine à états de la pré-intro (`show/pre-intro.ts`) : sans Bevy, elle lit le niveau du spectacle, la musique et
//! rend ce que les systèmes du monde doivent montrer cette image.

use super::timeline::{IntroStages, advance, speed_cap, stages_at};

/// Largeur du fondu du front des fenêtres allumées (m).
pub const FRONT_WIDTH_M: f32 = 40.0;

/// Avance du front à chaque kick (m, avant le facteur d'étape).
const KICK_SURGE_M: f32 = 30.0;

/// Retombée de l'à-coup des kicks (1/s) : `exp(-2 dt)` de Sillage.
const SURGE_DECAY: f32 = 2.0;

/// Rattrapage de la cible de l'à-coup (1/s). Sillage ajoutait l'à-coup d'un seul pas, jusqu'à 42 m d'un coup : un front
/// qui saute d'une demi-largeur de fondu à chaque kick. Ici il y glisse en ~0,1 s, pour que les fenêtres ne claquent pas
/// au rythme des kicks (règle 7 de `CLAUDE.md`).
const SURGE_FOLLOW: f32 = 12.0;

/// Portée maximale du front (m) et vitesse à laquelle il la rejoint (1/s).
const REACH_M: f32 = 950.0;
const REACH_FOLLOW: f32 = 0.9;

/// Ce que la pré-intro lit à chaque image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IntroInput {
    /// Niveau d'intensité du spectacle en cours (0 à 5), ou `None` sans spectacle.
    pub level: Option<u8>,
    /// Durée de l'image (s).
    pub dt_s: f32,
    /// Kick de l'image.
    pub kick: bool,
    /// Énergie de la musique, 0 à 1 : elle accélère la construction.
    pub energy: f32,
}

/// Le front des fenêtres allumées, devant la voiture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowFront {
    /// Distance du front devant la voiture (m, positive vers l'avant).
    pub ahead_m: f32,
    /// Demi-largeur de son fondu (m).
    pub edge_m: f32,
}

/// Ce que la pré-intro demande au monde cette image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IntroOutput {
    /// Les étapes (toutes à 1 quand la pré-intro est finie ou absente).
    pub stages: IntroStages,
    /// La pré-intro s'occupe du monde (sinon : aucun effet, tout à 1).
    pub running: bool,
    /// Plafond de vitesse (m/s), tant que le niveau vaut 0.
    pub speed_cap_mps: Option<f32>,
    /// Front des fenêtres, tant que les fenêtres ne sont pas toutes allumées.
    pub front: Option<WindowFront>,
}

impl IntroOutput {
    /// Aucun effet.
    pub const NEUTRAL: Self = Self {
        stages: IntroStages::DONE,
        running: false,
        speed_cap_mps: None,
        front: None,
    };

    /// Part des lettres de l'enseigne à allumer, ou `None` quand la pré-intro ne s'en occupe pas.
    #[must_use]
    pub fn sign_progress(&self) -> Option<f32> {
        self.running.then_some(self.stages.sign)
    }
}

/// La pré-intro (niveau 0) : le monde se construit avant le départ.
///
/// Une progression `t` avance avec le temps, et plus vite quand la musique monte. Quand le spectacle quitte le niveau 0,
/// ce qui manque s'achève en quelques secondes. Sans spectacle, ou s'il démarre à un niveau ≥ 1, tout reste à 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreIntro {
    t: f32,
    running: bool,
    finishing: bool,
    reach_m: f32,
    surge_target_m: f32,
    surge_m: f32,
}

impl Default for PreIntro {
    fn default() -> Self {
        Self::new()
    }
}

impl PreIntro {
    /// Pré-intro au repos : tout en place.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            t: 1.0,
            running: false,
            finishing: false,
            reach_m: 0.0,
            surge_target_m: 0.0,
            surge_m: 0.0,
        }
    }

    /// Avance d'une image.
    pub fn update(&mut self, input: &IntroInput) -> IntroOutput {
        let Some(level) = input.level else {
            *self = Self::new();
            return IntroOutput::NEUTRAL;
        };
        if level == 0 {
            if !self.running || self.finishing {
                *self = Self {
                    t: 0.0,
                    running: true,
                    ..Self::new()
                };
            }
        } else if self.running {
            self.finishing = true;
        }
        if !self.running {
            return IntroOutput::NEUTRAL;
        }
        self.t = advance(self.t, input.dt_s, input.energy, self.finishing);
        let stages = stages_at(self.t);
        if self.finishing && self.t >= 1.0 {
            *self = Self::new();
            return IntroOutput::NEUTRAL;
        }
        IntroOutput {
            stages,
            running: true,
            speed_cap_mps: (!self.finishing).then(|| speed_cap(stages.drive)),
            front: self.front(&stages, input),
        }
    }

    /// Le front avance, par à-coups sur les kicks : derrière lui tout est allumé, devant tout est éteint.
    fn front(&mut self, stages: &IntroStages, input: &IntroInput) -> Option<WindowFront> {
        if stages.windows >= 1.0 {
            self.reach_m = 0.0;
            self.surge_target_m = 0.0;
            self.surge_m = 0.0;
            return None;
        }
        let dt = input.dt_s;
        if input.kick {
            self.surge_target_m += KICK_SURGE_M * (0.4 + stages.windows);
        }
        self.surge_target_m *= (-dt * SURGE_DECAY).exp();
        self.surge_m += (self.surge_target_m - self.surge_m) * (1.0 - (-dt * SURGE_FOLLOW).exp());
        self.reach_m += (stages.windows * REACH_M - self.reach_m) * (1.0 - (-dt * REACH_FOLLOW).exp());
        Some(WindowFront {
            ahead_m: self.reach_m + self.surge_m,
            edge_m: FRONT_WIDTH_M,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn input(level: Option<u8>) -> IntroInput {
        IntroInput {
            level,
            dt_s: DT,
            kick: false,
            energy: 0.0,
        }
    }

    #[test]
    fn without_a_show_nothing_happens() {
        let mut intro = PreIntro::new();
        assert_eq!(intro.update(&input(None)), IntroOutput::NEUTRAL);
    }

    #[test]
    fn a_show_starting_above_level_zero_never_runs() {
        let mut intro = PreIntro::new();
        for _ in 0..600 {
            assert_eq!(intro.update(&input(Some(2))), IntroOutput::NEUTRAL);
        }
    }

    #[test]
    fn level_zero_builds_the_world_from_nothing() {
        let mut intro = PreIntro::new();
        let first = intro.update(&input(Some(0)));
        assert!(first.running && first.stages.build < 0.01 && first.stages.sky < 0.01);
        assert!(
            first.speed_cap_mps.is_some_and(|c| c < 4.0),
            "un filet de vitesse au départ"
        );
        assert!(first.sign_progress().is_some());
        // 20 s plus tard : la route est tracée, les immeubles montent, le ciel est encore noir.
        let mut mid = first;
        for _ in 0..(20.0 / DT) as u32 {
            mid = intro.update(&input(Some(0)));
        }
        assert!((mid.stages.road - 1.0).abs() < f32::EPSILON);
        assert!(mid.stages.build > 0.5 && mid.stages.sky < 0.3);
        assert!(mid.front.is_some());
    }

    #[test]
    fn leaving_level_zero_finishes_in_a_few_seconds_then_goes_neutral() {
        let mut intro = PreIntro::new();
        for _ in 0..(5.0 / DT) as u32 {
            intro.update(&input(Some(0)));
        }
        let leaving = intro.update(&input(Some(1)));
        assert!(
            leaving.running && leaving.speed_cap_mps.is_none(),
            "plus de plafond de vitesse"
        );
        let mut frames = 0;
        let mut last = leaving;
        while last.running {
            last = intro.update(&input(Some(1)));
            frames += 1;
            assert!(frames < 600, "la fin dure trop longtemps");
        }
        assert!(f32::from(u16::try_from(frames).unwrap_or(u16::MAX)) * DT < 3.0);
        assert_eq!(last, IntroOutput::NEUTRAL);
    }

    #[test]
    fn coming_back_to_level_zero_restarts_the_intro() {
        let mut intro = PreIntro::new();
        for _ in 0..(10.0 / DT) as u32 {
            intro.update(&input(Some(0)));
        }
        intro.update(&input(Some(2)));
        let restarted = intro.update(&input(Some(0)));
        assert!(restarted.running && restarted.stages.build < 0.01);
    }

    #[test]
    fn a_kick_pushes_the_window_front_forward_without_a_jump() {
        let mut intro = PreIntro::new();
        let mut out = intro.update(&input(Some(0)));
        // On amène les fenêtres à mi-course.
        while out.stages.windows < 0.5 {
            out = intro.update(&input(Some(0)));
        }
        let before = out.front.expect("front").ahead_m;
        let kicked = intro.update(&IntroInput {
            kick: true,
            ..input(Some(0))
        });
        let step = kicked.front.expect("front").ahead_m - before;
        assert!(step > 0.0, "le kick avance le front");
        assert!(step < 10.0, "sans sauter de plusieurs mètres en une image : {step}");
    }

    #[test]
    fn the_front_disappears_once_every_window_is_lit() {
        let mut intro = PreIntro::new();
        let mut energetic = input(Some(0));
        energetic.energy = 1.0;
        let mut out = intro.update(&energetic);
        let mut frames = 0;
        while out.stages.windows < 1.0 {
            out = intro.update(&energetic);
            frames += 1;
            assert!(frames < 60 * 60, "n'arrive jamais");
        }
        assert!(out.front.is_none());
    }
}
