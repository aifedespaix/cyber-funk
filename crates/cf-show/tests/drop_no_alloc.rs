//! Aucune allocation par image en régime établi (règle 4) : calme, montée, plan en attente, image du drop, après le
//! drop. Seule la *préparation* d'un drop (une fois par montée) copie des identifiants, donc elle reste hors zone.
//! `assert_no_alloc` remplace l'allocateur de ce binaire de test et fait avorter le processus à la moindre allocation.

#![allow(
    clippy::unwrap_used,
    reason = "fichier de test : les aides hors #[test] peuvent échouer bruyamment"
)]

use std::path::PathBuf;

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use cf_show::drop::{DropConductor, DropInput, DropTitles, TitleLanguage};
use cf_show::freshness::Freshness;
use cf_show::gates::open_gates;
use cf_show::model::ShowGates;
use rand_core::SeedableRng;
use rand_pcg::Pcg32;

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

const DT: f32 = 1.0 / 60.0;

struct Rig {
    conductor: DropConductor<Pcg32>,
    freshness: Freshness,
    gates: ShowGates,
    now: f64,
}

impl Rig {
    /// Une image ; `guarded` : toute allocation fait avorter le test.
    fn frame(&mut self, intensity: f32, real_drop: bool, guarded: bool) {
        self.now += f64::from(DT);
        let input = DropInput {
            dt: DT,
            now: self.now,
            intensity,
            real_drop,
            forced: false,
            gates: &self.gates,
            heat: 0.8,
            level: 3,
            flashes: true,
            language: TitleLanguage::Fr,
        };
        if guarded {
            assert_no_alloc(|| self.conductor.update(&mut self.freshness, &input));
        } else {
            self.conductor.update(&mut self.freshness, &input);
        }
    }
}

#[test]
fn steady_state_frames_never_allocate() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/titles/drop-titles.ron");
    let titles = DropTitles::load(&path).unwrap();
    let mut rig = Rig {
        conductor: DropConductor::new(Pcg32::seed_from_u64(5), titles),
        freshness: Freshness::default(),
        gates: open_gates(),
        now: 0.0,
    };
    rig.conductor.register_known(&mut rig.freshness);
    // Les cascades du look et le plan de cascade sont connus de la fraîcheur à l'ouverture du show : en jouer un
    // n'insère rien. (Le chef du spectacle les déclare ; au premier usage d'un id inconnu, la fraîcheur alloue une fois.)
    let mut stunts: Vec<String> = rig.gates.stunt_kinds.iter().map(|k| format!("stunt:{k}")).collect();
    stunts.push("stunt:ascension".to_owned());
    stunts.push("shot:stunt-cam".to_owned());
    rig.freshness.known(&stunts);

    // Deux cycles complets : le premier prépare puis frappe, le second rejoue le régime établi.
    for cycle in 0..2 {
        for _ in 0..1800 {
            rig.frame(0.3, false, true); // calme
        }
        // Montée : la préparation (copie des identifiants) a lieu sur l'image où le plan apparaît, hors zone.
        let mut i = 0;
        while rig.conductor.plan().is_none() {
            rig.frame(0.3 + 0.55 * (i as f32 / 720.0).min(1.0), false, false);
            i += 1;
            assert!(i < 3000, "cycle {cycle} : aucune montée reconnue");
        }
        for _ in 0..120 {
            rig.frame(0.85, false, true); // plan en attente
        }
        rig.frame(0.9, true, true);
        assert!(rig.conductor.drop_now());
        for _ in 0..600 {
            rig.frame(0.9, false, true); // après le drop
        }
    }
}
