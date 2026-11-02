//! La couleur d'un nouveau quartier ne part jamais en plein drop ni en Peak (anti-objectif de la spec, règle 6) : la
//! décision est prise en Calm, mais la voiture n'atteint la zone de dégradé que ~20 à 60 s plus tard. Arrivée dans la
//! zone, la colle consulte `Governor::music_blocks` et retient le départ de l'étalonnage tant que la musique l'interdit,
//! au plus `MUSIC_HOLD_MAX_S`. Un forçage de debug n'est jamais retenu.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use cf_analysis::Section;
use cf_render::{DistrictGradeState, DistrictOrder, DistrictState};
use cf_show::district::{DistrictId, DistrictKind, Governor, StayReason, TransitionKind};
use cf_world_neon::districts::{Change, DistrictDriver, DistrictInput, MUSIC_HOLD_MAX_S};

const DT: f32 = 0.1;
/// La voiture avance de 6 m par image (60 m/s).
const STEP_M: f64 = 6.0;
const START_Z: f64 = -400_000.0;

#[derive(Clone, Copy)]
struct Music {
    section: Section,
    drop_imminent: bool,
}

const CALM: Music = Music {
    section: Section::Calm,
    drop_imminent: false,
};
const PEAK: Music = Music {
    section: Section::Peak,
    drop_imminent: false,
};
const BUILD_UP: Music = Music {
    section: Section::Groove,
    drop_imminent: true,
};

struct Rig {
    governor: Governor,
    driver: DistrictDriver,
    grade: DistrictGradeState,
    state: DistrictState,
    frame: u32,
}

impl Rig {
    fn new() -> Self {
        Self {
            governor: Governor::new(7, [6, 6, 6]),
            driver: DistrictDriver::default(),
            grade: DistrictGradeState::default(),
            state: DistrictState::default(),
            frame: 0,
        }
    }

    fn car_z(&self) -> f64 {
        START_Z - f64::from(self.frame) * STEP_M
    }

    fn car_row(&self) -> i64 {
        (-self.car_z() / 26.0).floor() as i64
    }

    fn input(&self, music: Music) -> DistrictInput {
        DistrictInput {
            now_s: f64::from(self.frame) * f64::from(DT),
            dt_s: DT,
            section: music.section,
            section_changed: false,
            intensity: 0.3,
            bass: 0.4,
            high: 0.3,
            bpm: Some(120.0),
            drop_imminent: music.drop_imminent,
            drop_now: false,
            show_present: true,
            active_events: None,
            car_z: Some(self.car_z()),
        }
    }

    fn tick(&mut self, music: Music) -> Option<Change> {
        self.frame += 1;
        let input = self.input(music);
        self.driver
            .tick(&mut self.governor, &input, Some(&mut self.grade), &mut self.state)
    }

    fn colour_started(&self) -> bool {
        self.grade.is_pending() || self.grade.is_fading()
    }

    /// Calm jusqu'à la décision du gouverneur ; le segment est posé 5 rangées devant la voiture (~2 s d'approche).
    fn decide_in_calm(&mut self) {
        let change = (0..3000)
            .find_map(|_| self.tick(CALM))
            .expect("aucun changement en Calm");
        assert!(!change.forced);
        let from = self.car_row() + 5;
        self.driver.begin(&change, Some(from), &mut self.state);
    }

    /// Joue `seconds` de `music` ; rend vrai si la couleur est partie.
    fn play(&mut self, music: Music, seconds: f32) -> bool {
        for _ in 0..(seconds / DT) as u32 {
            self.tick(music);
            if self.colour_started() {
                return true;
            }
        }
        false
    }
}

#[test]
fn drop_imminent_a_l_entree_retient_la_couleur() {
    let mut rig = Rig::new();
    rig.decide_in_calm();
    // La montée commence pendant l'approche : la voiture entre dans la zone en plein drop imminent.
    assert!(!rig.play(BUILD_UP, 20.0), "la couleur part pendant la montée");
    assert!(rig.state.target.is_some());
    assert_eq!(rig.state.last_reason, StayReason::DropImminent);
    assert!(rig.state.progress.abs() < f32::EPSILON);
    // Le drop plan disparaît : la couleur part à l'image suivante.
    assert!(rig.play(CALM, DT), "la couleur ne part pas une fois le drop passé");
}

#[test]
fn peak_a_l_entree_retient_puis_libere_en_calm() {
    let mut rig = Rig::new();
    rig.decide_in_calm();
    assert!(!rig.play(PEAK, 30.0), "la couleur part en Peak");
    assert_eq!(rig.state.last_reason, StayReason::PeakSection);
    assert!(rig.play(CALM, DT));
    // La transition se termine ensuite normalement.
    for _ in 0..600 {
        rig.grade.tick_pending(DT, true, 0);
        rig.grade.update(DT);
        rig.tick(CALM);
        if rig.state.target.is_none() {
            break;
        }
    }
    assert_eq!(rig.state.target, None);
}

#[test]
fn la_retenue_est_bornee() {
    let mut rig = Rig::new();
    rig.decide_in_calm();
    // Approche (~2 s) puis Peak sans fin : la couleur part après MUSIC_HOLD_MAX_S passées dans la zone, pas avant.
    let approach = 3.0;
    assert!(!rig.play(PEAK, approach + MUSIC_HOLD_MAX_S - 2.0));
    assert!(rig.play(PEAK, 3.0), "la retenue ne se relâche jamais");
}

#[test]
fn un_forcage_n_est_jamais_retenu() {
    let mut rig = Rig::new();
    for _ in 0..30 {
        rig.tick(BUILD_UP);
    }
    let to = DistrictId {
        kind: DistrictKind::Suburb,
        variant: 4,
    };
    let order = DistrictOrder::Force {
        to,
        transition: TransitionKind::Gradient,
    };
    let now = f64::from(rig.frame) * f64::from(DT);
    let change = rig
        .driver
        .apply_order(&mut rig.governor, order, now, &mut rig.state)
        .unwrap();
    assert!(change.forced);
    let from = rig.car_row() + 5;
    rig.driver.begin(&change, Some(from), &mut rig.state);
    // En Peak et en pleine montée : la couleur part dès l'entrée dans la zone (~2 s).
    let blocked = Music {
        section: Section::Peak,
        drop_imminent: true,
    };
    assert!(rig.play(blocked, 4.0), "un forçage retenu par la musique");
}
