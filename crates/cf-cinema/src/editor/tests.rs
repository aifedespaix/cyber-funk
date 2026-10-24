//! L'éditeur sur une heure simulée avec un catalogue à la mesure de celui de Sillage : parts par rang, écarts rares et
//! épiques, plafond de l'habitacle, respiration, portes, et sécurité de `strobe`.

use cf_show::model::ShotTier as GateTier;

use super::testkit::{TestShow, sillage_catalog};
use super::*;
use crate::choreo::ShowSide;
use crate::types::Section;
use crate::world::{CinemaWorld, NoObstacles};

const HOUR: f64 = 3600.0;
/// Durée moyenne d'un plan en spectacle (s) : grille de 4 à 8 temps à 128 BPM, étirée par le rythme.
const SHOT_SECONDS: f64 = 9.0;

struct Run {
    catalog: Vec<Box<dyn ShotSpec>>,
    editor: Editor,
    show: TestShow,
    world: CinemaWorld,
}

impl Run {
    fn new(seed: u32, calm: bool) -> Self {
        let catalog = sillage_catalog();
        let mut show = TestShow::default();
        let mut editor = Editor::new(&catalog, calm, CinemaRng::new(seed));
        editor.reset(0.0, &mut show);
        let mut world = CinemaWorld::default();
        world.show.active = true;
        Self {
            catalog,
            editor,
            show,
            world,
        }
    }

    /// Un morceau qui tourne : 40 s de calme, 40 s de groove, 40 s de sommet, avec un break au début du calme et une
    /// montée avant le sommet.
    fn set_music(&mut self, t: f64) {
        self.world.time = t;
        let phase = t % 120.0;
        let (section, intensity) = match phase {
            p if p < 40.0 => (Section::Calm, 0.25),
            p if p < 80.0 => (Section::Groove, 0.55),
            _ => (Section::Peak, 0.9),
        };
        self.world.section = section;
        self.world.intensity = intensity;
        if (phase - 66.0).abs() < SHOT_SECONDS / 2.0 {
            self.editor.build_at = t;
        }
        if phase < SHOT_SECONDS {
            self.editor.break_at = t;
        }
    }

    fn pick(&mut self, mode: PickMode, current: Option<usize>) -> Option<usize> {
        let frame = CinemaFrame::new(&self.world, &NoObstacles, &[]);
        let index = self.editor.pick(&frame, &self.show, &self.catalog, mode, current)?;
        let spec = self.catalog.get(index)?;
        self.editor
            .register(index, spec.as_ref(), &frame, &mut self.show, false);
        Some(index)
    }

    fn id(&self, index: usize) -> &'static str {
        self.catalog[index].id()
    }

    fn tier(&self, index: usize) -> ShotTier {
        self.catalog[index].tier()
    }

    /// Une heure de plans : `(rang, id, instant)` de chacun.
    fn hour(&mut self) -> Vec<(ShotTier, &'static str, f64)> {
        let mut shots = Vec::new();
        let mut current = None;
        let mut t = 0.0;
        while t < HOUR {
            self.set_music(t);
            let mode = if shots.is_empty() {
                PickMode::Start
            } else {
                PickMode::Normal
            };
            let Some(index) = self.pick(mode, current) else { break };
            shots.push((self.tier(index), self.id(index), t));
            current = Some(index);
            t += SHOT_SECONDS;
        }
        shots
    }
}

#[test]
fn parts_par_rang_sur_une_heure() {
    let mut run = Run::new(0x00c0_ffee, false);
    let shots = run.hour();
    let n = shots.len() as f64;
    let share = |tier: ShotTier| shots.iter().filter(|s| s.0 == tier).count() as f64 / n;
    let (common, uncommon, rare, epic) = (
        share(ShotTier::Common),
        share(ShotTier::Uncommon),
        share(ShotTier::Rare),
        share(ShotTier::Epic),
    );
    let report =
        format!("commun {common:.3}, peu commun {uncommon:.3}, rare {rare:.3}, épique {epic:.3} sur {n} plans");
    // Probabilités de base de Sillage : 0,70 / 0,22 / 0,07 / 0,01. Les garde-fous de rareté (un rare au plus toutes les
    // 75 s, soit au mieux 9 s / 75 s = 12 % des plans) et les bonus de montée ôtent des rares au profit du reste.
    assert!((0.55..=0.85).contains(&common), "commun : {report}");
    assert!((0.12..=0.40).contains(&uncommon), "peu commun : {report}");
    assert!(rare <= SHOT_SECONDS / RARE_GAP, "rare : {report}");
    assert!(rare >= 0.02, "les rares existent : {report}");
    assert!(epic <= 0.02, "épique : {report}");
}

#[test]
fn ecarts_entre_rares_et_epiques() {
    for seed in [1, 2, 3, 0x00c0_ffee] {
        let shots = Run::new(seed, false).hour();
        let rares: Vec<f64> = shots
            .iter()
            .filter(|s| matches!(s.0, ShotTier::Rare | ShotTier::Epic))
            .map(|s| s.2)
            .collect();
        assert!(!rares.is_empty(), "graine {seed} : aucun rare en une heure");
        for pair in rares.windows(2) {
            assert!(
                pair[1] - pair[0] >= RARE_GAP,
                "graine {seed} : deux rares à {} s",
                pair[1] - pair[0]
            );
        }
        let epics: Vec<f64> = shots.iter().filter(|s| s.0 == ShotTier::Epic).map(|s| s.2).collect();
        assert!(
            epics.first().is_none_or(|&t| t >= EPIC_NOT_BEFORE),
            "épique trop tôt : {epics:?}"
        );
        for pair in epics.windows(2) {
            assert!(
                pair[1] - pair[0] >= EPIC_GAP,
                "graine {seed} : deux épiques à {} s",
                pair[1] - pair[0]
            );
        }
    }
}

#[test]
fn respiration_apres_un_rare_et_retour_voiture() {
    let mut run = Run::new(9, false);
    run.world.intensity = 0.3;
    run.world.time = 200.0;
    let rare = run.editor.find("god-view");
    assert!(rare.is_some());
    // Un rare à l'antenne : les deux plans suivants sont communs (le tirage strict les y contraint).
    let frame = CinemaFrame::new(&run.world, &NoObstacles, &[]);
    let spec = &run.catalog[rare.unwrap_or(0)];
    run.editor
        .register(rare.unwrap_or(0), spec.as_ref(), &frame, &mut run.show, false);
    for step in 1..=2 {
        run.world.time += SHOT_SECONDS;
        let index = run.pick(PickMode::Normal, rare);
        let tier = index.map(|i| run.tier(i));
        assert_eq!(tier, Some(ShotTier::Common), "plan {step} après un rare");
    }
}

#[test]
fn le_demarrage_ne_montre_qu_une_voiture_commune() {
    for seed in 0..40 {
        let mut run = Run::new(seed, false);
        run.set_music(0.0);
        let index = run.pick(PickMode::Start, None).unwrap_or(0);
        assert_eq!(run.tier(index), ShotTier::Common);
        assert!(run.catalog[index].follows_car(), "{}", run.id(index));
    }
}

#[test]
fn l_habitacle_reste_plafonne() {
    let mut run = Run::new(5, false);
    run.hour();
    let cockpit = run.editor.find("cockpit").unwrap_or(0);
    let share = run.editor.share(cockpit, HOUR);
    assert!(share < COCKPIT_CAP + 0.04, "part d'écran de l'habitacle : {share:.3}");
}

#[test]
fn les_plans_nerveux_sont_bloques_en_mode_calme() {
    let shots = Run::new(4, true).hour();
    for blocked in ["strobe", "dutch", "flyby", "spiral", "vertigo", "wheel"] {
        assert!(shots.iter().all(|s| s.1 != blocked), "{blocked} en mode calme");
    }
}

#[test]
fn la_respiration_ne_garde_que_les_plans_lents() {
    let mut run = Run::new(6, false);
    run.world.show.breathing = true;
    let shots = run.hour();
    assert!(shots.len() > 100);
    for (_, id, _) in &shots {
        assert!(BREATH_SHOTS.contains(id), "{id} pendant la respiration");
    }
}

#[test]
fn les_portes_du_look_filtrent_rangs_et_plans() {
    let mut run = Run::new(8, false);
    run.show.gates.shot_tiers = ["common", "uncommon"]
        .iter()
        .filter_map(|t| GateTier::new(*t).ok())
        .collect();
    run.show.gates.shot_block = ["orbit", "chase"]
        .iter()
        .filter_map(|id| cf_show::model::ShotId::new(*id).ok())
        .collect();
    let shots = run.hour();
    assert!(
        shots
            .iter()
            .all(|s| matches!(s.0, ShotTier::Common | ShotTier::Uncommon))
    );
    assert!(shots.iter().all(|s| s.1 != "orbit" && s.1 != "chase"));
}

#[test]
fn le_catalogue_se_declare_a_la_fraicheur_et_chaque_plan_la_marque() {
    let mut run = Run::new(3, false);
    run.set_music(10.0);
    let index = run.pick(PickMode::Start, None).unwrap_or(0);
    let key = format!("shot:{}", run.id(index));
    let fresh = run.show.freshness().weight("shot:wheel", 20.0);
    let used = run.show.freshness().weight(&key, 10.0);
    assert!(
        fresh > used,
        "un plan neuf ({fresh}) pèse plus qu'un plan qui vient de passer ({used})"
    );
    // Un plan déjà compté par le chef du spectacle (plan de drop) n'est pas compté deux fois.
    let before = run.show.freshness().weight("shot:orbit", 10.0);
    let orbit = run.editor.find("orbit").unwrap_or(0);
    let frame = CinemaFrame::new(&run.world, &NoObstacles, &[]);
    run.editor
        .register(orbit, run.catalog[orbit].as_ref(), &frame, &mut run.show, true);
    let after = run.show.freshness().weight("shot:orbit", 10.0);
    assert!((before - after).abs() < 1e-12);
}

#[test]
fn meme_graine_meme_heure() {
    let a = Run::new(77, false).hour();
    let b = Run::new(77, false).hour();
    assert_eq!(a, b);
    assert_ne!(a, Run::new(78, false).hour());
}

// --- Sécurité : le plan `strobe` ---

/// Une heure de tirages ordinaires, de drops et d'occasions, flashs coupés : `strobe` n'est jamais choisi.
#[test]
fn strobe_jamais_choisi_quand_les_flashs_sont_coupes() {
    let mut run = Run::new(11, false);
    run.world.show.flashes = false;
    let strobe = run.editor.find("strobe").unwrap_or(0);
    let mut current = None;
    let mut t = 0.0;
    let mut drops = 0;
    while t < HOUR {
        run.set_music(t);
        for mode in [PickMode::Normal, PickMode::Drop, PickMode::Ended] {
            let index = run.pick(mode, current);
            assert_ne!(index, Some(strobe), "strobe tiré en {mode:?} à {t} s flashs coupés");
            current = index;
            drops += usize::from(mode == PickMode::Drop);
        }
        let frame = CinemaFrame::new(&run.world, &NoObstacles, &[]);
        assert!(!run.editor.can_use(&frame, &run.show, &run.catalog, strobe, false));
        // Un ordre de l'opérateur (portes ignorées) n'y change rien : la sûreté ne s'ignore pas.
        assert!(!run.editor.can_use(&frame, &run.show, &run.catalog, strobe, true));
        assert_ne!(
            run.editor
                .pick_among(&frame, &run.show, &run.catalog, &["strobe"], None),
            Some(strobe)
        );
        t += SHOT_SECONDS;
    }
    assert!(drops > 100);
}

#[test]
fn strobe_jamais_choisi_en_respiration() {
    let mut run = Run::new(12, false);
    run.world.show.breathing = true;
    let strobe = run.editor.find("strobe").unwrap_or(0);
    let shots = run.hour();
    assert!(shots.iter().all(|s| s.1 != "strobe"));
    let frame = CinemaFrame::new(&run.world, &NoObstacles, &[]);
    assert!(!run.editor.can_use(&frame, &run.show, &run.catalog, strobe, true));
}

/// Témoin : avec les flashs permis, `strobe` sort bien (le test précédent ne passe donc pas par défaut).
#[test]
fn strobe_est_tire_quand_les_flashs_sont_permis() {
    let mut run = Run::new(11, false);
    let mut seen = false;
    let mut current = None;
    for step in 0..2000 {
        run.set_music(f64::from(step) * SHOT_SECONDS);
        let index = run.pick(PickMode::Drop, current);
        seen |= index.is_some_and(|i| run.id(i) == "strobe");
        current = index;
    }
    assert!(seen);
}
