//! La barre « Bac à sable » de l'onglet Debug : tester un événement seul, au calme.
//!
//! - **Tout nettoyer** : arrête tous les événements en cours, vide la liste et passe en mode manuel (rien ne revient de
//!   lui-même) ;
//! - **Quartier vide** : la route seule, sans immeubles ni enseignes ni graffitis ;
//! - **Vitesse** de la voiture : arrêt, rampant, lent, moitié, normal, et un curseur ;
//! - **Pause** : la simulation s'arrête ; les événements cliqués pendant la pause sont **armés** et partent ensemble à
//!   la reprise, pour composer un plateau avant de le lancer.
//!
//! Un clic sur une puce avec **Maj** enfoncée la lance seule : nettoyage, mode manuel, puis l'événement.
//! Le dessin ne lit que des valeurs et n'écrit que des [`SandboxGesture`] (appliqués ensuite aux ressources).

use bevy::prelude::{DetectChangesMut, ResMut};
use bevy_egui::egui;
use cf_render::{DebugSandbox, DebugTimeline, SPEED_PRESETS, SimControl, TimelineReplay, WorldAutomation};

use super::Gesture;

/// Un geste du bac à sable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum SandboxGesture {
    /// Tout nettoyer (et passer en manuel).
    Clear,
    /// Vitesse de la voiture, 0..=1.
    Speed(f32),
    /// Quartier vide ou non.
    Empty(bool),
    /// Mettre en pause ou reprendre.
    SetPaused(bool),
    /// Rejouer la séquence enregistrée : nettoyer, puis relancer les mêmes demandes aux mêmes instants.
    Replay,
    /// Remettre la timeline à zéro.
    ClearTimeline,
}

/// Ce que la barre affiche.
#[derive(Debug, Clone)]
pub(crate) struct SandboxView {
    /// Réglages (`None` : ressource absente, la barre est grisée).
    pub sandbox: Option<DebugSandbox>,
    /// La simulation est en pause (`None` : commande absente).
    pub paused: Option<bool>,
    /// Événements armés pendant la pause.
    pub armed: usize,
}

/// Vrai si la vitesse `speed` est le bouton prédéfini `preset`.
fn is_preset(speed: f32, preset: f32) -> bool {
    (speed - preset).abs() < 0.005
}

/// La barre. Les gestes s'ajoutent à `gestures`.
pub(crate) fn draw(ui: &mut egui::Ui, view: &SandboxView, gestures: &mut Vec<Gesture>) {
    ui.horizontal_wrapped(|ui| {
        let enabled = view.sandbox.is_some();
        let sandbox = view.sandbox.clone().unwrap_or_default();
        ui.strong("Bac à sable");

        let clear = ui
            .add_enabled(enabled, egui::Button::new("🧹 Tout nettoyer"))
            .on_hover_text(
                "Arrête tous les événements, vide la liste et passe en mode manuel : plus rien ne revient seul.",
            )
            .on_disabled_hover_text("Bac à sable absent (monde absent).");
        if clear.clicked() {
            gestures.push(Gesture::Sandbox(SandboxGesture::Clear));
        }

        let mut empty = sandbox.empty();
        if ui
            .add_enabled(enabled, egui::Checkbox::new(&mut empty, "Quartier vide"))
            .on_hover_text(
                "La route seule : ni immeubles, ni enseignes, ni graffitis. Les événements restent lançables.",
            )
            .changed()
        {
            gestures.push(Gesture::Sandbox(SandboxGesture::Empty(empty)));
        }

        ui.separator();
        ui.label("Voiture");
        let speed = sandbox.car_speed();
        for (label, preset) in SPEED_PRESETS {
            if ui
                .add_enabled(enabled, egui::Button::selectable(is_preset(speed, preset), label))
                .on_hover_text(format!("{:.0} % de l'allure normale", preset * 100.0))
                .clicked()
            {
                gestures.push(Gesture::Sandbox(SandboxGesture::Speed(preset)));
            }
        }
        let mut slider = speed;
        if ui
            .add_enabled(
                enabled,
                egui::Slider::new(&mut slider, 0.0..=1.0)
                    .show_value(false)
                    .fixed_decimals(2),
            )
            .on_hover_text(
                "Vitesse de la voiture, de l'arrêt à l'allure normale. Les événements gardent leur propre temps.",
            )
            .changed()
        {
            gestures.push(Gesture::Sandbox(SandboxGesture::Speed(slider)));
        }

        ui.separator();
        let paused = view.paused.unwrap_or(false);
        let pause = ui
            .add_enabled(
                view.paused.is_some(),
                egui::Button::selectable(paused, if paused { "▶ Reprendre" } else { "⏸ Pause" }),
            )
            .on_hover_text(
                "Pendant la pause, les événements cliqués sont armés et partent tous ensemble à la reprise.",
            );
        if pause.clicked() {
            gestures.push(Gesture::Sandbox(SandboxGesture::SetPaused(!paused)));
        }
        if paused || view.armed > 0 {
            ui.colored_label(
                egui::Color32::from_rgb(230, 190, 90),
                format!("{} armé{}", view.armed, if view.armed > 1 { "s" } else { "" }),
            );
        }
    });
}

/// Les ressources que les gestes du bac à sable écrivent. Chacune est facultative : monde ou commande absents, le geste
/// est ignoré sans bruit.
#[allow(missing_debug_implementations)] // Des accès aux ressources, rien à afficher.
pub(crate) struct Targets<'a, 'w, 'x> {
    pub sandbox: &'a mut Option<ResMut<'w, DebugSandbox>>,
    pub sim: &'a mut Option<ResMut<'x, SimControl>>,
    pub automation: &'a mut Option<ResMut<'x, WorldAutomation>>,
    pub timeline: &'a mut Option<ResMut<'w, DebugTimeline>>,
    pub replay: &'a mut Option<ResMut<'w, TimelineReplay>>,
    /// Temps de simulation (s).
    pub now_s: f64,
}

impl Targets<'_, '_, '_> {
    /// Nettoie : événements arrêtés, timeline à zéro, mode manuel. La timeline repart de zéro ici, avant que les demandes
    /// qui suivent (« lancer seul ») ne soient enregistrées.
    fn clear(&mut self) {
        if let Some(sandbox) = self.sandbox.as_mut() {
            sandbox.request_clear();
        }
        if let Some(timeline) = self.timeline.as_mut() {
            timeline.reset();
        }
        if let Some(replay) = self.replay.as_mut() {
            replay.stop();
        }
        // Un monde propre qui se repeuple tout seul ne sert à rien pour tester : on passe en manuel.
        if let Some(automation) = self.automation.as_mut() {
            automation.set_if_neq(WorldAutomation { manual: true });
        }
    }
}

/// Applique un geste aux ressources. Chaque ressource n'est écrite que si le geste la change.
pub(crate) fn apply(gesture: SandboxGesture, targets: &mut Targets<'_, '_, '_>) {
    match gesture {
        SandboxGesture::Clear => targets.clear(),
        SandboxGesture::Speed(speed) => {
            if let Some(sandbox) = targets.sandbox.as_mut() {
                let mut next = (**sandbox).clone();
                next.set_car_speed(speed);
                if **sandbox != next {
                    **sandbox = next;
                }
            }
        }
        SandboxGesture::Empty(empty) => {
            if let Some(sandbox) = targets.sandbox.as_mut()
                && sandbox.empty() != empty
            {
                sandbox.set_empty(empty);
            }
        }
        SandboxGesture::SetPaused(paused) => {
            if let Some(sim) = targets.sim.as_mut()
                && sim.paused() != paused
            {
                sim.set_paused(paused);
            }
        }
        SandboxGesture::Replay => {
            // Les demandes sont copiées avant le nettoyage, qui remet la timeline à zéro.
            let requests: Vec<(f64, String)> = targets
                .timeline
                .as_deref()
                .map_or_default(|timeline| timeline.requests().to_vec());
            if requests.is_empty() {
                return;
            }
            targets.clear();
            if let Some(replay) = targets.replay.as_mut() {
                replay.start(targets.now_s, &requests);
            }
        }
        SandboxGesture::ClearTimeline => {
            if let Some(timeline) = targets.timeline.as_mut() {
                timeline.reset();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_recognised_with_a_small_tolerance() {
        assert!(is_preset(0.3, 0.3));
        assert!(is_preset(0.301, 0.3));
        assert!(!is_preset(0.35, 0.3));
    }

    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;

    /// Les ressources du bac à sable, prises ensemble pour qu'elles partagent une même durée de vie.
    #[derive(bevy::ecs::system::SystemParam)]
    struct Res3<'w> {
        sandbox: Option<ResMut<'w, DebugSandbox>>,
        sim: Option<ResMut<'w, SimControl>>,
        automation: Option<ResMut<'w, WorldAutomation>>,
        timeline: Option<ResMut<'w, DebugTimeline>>,
        replay: Option<ResMut<'w, TimelineReplay>>,
    }

    fn run(world: &mut World, gesture: SandboxGesture) {
        world
            .run_system_once(move |mut r: Res3| {
                let mut targets = Targets {
                    sandbox: &mut r.sandbox,
                    sim: &mut r.sim,
                    automation: &mut r.automation,
                    timeline: &mut r.timeline,
                    replay: &mut r.replay,
                    now_s: 42.0,
                };
                apply(gesture, &mut targets);
            })
            .expect("le système tourne");
    }

    #[test]
    fn clearing_asks_the_world_resets_the_timeline_and_goes_manual() {
        let mut world = World::new();
        world.init_resource::<DebugSandbox>();
        world.init_resource::<WorldAutomation>();
        world.init_resource::<DebugTimeline>();
        world.resource_mut::<DebugTimeline>().request(1.0, "chase");
        run(&mut world, SandboxGesture::Clear);
        assert_eq!(world.resource::<DebugSandbox>().clear_seq(), 1);
        assert!(world.resource::<WorldAutomation>().manual);
        assert!(world.resource::<DebugTimeline>().is_empty());
    }

    #[test]
    fn replay_clears_then_starts_from_the_recorded_requests() {
        let mut world = World::new();
        world.init_resource::<DebugSandbox>();
        world.init_resource::<DebugTimeline>();
        world.init_resource::<TimelineReplay>();
        world.resource_mut::<DebugTimeline>().request(5.0, "chase");
        world.resource_mut::<DebugTimeline>().request(7.0, "storm");
        run(&mut world, SandboxGesture::Replay);
        assert_eq!(world.resource::<DebugSandbox>().clear_seq(), 1, "nettoyé d'abord");
        assert!(world.resource::<DebugTimeline>().is_empty());
        let mut replay = world.resource_mut::<TimelineReplay>();
        assert!(replay.running());
        let first: Vec<_> = replay.due(42.0).map(|id| id.0.to_string()).collect();
        assert_eq!(first, ["chase"], "la première demande part à l'instant 0 du rejeu");
    }

    #[test]
    fn replay_with_nothing_recorded_does_nothing() {
        let mut world = World::new();
        world.init_resource::<DebugSandbox>();
        world.init_resource::<DebugTimeline>();
        world.init_resource::<TimelineReplay>();
        run(&mut world, SandboxGesture::Replay);
        assert_eq!(
            world.resource::<DebugSandbox>().clear_seq(),
            0,
            "pas de nettoyage pour rien"
        );
        assert!(!world.resource::<TimelineReplay>().running());
    }

    #[test]
    fn speed_empty_and_pause_reach_their_resources() {
        let mut world = World::new();
        world.init_resource::<DebugSandbox>();
        world.init_resource::<SimControl>();
        run(&mut world, SandboxGesture::Speed(0.3));
        run(&mut world, SandboxGesture::Empty(true));
        run(&mut world, SandboxGesture::SetPaused(true));
        let sandbox = world.resource::<DebugSandbox>();
        assert!((sandbox.car_speed() - 0.3).abs() < 1e-6);
        assert!(sandbox.empty());
        assert!(world.resource::<SimControl>().paused());
    }

    #[test]
    fn absent_resources_are_ignored_without_panic() {
        let mut world = World::new();
        for gesture in [
            SandboxGesture::Clear,
            SandboxGesture::Speed(0.5),
            SandboxGesture::Empty(true),
            SandboxGesture::SetPaused(true),
            SandboxGesture::Replay,
            SandboxGesture::ClearTimeline,
        ] {
            run(&mut world, gesture);
        }
    }

    #[test]
    fn presets_cover_stop_and_normal() {
        assert!(SPEED_PRESETS.iter().any(|(_, s)| s.abs() < f32::EPSILON));
        assert!(SPEED_PRESETS.iter().any(|(_, s)| (s - 1.0).abs() < f32::EPSILON));
    }
}
