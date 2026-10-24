//! La qualité adaptative sans GPU : une horloge et des mesures injectées dans la logique, puis la glue Bevy sous
//! `MinimalPlugins`.

#![allow(clippy::expect_used, clippy::panic, clippy::float_cmp)]

use std::time::Duration;

use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;

use super::logic::{MAX_LEVEL, PIXEL_SCALE, Quality};
use super::{QualityChanged, QualityLevers, QualityMode, QualityPlugin, levers_for};
use crate::{
    Compositor, CorePlugin, DiagnosticsSnapshot, FloatingOrigin, Layer, LayerTargets, LayersPlugin, PostFxPlugin,
    PostFxSettings, RenderScale,
};

/// Une machine simulée : coût GPU `base_ms` à pleine résolution, proportionnel au nombre de pixels, avec la
/// durée d'image collée à 16,7 ms (vsync) tant que le GPU suit.
struct Rig {
    quality: Quality,
    level: usize,
    base_ms: f64,
    clock_s: f64,
    /// Niveaux successifs, un par changement : (instant en s, niveau).
    changes: Vec<(f64, usize)>,
}

impl Rig {
    fn new(base_ms: f64) -> Self {
        Self {
            quality: Quality::new(),
            level: 0,
            base_ms,
            clock_s: 0.0,
            changes: Vec::new(),
        }
    }

    fn gpu_ms(&self) -> f64 {
        let scale = f64::from(PIXEL_SCALE[self.level]);
        self.base_ms * scale * scale
    }

    /// Une image : renvoie vrai si le niveau a changé.
    fn step(&mut self) -> bool {
        let gpu_s = self.gpu_ms() / 1000.0;
        let frame_s = gpu_s.max(1.0 / 60.0);
        self.clock_s += frame_s;
        if let Some(level) = self.quality.sample(frame_s, Some(gpu_s)) {
            self.level = level;
            self.changes.push((self.clock_s, level));
            return true;
        }
        false
    }

    fn run_for(&mut self, seconds: f64) {
        let end = self.clock_s + seconds;
        while self.clock_s < end {
            self.step();
        }
    }
}

#[test]
fn une_machine_qui_tient_ne_bouge_jamais() {
    let mut rig = Rig::new(10.0);
    rig.run_for(120.0);
    assert_eq!(rig.changes.len(), 0, "{:?}", rig.changes);
    assert_eq!(rig.level, 0);
}

#[test]
fn une_charge_gpu_fait_descendre_d_un_cran_en_moins_de_5_secondes() {
    let mut rig = Rig::new(10.0);
    rig.run_for(10.0);
    assert_eq!(rig.level, 0);

    // La charge arrive : à pleine résolution le GPU met 45 ms (22 i/s).
    rig.base_ms = 45.0;
    let start = rig.clock_s;
    while !rig.step() {
        assert!(rig.clock_s - start < 5.0, "toujours au niveau 0 après 5 s de charge");
    }
    let delay = rig.clock_s - start;
    assert_eq!(rig.level, 1);
    assert!(delay < 5.0, "descente en {delay:.2} s");
}

#[test]
fn la_descente_tient_aussi_a_20_images_par_seconde_et_en_cas_de_charge_severe() {
    // 120 images à 100 ms feraient 12 s : c'est la fenêtre bornée en durée qui tient le délai.
    let mut rig = Rig::new(10.0);
    rig.run_for(6.0);
    rig.base_ms = 100.0;
    let start = rig.clock_s;
    while !rig.step() {
        assert!(rig.clock_s - start < 5.0);
    }
    assert!(rig.clock_s - start < 5.0);
}

#[test]
fn le_retrait_de_la_charge_fait_remonter_sans_osciller() {
    let mut rig = Rig::new(10.0);
    rig.run_for(6.0);
    rig.base_ms = 60.0;
    rig.run_for(40.0);
    // 60 ms → 48,6 → 38,4 → 29,4 ms : il faut le dernier cran pour passer sous le seuil de 33 ms.
    assert_eq!(rig.level, MAX_LEVEL);
    let descent_changes = rig.changes.len();

    // La charge part : le GPU ne met plus que 10 ms à pleine résolution.
    rig.base_ms = 10.0;
    rig.run_for(120.0);
    assert_eq!(rig.level, 0, "la qualité doit revenir au plein régime");

    let after: Vec<usize> = rig.changes[descent_changes..].iter().map(|c| c.1).collect();
    assert_eq!(
        after,
        vec![2, 1, 0],
        "remontée d'un cran à la fois, sans aller-retour : {after:?}"
    );
}

#[test]
fn une_charge_limite_se_stabilise_sans_osciller() {
    // À 40 ms le plein régime est lent ; au niveau 1 (32,4 ms) c'est tenable, mais remonter coûterait 40 ms : on reste.
    let mut rig = Rig::new(40.0);
    rig.run_for(300.0);
    assert_eq!(rig.level, 1);
    assert_eq!(rig.changes.len(), 1, "un seul changement attendu : {:?}", rig.changes);
}

#[test]
fn une_remontee_trop_optimiste_double_la_patience() {
    let mut q = Quality::new();
    // Chaque image dure `frame` ; renvoie le nouveau niveau et l'horloge à laquelle il est apparu.
    let mut clock = 0.0;
    let mut feed = |q: &mut Quality, frame: f64, seconds: f64| -> Vec<(f64, usize)> {
        let end = clock + seconds;
        let mut out = Vec::new();
        while clock < end {
            clock += frame;
            if let Some(level) = q.sample(frame, None) {
                out.push((clock, level));
            }
        }
        out
    };
    assert_eq!(feed(&mut q, 0.05, 8.0).len(), 1, "descente au niveau 1");
    assert_eq!(q.patience(), 5.0);

    // Remontée tout de suite jugée tenable (images de 10 ms) : on s'arrête dès qu'elle a lieu…
    let raised_at = loop {
        let up = feed(&mut q, 0.01, 0.5);
        if let Some((at, level)) = up.first() {
            assert_eq!(*level, 0);
            break *at;
        }
    };
    // … puis rechute aussitôt : la patience double.
    let fell_at = loop {
        let down = feed(&mut q, 0.05, 0.5);
        if let Some((at, level)) = down.first() {
            assert_eq!(*level, 1);
            break *at;
        }
    };
    assert_eq!(q.patience(), 10.0);
    assert!(fell_at - raised_at < 25.0);

    // La remontée suivante attend au moins la nouvelle patience.
    let again = feed(&mut q, 0.01, 40.0);
    let second_raise = again.first().map(|c| c.0).expect("seconde remontée");
    assert!(
        second_raise - fell_at >= 10.0,
        "remontée après {:.1} s",
        second_raise - fell_at
    );
}

#[test]
fn les_accrocs_font_descendre_meme_si_la_moyenne_est_bonne() {
    let mut q = Quality::new();
    let mut changed = None;
    // 1 image sur 6 dure 60 ms (accroc), les autres 8 ms : moyenne ~16 ms, mais > 10 % d'accrocs.
    for i in 0..2000 {
        let frame = if i % 6 == 0 { 0.06 } else { 0.008 };
        if let Some(level) = q.sample(frame, None) {
            changed = Some(level);
            break;
        }
    }
    assert_eq!(changed, Some(1));
}

#[test]
fn une_pause_n_est_pas_une_mesure() {
    let mut q = Quality::new();
    for _ in 0..2000 {
        // Fenêtre masquée : des images de 1 s ne font jamais descendre.
        assert_eq!(q.sample(1.0, None), None);
    }
    assert_eq!(q.level(), 0);
    // Durées absurdes ignorées.
    assert_eq!(q.sample(f64::NAN, None), None);
    assert_eq!(q.sample(-1.0, None), None);
}

#[test]
fn rien_ne_bouge_pendant_le_demarrage() {
    let mut q = Quality::new();
    for _ in 0..39 {
        assert_eq!(q.sample(0.1, Some(0.1)), None, "4 s d'échauffement sans jugement");
    }
}

#[test]
fn un_gpu_sature_fait_descendre_meme_si_l_image_parait_bonne() {
    let mut q = Quality::new();
    let mut level = None;
    for _ in 0..1000 {
        // Durée d'image de 30 ms (< 33 ms) mais GPU à 36 ms : timestamps en retard sur l'image.
        if let Some(l) = q.sample(0.030, Some(0.036)) {
            level = Some(l);
            break;
        }
    }
    assert_eq!(level, Some(1));
}

#[test]
fn le_temps_gpu_permet_une_remontee_que_la_duree_d_image_seule_refuse() {
    // Niveau 2, images de 22 ms : sans GPU on estime 22 × (0,9/0,8)² = 28 ms au cran du dessus, trop pour remonter.
    // Le GPU ne met que 8 ms : le cran du dessus est largement tenable.
    let run = |gpu: Option<f64>| {
        let mut q = Quality::new();
        q.force_level(2);
        for _ in 0..10_000 {
            if q.sample(0.022, gpu).is_some() {
                return q.level();
            }
        }
        q.level()
    };
    assert_eq!(run(None), 2);
    assert_eq!(run(Some(0.008)), 1);
}

#[test]
fn au_repos_la_nettete_revient_tout_de_suite() {
    let mut q = Quality::new();
    q.force_level(3);
    assert_eq!(q.settle(true, 0.5), None);
    assert_eq!(q.settle(true, 0.5), None);
    assert_eq!(q.settle(true, 0.5), Some(0));
    assert_eq!(q.level(), 0);
    // Un mouvement remet le compteur à zéro.
    q.force_level(2);
    assert_eq!(q.settle(false, 0.1), None);
    assert_eq!(q.settle(true, 1.0), None);
    assert_eq!(q.settle(false, 0.1), None);
    assert_eq!(q.settle(true, 1.0), None);
}

#[test]
fn la_table_des_leviers_reprend_celle_de_sillage() {
    let full = levers_for(0);
    assert_eq!(full.render_scale, 1.0);
    assert!(full.bloom);
    assert_eq!(full.bloom_divisor, 2);
    assert_eq!(
        (full.particle_density, full.rain_density, full.side_render_every),
        (1.0, 1.0, 1)
    );

    let one = levers_for(1);
    assert_eq!((one.render_scale, one.bloom, one.bloom_divisor), (0.9, true, 4));

    let two = levers_for(2);
    assert_eq!((two.render_scale, two.bloom, two.side_render_every), (0.8, false, 2));
    assert_eq!(two.particle_density, 1.0);

    let last = levers_for(MAX_LEVEL);
    assert_eq!((last.render_scale, last.bloom, last.side_render_every), (0.7, false, 3));
    assert_eq!((last.particle_density, last.rain_density), (0.4, 0.5));
    assert_eq!(levers_for(99), last, "niveau borné");

    assert!((0..6).all(|f| full.side_turn(f)));
    assert_eq!((0..6).filter(|f| two.side_turn(*f)).count(), 3);
    assert_eq!((0..6).filter(|f| last.side_turn(*f)).count(), 2);
}

// ---------------------------------------------------------------------------------------------------------------
// Glue Bevy
// ---------------------------------------------------------------------------------------------------------------

fn app(frame: Duration) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), CorePlugin))
        .init_asset::<Image>()
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .init_resource::<FloatingOrigin>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(frame))
        .add_plugins((
            LayersPlugin {
                fallback_size: UVec2::new(1000, 500),
            },
            PostFxPlugin,
            QualityPlugin,
        ));
    app.update();
    app.update();
    app
}

fn run(app: &mut App, frames: u32) {
    for _ in 0..frames {
        app.update();
    }
}

fn image_size(app: &App, layer: Layer) -> UVec2 {
    let handle = app.world().resource::<LayerTargets>().image(layer).clone();
    app.world()
        .resource::<Assets<Image>>()
        .get(&handle)
        .expect("texture")
        .size()
}

fn bloom_of_compositor(app: &mut App) -> Option<Bloom> {
    app.world_mut()
        .query_filtered::<&Bloom, With<Compositor>>()
        .single(app.world())
        .ok()
        .cloned()
}

#[test]
fn sous_charge_les_couches_3d_descendent_en_resolution_mais_pas_la_surcouche() {
    // Images de 45 ms (22 i/s), GPU à 40 ms.
    let mut app = app(Duration::from_millis(45));
    app.insert_resource(DiagnosticsSnapshot {
        gpu_total_ms: Some(40.0),
        ..default()
    });
    run(&mut app, 200);

    let levers = *app.world().resource::<QualityLevers>();
    assert!(levers.level >= 1, "niveau {}", levers.level);
    let scale = app.world().resource::<RenderScale>().0;
    assert_eq!(scale, levers.render_scale);
    assert!(scale < 1.0);

    let output = app.world().resource::<LayerTargets>().size();
    let render = app.world().resource::<LayerTargets>().render_size();
    assert_eq!(output, UVec2::new(1000, 500));
    assert_eq!(render.x, (1000.0 * scale).round() as u32);
    assert_eq!(image_size(&app, Layer::World), render);
    assert_eq!(image_size(&app, Layer::Background), render);
    assert_eq!(image_size(&app, Layer::Foreground), render);
    assert_eq!(image_size(&app, Layer::Overlay), output, "les titres restent nets");

    let post = *app.world().resource::<PostFxSettings>();
    assert_eq!(post.quality as usize, levers.level);
    assert_eq!(post.bloom_divisor, levers.bloom_divisor);
}

#[derive(Resource, Default)]
struct Heard(Vec<QualityChanged>);

fn listen(mut messages: MessageReader<QualityChanged>, mut heard: ResMut<Heard>) {
    heard.0.extend(messages.read().copied());
}

#[test]
fn chaque_changement_de_niveau_est_annonce_une_fois() {
    let mut app = app(Duration::from_millis(45));
    app.init_resource::<Heard>().add_systems(Update, listen);
    run(&mut app, 400);
    let heard = &app.world().resource::<Heard>().0;
    assert_ne!(heard.len(), 0);
    // Un cran à la fois, de proche en proche depuis 0.
    let mut level = 0;
    for m in heard {
        assert_eq!(m.from, level);
        assert_eq!(m.from.abs_diff(m.to), 1);
        level = m.to;
    }
    assert_eq!(level, app.world().resource::<QualityLevers>().level);
}

#[test]
fn le_mode_fixe_impose_le_niveau_et_le_plein_regime_revient() {
    let mut app = app(Duration::from_micros(16_667));
    app.insert_resource(QualityMode::Fixed(3));
    run(&mut app, 2);
    assert_eq!(app.world().resource::<RenderScale>().0, 0.7);
    assert_eq!(
        app.world().resource::<LayerTargets>().render_size(),
        UVec2::new(700, 350)
    );

    app.insert_resource(QualityMode::Fixed(0));
    run(&mut app, 2);
    assert_eq!(app.world().resource::<RenderScale>().0, 1.0);
    assert_eq!(
        app.world().resource::<LayerTargets>().render_size(),
        UVec2::new(1000, 500)
    );
    assert_eq!(image_size(&app, Layer::World), UVec2::new(1000, 500));
}

#[test]
fn le_bloom_s_estompe_avant_d_etre_retire_et_revient_sans_a_coup() {
    let mut app = app(Duration::from_micros(16_667));
    // Le compositeur est équipé : bloom présent, gain 1.
    let full = bloom_of_compositor(&mut app).expect("bloom");
    assert!(full.intensity > 0.4);

    // Niveau 2 : bloom éteint… mais pas d'un coup.
    app.insert_resource(QualityMode::Fixed(2));
    run(&mut app, 3);
    let post = *app.world().resource::<PostFxSettings>();
    assert!(post.bloom, "encore là au début de l'estompage");
    assert!(
        post.bloom_gain < 1.0 && post.bloom_gain > 0.8,
        "gain {}",
        post.bloom_gain
    );
    let fading = bloom_of_compositor(&mut app).expect("bloom");
    assert!(fading.intensity < full.intensity && fading.intensity > 0.8 * full.intensity);

    // Chaque image ne retire qu'une petite part de l'intensité (pas de saut).
    let mut previous = fading.intensity;
    for _ in 0..120 {
        app.update();
        if let Some(bloom) = bloom_of_compositor(&mut app) {
            assert!(previous - bloom.intensity < 0.1 * full.intensity, "saut de bloom");
            previous = bloom.intensity;
        }
    }
    assert!(!app.world().resource::<PostFxSettings>().bloom);
    assert!(bloom_of_compositor(&mut app).is_none(), "bloom retiré une fois à zéro");

    // Retour au niveau 0 : le bloom revient à zéro et remonte progressivement.
    app.insert_resource(QualityMode::Fixed(0));
    run(&mut app, 2);
    let back = bloom_of_compositor(&mut app).expect("bloom de retour");
    assert!(
        back.intensity < 0.2 * full.intensity,
        "retour sans à-coup : {}",
        back.intensity
    );
    run(&mut app, 120);
    let settled = bloom_of_compositor(&mut app).expect("bloom");
    assert!(settled.intensity > 0.9 * full.intensity);
}

#[test]
fn sans_changement_aucune_ecriture() {
    let mut app = app(Duration::from_micros(16_667));
    run(&mut app, 5);
    app.world_mut().clear_trackers();
    run(&mut app, 30);
    // Au repos, ni la mise à l'échelle ni les réglages du post-process ne sont réécrits : pas de ré-extraction.
    assert!(!app.world().resource_ref::<RenderScale>().is_changed());
    assert!(!app.world().resource_ref::<PostFxSettings>().is_changed());
    assert!(!app.world().resource_ref::<QualityLevers>().is_changed());
    assert_eq!(
        app.world().resource::<LayerTargets>().render_size(),
        UVec2::new(1000, 500)
    );
}
