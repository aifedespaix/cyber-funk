//! La façade « Matrix Data » (`matrix-building.ts`) : une pluie de glyphes sur quatre plans en profondeur, en trois
//! variantes (`rain`, `flux`, `binary`).
//!
//! Approche de Sillage, gardée : pas de texture. Chaque plan est un calcul de colonnes dans le fragment
//! (`landmark_matrix_wall.wesl`), décalé par parallaxe selon la position de la caméra dans le repère de la façade. Devant
//! le mur, un petit essaim de grands glyphes tombe à quelques mètres : ceux-là sont de vraie géométrie (un maillage fusionné,
//! `landmark_matrix_sprouts.wesl`). Au drop, la couleur du flux bascule (rouge, cyan, arc-en-ciel).
//!
//! Écart avec Sillage (règle 7 de `CLAUDE.md`) : la bascule de palette était instantanée sur tout le mur. Elle passe par un
//! limiteur (un départ à la fois, au plus trois par seconde) et se **fond** en plus de `MIN_FADE` ; flashs coupés ou mode
//! respiration : le flux reste vert terminal, l'état de repos. Le défilement est intégré côté CPU (`speed × time` de
//! Sillage téléportait les rangées à chaque variation des basses). L'arc-en-ciel des lumières glisse sur le cycle au lieu de
//! changer de teinte quatre fois par seconde.

use bevy::math::{Quat, Vec2, Vec3, Vec4};
use bevy::transform::components::Transform;
use cf_render::SceneColor;

use super::flash::Burst;
use super::frame::LandmarkFrame;
use super::landmark::{Landmark, LandmarkKind, Placement, StepCtx};
use super::math::damp;
use super::part::{BodySpec, Look, MeshData, MeshSpec, PartPaint, PartSpec};
use crate::pilot::rand::Rand;

const SPAN: f32 = 36.0;
const HEIGHT: f32 = 74.0;
const DEPTH: f32 = 26.0;
/// Caractères qui jaillissent du mur : de gros glyphes nets, à plusieurs mètres de la façade.
const SPROUTS: usize = 70;
const SPROUT_OUT: f32 = 7.0;
/// Après un drop, la palette bascule pendant ce nombre de temps avant de revenir au vert.
const MODE_BEATS: u32 = 16;
/// Palettes du flux : 0 vert terminal, 1 rouge sang, 2 cyan électrique, 3 arc-en-ciel qui balaie.
const MODES: u32 = 4;
/// Vitesse de fondu de la couleur du flux (1/s) : moins de 1 / `MIN_FADE`, la pente d'un fondu ne dépasse jamais la borne.
const FADE_RATE: f32 = 8.0;
/// Durée de retombée demandée au limiteur pour une bascule (s).
const SWITCH_DECAY_S: f32 = 0.5;

/// Les trois façons de faire couler le code : pluie verticale, courants horizontaux, chiffres binaires avec balayage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatrixVariant {
    /// Pluie verticale.
    Rain,
    /// Courants horizontaux.
    Flux,
    /// Chiffres binaires, balayage et glitchs.
    Binary,
}

impl MatrixVariant {
    const fn seed(self) -> u32 {
        match self {
            Self::Rain => 131,
            Self::Flux => 137,
            Self::Binary => 149,
        }
    }

    const fn kind(self) -> LandmarkKind {
        match self {
            Self::Rain => LandmarkKind::Matrix,
            Self::Flux => LandmarkKind::Flux,
            Self::Binary => LandmarkKind::Binary,
        }
    }

    const fn index(self) -> f32 {
        match self {
            Self::Rain => 0.0,
            Self::Flux => 1.0,
            Self::Binary => 2.0,
        }
    }
}

/// Le mur de code.
#[derive(Debug)]
pub struct MatrixBuilding {
    variant: MatrixVariant,
    rng: Rand,
    burst: Burst,
    side: i8,
    mode: u32,
    mode_beats: u32,
    last_mode: u32,
    /// Teinte courante du flux, fondue vers `tint_target`.
    tint: Vec3,
    tint_target: Vec3,
    /// Part d'arc-en-ciel, fondue vers `rainbow_target`.
    rainbow: f32,
    rainbow_target: f32,
    light: Vec3,
    colors_ready: bool,
    /// Défilement du code, intégré côté CPU (voir l'écart de la PR).
    scroll: f64,
}

impl MatrixBuilding {
    /// Le mur de code de cette variante.
    #[must_use]
    pub fn new(variant: MatrixVariant) -> Self {
        Self {
            variant,
            rng: Rand::new(u64::from(variant.seed()) ^ 0x4D41_5452),
            burst: Burst::default(),
            side: 1,
            mode: 0,
            mode_beats: 0,
            last_mode: 0,
            tint: Vec3::ZERO,
            tint_target: Vec3::ZERO,
            rainbow: 0.0,
            rainbow_target: 0.0,
            light: Vec3::ZERO,
            colors_ready: false,
            scroll: 0.0,
        }
    }

    /// La palette courante du flux (0 : vert de repos).
    #[must_use]
    pub fn mode(&self) -> u32 {
        self.mode
    }

    fn set_mode(&mut self, mode: u32, frame: &LandmarkFrame) {
        self.mode = mode;
        self.mode_beats = 0;
        let colors = &frame.colors;
        self.tint_target = match mode.min(2) {
            0 => colors.get(SceneColor::Lime),
            1 => colors.get(SceneColor::Alarm),
            _ => colors.get(SceneColor::Mint),
        };
        self.rainbow_target = if mode > 2 { 1.0 } else { 0.0 };
    }

    /// Les glyphes qui jaillissent : un plan par glyphe, `(x, phase, profondeur, vitesse)` dans la couleur des sommets.
    fn sprouts_mesh(&self) -> MeshData {
        let mut rng = Rand::new(u64::from(self.variant.seed()) ^ 0x5352_4F55);
        let mut mesh = MeshData::default();
        for _ in 0..SPROUTS {
            let seed = [
                rng.unit() as f32,
                rng.unit() as f32,
                1.0 + rng.unit() as f32 * SPROUT_OUT,
                0.6 + rng.unit() as f32 * 1.6,
            ];
            let first = mesh.push_quad(Vec3::ZERO, Vec2::ONE);
            debug_assert_eq!(first as usize % 4, 0);
            mesh.colors.extend([seed; 4]);
        }
        mesh
    }
}

impl Landmark for MatrixBuilding {
    fn kind(&self) -> LandmarkKind {
        self.variant.kind()
    }

    fn span(&self) -> f32 {
        SPAN
    }

    fn body(&self) -> Option<BodySpec> {
        Some(BodySpec {
            height: HEIGHT,
            depth: DEPTH,
            seed: self.variant.seed(),
        })
    }

    fn body_tint(&self) -> Option<SceneColor> {
        Some(SceneColor::Lime)
    }

    fn parts(&self) -> Vec<PartSpec> {
        vec![
            PartSpec::new("mur", MeshSpec::Quad { w: SPAN, h: HEIGHT }, Look::MatrixWall),
            PartSpec::new("bout du mur", MeshSpec::Quad { w: DEPTH, h: HEIGHT }, Look::MatrixWall),
            // Les glyphes jaillissent jusqu'à `SPROUT_OUT` + la taille d'un glyphe devant la façade, sur toute la largeur et la
            // hauteur : l'enveloppe est celle de ce volume (le vertex shader déplace les sommets).
            PartSpec::new(
                "glyphes",
                MeshSpec::Custom(Box::new(self.sprouts_mesh())),
                Look::MatrixSprouts,
            )
            .with_bounds(
                Vec3::new(-SPAN * 0.5 - 4.0, -4.0, -2.0),
                Vec3::new(SPAN * 0.5 + 4.0, HEIGHT + 4.0, SPROUT_OUT + 5.0),
            ),
        ]
    }

    fn show(&mut self, place: &Placement) {
        self.side = place.side;
        self.mode = 0;
        self.mode_beats = 0;
        self.rainbow = 0.0;
        self.rainbow_target = 0.0;
        self.colors_ready = false;
        self.burst.reset();
    }

    fn step(&mut self, frame: &LandmarkFrame, ctx: &mut StepCtx<'_>) {
        let colors = &frame.colors;
        if !self.colors_ready {
            // Première image du passage : le vert, sans fondu (l'immeuble naît dans cette couleur).
            self.set_mode(0, frame);
            self.tint = self.tint_target;
            self.colors_ready = true;
        }
        self.burst.step(frame.dt, frame.policy.permitted());
        // Le défilement : `speed × time` de Sillage téléportait les rangées à chaque variation des basses.
        let rate = 1.0 + frame.beat.bass * 0.9 + frame.drive.intensity * 0.6;
        self.scroll = (self.scroll + f64::from(rate * frame.dt)) % 65_536.0;
        if frame.beat.kick && self.mode != 0 {
            self.mode_beats += 1;
            if self.mode_beats >= MODE_BEATS {
                self.set_mode(0, frame);
            }
        }
        if frame.drive.drop && self.burst.trigger(SWITCH_DECAY_S, frame.time) {
            // Jamais deux fois la même palette de suite, et jamais de retour au vert avant la fin du délai.
            let mut next = 1 + (self.rng.unit() * f64::from(MODES - 1)) as u32;
            if next == self.last_mode {
                next = 1 + (next % (MODES - 1));
            }
            self.last_mode = next;
            self.set_mode(next, frame);
        }
        // Le fondu : la couleur glisse (au moins `MIN_FADE` d'un bout à l'autre, voir `FADE_RATE`).
        for axis in 0..3 {
            self.tint[axis] = damp(self.tint[axis], self.tint_target[axis], FADE_RATE, frame.dt);
        }
        self.rainbow = damp(self.rainbow, self.rainbow_target, FADE_RATE, frame.dt);

        // Les lumières prennent la couleur courante du flux (l'arc-en-ciel défile sur le cycle, sans saut).
        let cycle = colors.cycle();
        let t = (frame.time * 4.0).max(0.0);
        let i = t.floor() as usize;
        let sweep = cycle[i % cycle.len()].lerp(cycle[(i + 1) % cycle.len()], t.fract() as f32);
        self.light = self.tint.lerp(sweep, self.rainbow);
        let power = 0.9 + frame.beat.pulse * 1.8 + frame.beat.bass;
        ctx.light(0, Vec3::new(0.0, HEIGHT * 0.3, 14.0), 120.0, self.light, power);
        ctx.light(1, Vec3::new(0.0, HEIGHT * 0.75, 14.0), 120.0, self.light, power);
    }

    fn paint(&self, _frame: &LandmarkFrame, out: &mut [PartPaint]) {
        let a = self.tint.extend(self.rainbow);
        let paint = |width: f32, transform: Transform| PartPaint {
            transform,
            mood: self.light,
            a,
            b: Vec4::new(width, self.variant.index(), 0.0, 0.0),
            c: Vec4::new(self.scroll as f32, 0.0, 0.0, 0.0),
            ..PartPaint::default()
        };
        let side = f32::from(self.side);
        let [wall, front, sprouts] = out else { return };
        *wall = paint(SPAN, Transform::from_xyz(0.0, HEIGHT / 2.0, 0.2));
        // Façade d'entrée tournée vers le conducteur qui arrive : le mur se lit des deux côtés. Le bout du bloc qui regarde
        // vers la voiture (z monde plus grand) est à `+side × demi-largeur`.
        *front = paint(
            DEPTH,
            Transform::from_xyz(side * (SPAN / 2.0 + 0.2), HEIGHT / 2.0, -DEPTH / 2.0)
                .with_rotation(Quat::from_rotation_y(side * std::f32::consts::FRAC_PI_2)),
        );
        *sprouts = paint(SPAN, Transform::IDENTITY);
    }
}

#[cfg(test)]
mod tests {
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
}
