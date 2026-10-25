//! Tests de `matrix`.

#![allow(clippy::panic, clippy::expect_used)]

use super::*;
use crate::landmarks::field::{FieldState, MAX_LIGHTS};
use crate::landmarks::frame::{Colors, FlashPolicy};

fn colors() -> Colors {
    // Trois couleurs distinctes suffisent : lime, alarme, menthe.
    let palette = cf_render::ThemeLibrary::builtin()
        .map(|l| *l.preset(cf_show::model::PresetTheme::Velvet))
        .unwrap_or_default();
    Colors::of(&palette)
}

struct Rig {
    building: MatrixBuilding,
    field: FieldState,
    levels: [f32; MAX_LIGHTS],
    place: Placement,
    time: f64,
}

impl Rig {
    fn new(variant: MatrixVariant) -> Self {
        let place = Placement {
            z: -3000.0,
            side: 1,
            tint: 0,
            scale: 1.0,
            spans_road: false,
        };
        let mut building = MatrixBuilding::new(variant);
        building.show(&place);
        Self {
            building,
            field: FieldState::default(),
            levels: [0.0; MAX_LIGHTS],
            place,
            time: 0.0,
        }
    }

    fn step(&mut self, mut frame: LandmarkFrame) {
        self.time += f64::from(frame.dt);
        frame.time = self.time;
        frame.colors = colors();
        let mut ctx = StepCtx::new(&mut self.field, &self.place, &mut self.levels, frame.dt);
        self.building.step(&frame, &mut ctx);
    }
}

fn live() -> LandmarkFrame {
    LandmarkFrame {
        policy: FlashPolicy {
            flashes: true,
            breathing: false,
        },
        ..LandmarkFrame::rest()
    }
}

#[test]
fn trois_pieces_dont_un_maillage_de_soixante_dix_glyphes() {
    let parts = MatrixBuilding::new(MatrixVariant::Rain).parts();
    assert_eq!(parts.len(), 3);
    let MeshSpec::Custom(mesh) = &parts[2].mesh else {
        panic!("les glyphes sont un maillage fusionné");
    };
    assert_eq!(mesh.vertex_count(), SPROUTS * 4);
    assert_eq!(mesh.colors.len(), SPROUTS * 4);
    assert!(
        parts[2].bounds.is_some(),
        "les sommets sont déplacés par le shader : enveloppe imposée"
    );
}

#[test]
fn les_variantes_ont_les_graines_et_les_noms_de_sillage() {
    for (variant, seed, name) in [
        (MatrixVariant::Rain, 131, "matrix"),
        (MatrixVariant::Flux, 137, "flux"),
        (MatrixVariant::Binary, 149, "binary"),
    ] {
        let building = MatrixBuilding::new(variant);
        assert_eq!(building.body().map(|b| b.seed), Some(seed));
        assert_eq!(building.kind().as_str(), name);
    }
}

#[test]
fn un_drop_bascule_la_palette_quand_les_flashs_sont_permis() {
    let mut rig = Rig::new(MatrixVariant::Rain);
    rig.step(live());
    assert_eq!(rig.building.mode(), 0);
    let mut drop = live();
    drop.drive.drop = true;
    rig.step(drop);
    assert!(
        (1..MODES).contains(&rig.building.mode()),
        "mode {}",
        rig.building.mode()
    );
}

#[test]
fn flashs_coupes_le_flux_reste_au_vert_de_repos() {
    let mut rig = Rig::new(MatrixVariant::Rain);
    let mut drop = LandmarkFrame::rest();
    drop.drive.drop = true;
    for _ in 0..5 {
        rig.step(drop);
        rig.step(LandmarkFrame::rest());
    }
    assert_eq!(rig.building.mode(), 0);
}

#[test]
fn jamais_deux_fois_la_meme_palette_et_retour_au_vert_apres_seize_temps() {
    let mut rig = Rig::new(MatrixVariant::Binary);
    rig.step(live());
    let mut seen = Vec::new();
    for _ in 0..30 {
        let mut drop = live();
        drop.drive.drop = true;
        rig.step(drop);
        let mode = rig.building.mode();
        if mode != 0 {
            seen.push(mode);
        }
        // Seize kicks, espacés d'un quart de seconde (la cadence maximale du limiteur).
        for _ in 0..MODE_BEATS {
            let mut kick = live();
            kick.beat.kick = true;
            kick.dt = 0.25;
            rig.step(kick);
        }
        assert_eq!(rig.building.mode(), 0, "retour au vert");
    }
    assert!(seen.len() > 3);
    assert!(seen.windows(2).all(|w| w[0] != w[1]), "{seen:?}");
}

#[test]
fn la_couleur_se_fond_sans_depasser_la_pente_de_min_fade() {
    let mut rig = Rig::new(MatrixVariant::Rain);
    rig.step(live());
    let mut drop = live();
    drop.drive.drop = true;
    rig.step(drop);
    let mut paint = [PartPaint::default(); 3];
    let mut previous = rig.building.tint;
    for _ in 0..60 {
        rig.step(live());
        rig.building.paint(&live(), &mut paint);
        let now = rig.building.tint;
        let step = (now - previous).abs().max_element();
        assert!(
            step <= (1.0 / 60.0) / cf_show::limiter::MIN_FADE + 1e-4,
            "saut de {step}"
        );
        previous = now;
    }
}

#[test]
fn deux_lumieres_au_cinquieme_et_aux_trois_quarts() {
    let mut rig = Rig::new(MatrixVariant::Flux);
    for _ in 0..60 {
        rig.step(live());
    }
    let lit = rig.field.lights().iter().filter(|l| l.color.length() > 0.0).count();
    assert_eq!(lit, 2);
    let heights: Vec<f64> = rig.field.lights()[..2].iter().map(|l| l.position.y).collect();
    assert!(
        (heights[0] - f64::from(HEIGHT * 0.3)).abs() < 1e-3 && (heights[1] - f64::from(HEIGHT * 0.75)).abs() < 1e-3
    );
}

#[test]
fn le_bout_du_mur_regarde_la_voiture_selon_le_cote() {
    for side in [-1_i8, 1] {
        let mut building = MatrixBuilding::new(MatrixVariant::Rain);
        building.show(&Placement {
            z: 0.0,
            side,
            tint: 0,
            scale: 1.0,
            spans_road: false,
        });
        let mut paint = [PartPaint::default(); 3];
        building.paint(&LandmarkFrame::rest(), &mut paint);
        assert!((paint[1].transform.translation.x - f32::from(side) * (SPAN / 2.0 + 0.2)).abs() < 1e-5);
    }
}
