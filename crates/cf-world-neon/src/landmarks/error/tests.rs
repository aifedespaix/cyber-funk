//! Tests de `error`.

#![allow(clippy::panic, clippy::expect_used)]

use super::*;
use crate::landmarks::field::{FieldState, MAX_LIGHTS};
use crate::landmarks::frame::FlashPolicy;

struct Rig {
    building: ErrorBuilding,
    field: FieldState,
    levels: [f32; MAX_LIGHTS],
    place: Placement,
    frame_no: u32,
}

impl Rig {
    fn new() -> Self {
        let place = Placement {
            z: -2000.0,
            side: 1,
            tint: 0,
            scale: 1.0,
            spans_road: false,
        };
        let mut building = ErrorBuilding::new();
        building.show(&place);
        Self {
            building,
            field: FieldState::default(),
            levels: [0.0; MAX_LIGHTS],
            place,
            frame_no: 0,
        }
    }

    fn step(&mut self, policy: FlashPolicy, kick: bool, drop: bool, bass: f32) -> f32 {
        let mut frame = LandmarkFrame::rest();
        frame.policy = policy;
        frame.time = f64::from(self.frame_no) / 60.0;
        self.frame_no += 1;
        frame.car = DVec3::new(0.0, 0.0, self.place.z + 100.0);
        frame.beat.kick = kick;
        frame.beat.bass = bass;
        frame.drive.drop = drop;
        let mut ctx = StepCtx::new(&mut self.field, &self.place, &mut self.levels, frame.dt);
        self.building.step(&frame, &mut ctx);
        self.building.glitch()
    }
}

const LIVE: FlashPolicy = FlashPolicy {
    flashes: true,
    breathing: false,
};

#[test]
fn au_repos_pas_de_dechirure_ni_de_zone() {
    let mut rig = Rig::new();
    for i in 0..600 {
        let g = rig.step(FlashPolicy::NONE, i % 28 == 0, i == 100, 1.0);
        assert!(g.abs() < f32::EPSILON);
    }
    assert!(rig.field.zone().strength.abs() < f32::EPSILON);
}

#[test]
fn la_dechirure_ne_depasse_jamais_les_bornes_du_limiteur() {
    let mut rig = Rig::new();
    let mut last = 0.0_f32;
    let mut peak = 0.0_f32;
    let mut starts = Vec::new();
    for i in 0..1800 {
        // Pire cas : un kick toutes les 0,25 s, des drops fréquents.
        let g = rig.step(LIVE, i % 15 == 0, i % 240 == 120, 1.0);
        assert!(
            (g - last).abs() <= (1.0 / 60.0) / cf_show::limiter::MIN_FADE + 1e-3,
            "saut de {}",
            (g - last).abs()
        );
        if last < 0.5 && g >= 0.5 {
            starts.push(i);
        }
        last = g;
        peak = peak.max(g);
    }
    assert!(peak > 0.5, "la déchirure existe quand c'est permis ({peak})");
    for &s in &starts {
        let in_second = starts.iter().filter(|&&t| t >= s && t < s + 60).count();
        assert!(in_second <= 3, "{in_second} départs en 1 s");
    }
}

#[test]
fn le_mot_change_apres_huit_kicks_et_a_chaque_drop_sans_jamais_se_repeter() {
    let mut rig = Rig::new();
    let first = rig.building.word();
    for _ in 0..WORD_EVERY_KICKS {
        rig.step(FlashPolicy::NONE, true, false, 0.0);
    }
    assert_ne!(rig.building.word(), first);
    let before = rig.building.word();
    rig.step(FlashPolicy::NONE, false, true, 0.0);
    assert_ne!(rig.building.word(), before);
}

#[test]
fn le_mot_suit_la_voiture_dans_les_limites_de_la_route() {
    let mut rig = Rig::new();
    rig.step(LIVE, false, false, 0.0);
    let decal = rig.field.decal();
    assert!(decal.amount > 0.5, "la voiture est dans le champ du panneau");
    assert!((decal.center_z - (rig.place.z + 100.0 - 15.0)).abs() < 1e-9);
    assert!(decal.center_x.abs() <= 4.0);
    assert!(
        (decal.half_width / decal.half_depth - 4.0).abs() < 1e-5,
        "proportions 4:1 de l'atlas"
    );
    assert_eq!(decal.word.w as usize, ERROR_WORDS[rig.building.word()].len());
}

#[test]
fn loin_du_panneau_le_mot_et_le_faisceau_s_eteignent() {
    let mut rig = Rig::new();
    let mut frame = LandmarkFrame::rest();
    frame.car = DVec3::new(0.0, 0.0, rig.place.z + 600.0);
    let mut ctx = StepCtx::new(&mut rig.field, &rig.place, &mut rig.levels, frame.dt);
    rig.building.step(&frame, &mut ctx);
    assert!(rig.field.decal().amount.abs() < f32::EPSILON);
    let mut paint = [PartPaint::default(); 6];
    rig.building.paint(&frame, &mut paint);
    assert!(!paint[5].visible);
}

#[test]
fn le_panneau_a_sept_couches_et_trois_pieces_de_fer() {
    let building = ErrorBuilding::new();
    let parts = building.parts();
    assert_eq!(parts.len(), 6);
    let MeshSpec::Custom(mesh) = &parts[0].mesh else {
        panic!("le panneau est un maillage fusionné");
    };
    assert_eq!(mesh.vertex_count(), LAYERS * 4);
    assert_eq!(mesh.uv1.len(), mesh.vertex_count());
    assert_eq!(parts.iter().filter(|p| p.look == Look::Iron).count(), 3);
    assert_eq!(parts[0].map, Some(MapId::ErrorAtlas));
    assert_eq!(building.lead(), Some(620.0));
}
