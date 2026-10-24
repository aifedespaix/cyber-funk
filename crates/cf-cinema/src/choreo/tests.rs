//! La mise en scène : préparation et frappe du drop, cascades, salves, ordres de l'opérateur.

use std::sync::Arc;

use bevy::math::DVec3;
use cf_show::director::Order;
use cf_show::drop::{DropFxPackage, DropPlan};
use cf_show::model::{ShotId, StuntKind};

use super::*;
use crate::editor::testkit::{TestShow, sillage_catalog};
use crate::types::{Section, Stunt, TransitionKind};
use crate::world::{CinemaWorld, NoObstacles};

struct Stage {
    catalog: Vec<Box<dyn ShotSpec>>,
    editor: Editor,
    choreo: Choreography,
    show: TestShow,
    world: CinemaWorld,
}

fn plan(shot: Option<&str>, stunt: Option<&str>, forced: bool) -> DropPlan {
    DropPlan {
        stunt: stunt.and_then(|s| StuntKind::new(s).ok()),
        shot: shot.and_then(|s| ShotId::new(s).ok()),
        fx: DropFxPackage {
            flash: 0.0,
            rgb_split: 0.0,
            city_pulse: 0.5,
            shockwaves: 1,
            burst: 20,
        },
        title: None::<Arc<str>>,
        forced,
    }
}

impl Stage {
    fn new(seed: u32) -> Self {
        let catalog = sillage_catalog();
        let mut show = TestShow::default();
        let mut editor = Editor::new(&catalog, false, CinemaRng::new(seed));
        editor.reset(0.0, &mut show);
        let mut choreo = Choreography::new(CinemaRng::new(seed ^ 0x55));
        choreo.reset(&show);
        let mut world = CinemaWorld::default();
        world.show.active = true;
        world.time = 100.0;
        world.intensity = 0.6;
        world.section = Section::Groove;
        world.beat.bpm = Some(128.0);
        Self {
            catalog,
            editor,
            choreo,
            show,
            world,
        }
    }

    fn index(&self, id: &str) -> usize {
        self.editor.find(id).unwrap_or(usize::MAX)
    }

    fn decide(&mut self, flags: CueFlags, elapsed: f64, spec: Option<&str>) -> Option<Call> {
        let current = spec.map(|id| self.index(id));
        let frame = CinemaFrame::new(&self.world, &NoObstacles, &[]);
        self.choreo.decide(
            &frame,
            flags,
            elapsed,
            current,
            &mut self.editor,
            &self.catalog,
            &mut self.show,
        )
    }

    fn drop_hit(&mut self, elapsed: f64, spec: Option<&str>) -> Option<Call> {
        self.world.show.drop_now = true;
        let call = self.decide(CueFlags::default(), elapsed, spec);
        self.world.show.drop_now = false;
        call
    }
}

fn decided(call: Option<Call>) -> Option<Decision> {
    match call {
        Some(Call::Decide(d)) => Some(d),
        _ => None,
    }
}

const BEAT: CueFlags = CueFlags {
    drop: false,
    phrase: false,
    bar: false,
    beat: true,
};
const BAR: CueFlags = CueFlags {
    drop: false,
    phrase: false,
    bar: true,
    beat: true,
};

#[test]
fn le_drop_coupe_franchement_vers_le_plan_prevu() {
    let mut s = Stage::new(1);
    s.show.plan = Some(plan(Some("side-track"), None, false));
    let call = s.drop_hit(5.0, Some("chase"));
    let d = decided(call).expect("une décision");
    assert_eq!(d.spec, Some(s.index("side-track")));
    assert_eq!(d.kind, Some(TransitionKind::Cut));
    assert_eq!(d.mode, PickMode::Drop);
    assert!(d.counted, "le chef du spectacle a déjà compté ce plan");
}

#[test]
fn le_drop_attend_un_plan_neuf_et_respecte_la_respiration() {
    let mut s = Stage::new(1);
    s.show.plan = Some(plan(Some("side-track"), None, false));
    assert_eq!(s.drop_hit(HIT_GUARD - 0.1, Some("chase")), None);
    s.world.show.breathing = true;
    assert_eq!(s.drop_hit(5.0, Some("chase")), None);
}

#[test]
fn le_drop_sur_le_plan_prevu_ne_recoupe_pas() {
    let mut s = Stage::new(1);
    s.show.plan = Some(plan(Some("side-track"), None, false));
    assert_eq!(s.drop_hit(5.0, Some("side-track")), None);
}

#[test]
fn un_plan_de_drop_strobe_est_refuse_quand_les_flashs_sont_coupes() {
    let mut s = Stage::new(1);
    s.world.show.flashes = false;
    s.show.plan = Some(plan(Some("strobe"), None, false));
    let strobe = s.index("strobe");
    for _ in 0..200 {
        s.world.time += 400.0;
        let call = s.drop_hit(5.0, Some("chase"));
        let d = decided(call).expect("repli sur un plan énergique");
        assert_ne!(d.spec, Some(strobe));
        assert_eq!(d.kind, Some(TransitionKind::Cut));
    }
    // Flashs permis : le plan de drop `strobe` part bien.
    s.world.show.flashes = true;
    s.world.time += 400.0;
    let call = s.drop_hit(5.0, Some("chase"));
    assert_eq!(decided(call).and_then(|d| d.spec), Some(strobe));
}

#[test]
fn la_cascade_du_drop_attend_son_plan() {
    let mut s = Stage::new(2);
    s.show.plan = Some(plan(None, Some("big-jump"), false));
    // La cascade n'a pas commencé : `stunt-cam` n'est pas jouable, on le garde en attente.
    assert_eq!(s.drop_hit(5.0, Some("chase")), None);
    s.world.time += 1.0;
    s.world.stunt = Some(Stunt {
        kind: "big-jump",
        progress: 0.1,
        focus: DVec3::ZERO,
    });
    let d = decided(s.decide(CueFlags::default(), 6.0, Some("chase"))).expect("plan de cascade");
    assert_eq!(d.spec, Some(s.index("stunt-cam")));
    assert_eq!(d.kind, Some(TransitionKind::Cut));
    // L'attente expire au bout de 2,5 s.
    let mut s = Stage::new(2);
    s.show.plan = Some(plan(None, Some("big-jump"), false));
    assert_eq!(s.drop_hit(5.0, Some("chase")), None);
    s.world.time += 3.0;
    s.world.stunt = Some(Stunt {
        kind: "big-jump",
        progress: 0.1,
        focus: DVec3::ZERO,
    });
    assert_eq!(s.decide(CueFlags::default(), 6.0, Some("chase")), None);
}

#[test]
fn l_ascension_a_sa_propre_camera() {
    let mut s = Stage::new(2);
    s.show.plan = Some(plan(None, Some("ascension"), false));
    s.world.stunt = Some(Stunt {
        kind: "ascension",
        progress: 0.0,
        focus: DVec3::ZERO,
    });
    let d = decided(s.drop_hit(5.0, Some("chase"))).expect("décision");
    assert_eq!(d.spec, Some(s.index("ascension-cam")));
}

#[test]
fn la_montee_prepare_un_plan_qui_pousse_et_le_laisse_pousser() {
    let mut s = Stage::new(3);
    s.show.plan = Some(plan(None, None, false));
    // Trop tôt : le plan est neuf.
    assert_eq!(s.decide(BEAT, PREP_GUARD - 0.1, Some("chase")), None);
    let d = decided(s.decide(BEAT, PREP_GUARD + 0.1, Some("chase"))).expect("plan de préparation");
    let id = s.catalog[d.spec.expect("un plan")].id();
    assert!(PUSH_IDS.contains(&id), "{id}");
    assert_eq!(d.mode, PickMode::Normal);
    // Une seule préparation par montée.
    assert_eq!(s.decide(BEAT, 5.0, Some("chase")), None);
    // Sur un plan qui pousse, la routine se tait jusqu'à `PREP_HOLD`, puis reprend la main.
    assert_eq!(s.decide(BEAT, 10.0, Some(id)), Some(Call::Wait));
    assert_eq!(s.decide(BEAT, PREP_HOLD + 1.0, Some(id)), None);
}

#[test]
fn un_drop_prevu_avec_plan_ou_cascade_exclut_le_travelling_compense() {
    for seed in 0..60 {
        let mut s = Stage::new(seed);
        s.show.plan = Some(plan(Some("chase"), None, false));
        let d = decided(s.decide(BAR, PREP_GUARD + 0.5, Some("orbit"))).expect("préparation");
        assert_ne!(s.catalog[d.spec.expect("un plan")].id(), "dolly-zoom");
    }
}

#[test]
fn pas_de_preparation_en_respiration_ni_pour_un_drop_force() {
    let mut s = Stage::new(3);
    s.show.plan = Some(plan(None, None, true));
    assert_eq!(s.decide(BEAT, 5.0, Some("chase")), None);
    s.show.plan = Some(plan(None, None, false));
    s.world.show.breathing = true;
    assert_eq!(s.decide(BEAT, 5.0, Some("chase")), None);
}

#[test]
fn l_ordre_plan_suivant_coupe_au_prochain_temps() {
    let mut s = Stage::new(4);
    s.show.orders.push(Order::SkipShot);
    assert_eq!(
        s.decide(CueFlags::default(), 3.0, Some("chase")),
        None,
        "pas avant un temps"
    );
    s.world.time += 0.2;
    assert_eq!(
        s.decide(BEAT, 3.2, Some("chase")),
        Some(Call::Decide(Decision::of(PickMode::Normal)))
    );
    assert_eq!(s.decide(BEAT, 3.6, Some("chase")), None, "ordre consommé");
}

#[test]
fn l_ordre_plan_suivant_expire() {
    let mut s = Stage::new(4);
    s.show.orders.push(Order::SkipShot);
    assert_eq!(s.decide(CueFlags::default(), 3.0, Some("chase")), None);
    s.world.time += 3.0;
    assert_eq!(s.decide(BEAT, 6.0, Some("chase")), None);
}

#[test]
fn l_ordre_montre_le_dj_ignore_les_portes_mais_pas_le_changement_de_creneau() {
    let mut s = Stage::new(5);
    s.show.gates.shot_block = vec![ShotId::new("dj-sign").expect("slug")];
    s.show.orders.push(Order::ShowDj);
    let sign = s.index("dj-sign");
    let d = decided(s.decide(BEAT, 3.0, Some("chase"))).expect("enseigne");
    assert_eq!(d.spec, Some(sign));
    // Changement de créneau : l'enseigne vient d'elle-même, mais seulement si les portes la permettent.
    let mut s = Stage::new(5);
    s.show.gates.shot_block = vec![ShotId::new("dj-sign").expect("slug")];
    s.show.slot_seq += 1;
    assert_eq!(s.decide(BEAT, 3.0, Some("chase")), None);
    let mut s = Stage::new(5);
    s.show.slot_seq += 1;
    assert_eq!(
        decided(s.decide(BEAT, 3.0, Some("chase"))).and_then(|d| d.spec),
        Some(sign)
    );
    // Un plan neuf n'est pas coupé pour l'enseigne.
    let mut s = Stage::new(5);
    s.show.slot_seq += 1;
    assert_eq!(s.decide(BEAT, 1.0, Some("chase")), None);
}

/// Des drops à répétition, chaleur au maximum : les salves sont rares (3 à 4 minutes) et de 3 ou 4 coupes.
#[test]
fn les_salves_sont_rares_et_courtes() {
    let mut s = Stage::new(6);
    s.show.heat = 1.0;
    let mut cuts_in_salvo = Vec::new();
    for _ in 0..200 {
        s.world.time += 60.0;
        s.show.plan = Some(plan(Some("chase"), None, false));
        let before = s.choreo.salvo_at.len();
        let _ = s.drop_hit(5.0, Some("orbit"));
        s.show.plan = None;
        if s.choreo.salvo_at.len() > before {
            assert!(s.choreo.in_salvo());
            let mut cuts = 1;
            // Une coupe par mesure (1,875 s à 128 BPM).
            for _ in 0..6 {
                s.world.time += 1.9;
                if let Some(Call::Decide(d)) = s.decide(BAR, 1.9, Some("orbit")) {
                    assert_eq!(d.kind, Some(TransitionKind::Cut));
                    assert_eq!(d.mode, PickMode::Drop);
                    cuts += 1;
                }
            }
            cuts_in_salvo.push(cuts);
            assert!(!s.choreo.in_salvo());
        }
    }
    assert!(
        cuts_in_salvo.len() >= 20,
        "des salves existent : {}",
        cuts_in_salvo.len()
    );
    assert!(cuts_in_salvo.iter().all(|c| (3..=4).contains(c)), "{cuts_in_salvo:?}");
    for pair in s.choreo.salvo_at.windows(2) {
        assert!(pair[1] - pair[0] >= SALVO_GAP, "deux salves à {} s", pair[1] - pair[0]);
    }
}

#[test]
fn pas_de_salve_sans_la_porte() {
    let mut s = Stage::new(6);
    s.show.heat = 1.0;
    s.show.gates.salvo = false;
    for _ in 0..200 {
        s.world.time += 400.0;
        let _ = s.drop_hit(5.0, Some("orbit"));
        assert!(!s.choreo.in_salvo());
    }
    assert_eq!(s.choreo.salvo_at.len(), 0);
}

#[test]
fn la_salve_s_arrete_en_respiration() {
    let mut s = Stage::new(6);
    s.show.heat = 1.0;
    for _ in 0..50 {
        s.world.time += 400.0;
        let _ = s.drop_hit(5.0, Some("orbit"));
        if s.choreo.in_salvo() {
            break;
        }
    }
    assert!(s.choreo.in_salvo());
    s.world.show.breathing = true;
    s.world.time += 1.0;
    let _ = s.decide(BAR, 2.0, Some("orbit"));
    assert!(!s.choreo.in_salvo());
}
