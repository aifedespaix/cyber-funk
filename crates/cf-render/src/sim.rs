//! Pilotage de la simulation depuis l'opérateur : pause, reset, horloge à la milliseconde.
//!
//! - **Pause** : les ensembles `Show`, `Simulate` et `Direct` ne tournent plus (voir [`sim_running`]) ; `Input` et
//!   `Present` continuent, donc l'interface répond et l'analyse audio vit. La lecture de la démo est mise en pause
//!   avec la simulation et reprend avec elle.
//! - **Reset** : [`SimReset`] est publié ; le chef du spectacle est reconstruit (`show::reset_show`), son garde-fou de
//!   panique réarmé, la piste de démo repart du début. Les autres plugins peuvent lire [`SimReset`] pour remettre
//!   leur propre état à zéro.
//! - **Horloge** : [`SimClock`] compte le temps de simulation (pauses exclues) et les images.

use bevy::prelude::*;

use crate::CfSet;
use crate::audio::{AudioCommand, AudioStatus};
use crate::debuglog::DebugLog;

/// Demandes de l'opérateur sur la simulation. Écrite par l'interface, lue par [`apply_control`].
#[derive(Resource, Debug, Default)]
pub struct SimControl {
    paused: bool,
    reset_pending: bool,
}

impl SimControl {
    /// La simulation est en pause.
    #[must_use]
    pub const fn paused(&self) -> bool {
        self.paused
    }

    /// Met en pause ou reprend.
    pub const fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// Bascule pause / reprise.
    pub const fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    /// Demande un reset de la simulation à l'image suivante.
    pub const fn request_reset(&mut self) {
        self.reset_pending = true;
    }
}

/// Horloge de simulation : le temps passe tant que la simulation ne dort pas.
#[derive(Resource, Debug, Default, Clone)]
pub struct SimClock {
    sim_s: f64,
    frame: u64,
}

impl SimClock {
    /// Secondes de simulation (pauses exclues) depuis le démarrage ou le dernier reset.
    #[must_use]
    pub const fn sim_s(&self) -> f64 {
        self.sim_s
    }

    /// Millisecondes de simulation, arrondies à la milliseconde inférieure.
    #[must_use]
    pub fn sim_ms(&self) -> u64 {
        (self.sim_s * 1000.0).max(0.0) as u64
    }

    /// Nombre d'images depuis le démarrage.
    #[must_use]
    pub const fn frame(&self) -> u64 {
        self.frame
    }
}

/// Le spectacle vient d'être remis à zéro par l'opérateur : chaque plugin qui porte un état de simulation peut le lire
/// pour repartir proprement.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimReset;

/// Condition d'exécution des ensembles de simulation.
#[must_use]
pub fn sim_running(control: Res<SimControl>) -> bool {
    !control.paused
}

pub(crate) fn build(app: &mut App) {
    app.init_resource::<SimControl>()
        .init_resource::<SimClock>()
        .add_message::<SimReset>()
        // Idempotent : le plugin audio enregistre le même message.
        .add_message::<AudioCommand>()
        .configure_sets(
            Update,
            (CfSet::Show, CfSet::Simulate, CfSet::Direct).run_if(sim_running),
        )
        .add_systems(
            Update,
            (update_clock, apply_control).chain().in_set(CfSet::Input),
        );
}

/// Avance l'horloge : toujours le compteur d'images, le temps de simulation seulement hors pause.
pub(crate) fn update_clock(time: Option<Res<Time<Real>>>, control: Res<SimControl>, mut clock: ResMut<SimClock>) {
    clock.frame += 1;
    // Sans horloge Bevy (certains bancs d'essai n'ajoutent que `CorePlugin`), le temps de simulation reste à l'arrêt.
    if let (false, Some(time)) = (control.paused, time) {
        clock.sim_s += time.delta_secs_f64();
    }
}

/// Applique pause / reprise et reset : audio de démo, horloge, message [`SimReset`], journal.
#[allow(clippy::too_many_arguments)] // Paramètres de système Bevy : chacun est une ressource distincte.
pub(crate) fn apply_control(
    mut control: ResMut<SimControl>,
    mut clock: ResMut<SimClock>,
    status: Option<Res<AudioStatus>>,
    mut audio: MessageWriter<AudioCommand>,
    mut reset: MessageWriter<SimReset>,
    log: Option<Res<DebugLog>>,
    mut was_paused: Local<bool>,
    mut audio_was_playing: Local<bool>,
) {
    let demo = status.as_deref().and_then(|s| s.demo.as_ref());
    if control.reset_pending {
        control.reset_pending = false;
        control.paused = false;
        *was_paused = false;
        *audio_was_playing = false;
        clock.sim_s = 0.0;
        reset.write(SimReset);
        // La piste repart du début, et joue : c'est la musique qui doit retrouver la simulation, pas l'inverse.
        match demo.and_then(|d| d.track) {
            Some(track) => {
                audio.write(AudioCommand::DemoSelect(track));
                audio.write(AudioCommand::DemoPlay);
            }
            None if demo.is_some() => {
                audio.write(AudioCommand::DemoPlay);
            }
            None => {}
        }
        if let Some(log) = log {
            log.0.point(
                cf_debuglog::Level::Warn,
                "sim.reset",
                None,
                Some("reset demandé par l'opérateur"),
                serde_json::Value::Null,
            );
        }
        return;
    }
    if control.paused == *was_paused {
        return;
    }
    *was_paused = control.paused;
    if control.paused {
        *audio_was_playing = demo.is_some_and(|d| d.playing);
        if *audio_was_playing {
            audio.write(AudioCommand::DemoPause);
        }
    } else if std::mem::take(&mut *audio_was_playing) {
        audio.write(AudioCommand::DemoPlay);
    }
    if let Some(log) = log {
        let ev = if control.paused { "sim.pause" } else { "sim.resume" };
        log.0
            .point(cf_debuglog::Level::Info, ev, None, None, serde_json::Value::Null);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CorePlugin;

    #[derive(Resource, Default)]
    struct Counts {
        input: u32,
        show: u32,
        simulate: u32,
        present: u32,
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CorePlugin)).init_resource::<Counts>();
        app.add_systems(Update, (|mut c: ResMut<Counts>| c.input += 1).in_set(CfSet::Input));
        app.add_systems(Update, (|mut c: ResMut<Counts>| c.show += 1).in_set(CfSet::Show));
        app.add_systems(
            Update,
            (|mut c: ResMut<Counts>| c.simulate += 1).in_set(CfSet::Simulate),
        );
        app.add_systems(Update, (|mut c: ResMut<Counts>| c.present += 1).in_set(CfSet::Present));
        app
    }

    #[test]
    fn la_pause_fige_la_simulation_mais_pas_l_entree_ni_la_presentation() {
        let mut app = app();
        app.update();
        app.world_mut().resource_mut::<SimControl>().set_paused(true);
        app.update();
        app.update();
        let c = app.world().resource::<Counts>();
        assert_eq!((c.input, c.show, c.simulate, c.present), (3, 1, 1, 3));
        app.world_mut().resource_mut::<SimControl>().set_paused(false);
        app.update();
        let c = app.world().resource::<Counts>();
        assert_eq!((c.show, c.simulate), (2, 2));
    }

    #[test]
    fn l_horloge_de_simulation_s_arrete_en_pause_et_le_reset_la_remet_a_zero() {
        let mut app = app();
        std::thread::sleep(std::time::Duration::from_millis(20));
        app.update();
        std::thread::sleep(std::time::Duration::from_millis(20));
        app.update();
        let running = app.world().resource::<SimClock>().sim_s();
        assert!(running > 0.0);
        app.world_mut().resource_mut::<SimControl>().set_paused(true);
        app.update();
        let frozen = app.world().resource::<SimClock>().sim_s();
        std::thread::sleep(std::time::Duration::from_millis(20));
        app.update();
        assert!((app.world().resource::<SimClock>().sim_s() - frozen).abs() < f64::EPSILON);
        assert_eq!(app.world().resource::<SimClock>().frame(), 4);
        app.world_mut().resource_mut::<SimControl>().request_reset();
        app.update();
        assert!(app.world().resource::<SimClock>().sim_s() < f64::EPSILON);
        assert!(!app.world().resource::<SimControl>().paused(), "le reset reprend");
    }
}
