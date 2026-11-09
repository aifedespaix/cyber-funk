//! Le bac à sable de l'onglet Debug vu du monde : vitesse de la voiture, quartier vide et « Tout nettoyer ».
//!
//! L'interface écrit [`DebugSandbox`] (une ressource de `cf-render`) ; ces trois systèmes le lisent et agissent, chacun
//! seulement quand son réglage change (le monde n'est écrit qu'alors) :
//!
//! - la vitesse devient le multiple d'allure du pilote ([`PilotSettings::speed_scale`](crate::PilotSettings)) ;
//! - le quartier vide coupe la génération des immeubles et re-tire les tronçons vivants ;
//! - « Tout nettoyer » arrête les événements du trafic, coupe la cascade en cours et vide le registre.

use bevy::prelude::*;
use cf_render::{ActiveEvents, ArmedTriggers, ChunkStream, DebugSandbox};

use crate::pilot::PilotSettings;
use crate::stunts::StuntDirector;
use crate::traffic::TrafficDirector;
use crate::world::NeonWorld;

/// `CfSet::Simulate` : la vitesse du bac à sable devient l'allure du pilote.
pub(crate) fn apply_car_speed(sandbox: Res<DebugSandbox>, mut pilot: ResMut<PilotSettings>) {
    let scale = f64::from(sandbox.car_speed());
    // Comparaison exacte (bits) : le pilote n'est réveillé que quand le curseur bouge.
    if pilot.speed_scale.to_bits() != scale.to_bits() {
        pilot.speed_scale = scale;
    }
}

/// `CfSet::Simulate`, avant les tronçons : vide (ou rend) la ville, puis re-tire les tronçons vivants.
pub(crate) fn apply_empty(
    sandbox: Res<DebugSandbox>,
    mut world: ResMut<NeonWorld>,
    mut stream: ResMut<ChunkStream<NeonWorld>>,
) {
    if world.set_empty(sandbox.empty()) {
        stream.replan_all();
    }
}

/// `CfSet::Simulate`, avant le trafic : sur une nouvelle demande de nettoyage, le trafic finit tous ses événements, la
/// cascade en cours est coupée, le registre « En cours » est vidé et les demandes armées pendant une pause sont abandonnées.
pub(crate) fn apply_clear(
    sandbox: Res<DebugSandbox>,
    mut seen: Local<u32>,
    traffic: Option<ResMut<TrafficDirector>>,
    stunts: Option<ResMut<StuntDirector>>,
    active: Option<ResMut<ActiveEvents>>,
    armed: Option<ResMut<ArmedTriggers>>,
) {
    if *seen == sandbox.clear_seq() {
        return;
    }
    *seen = sandbox.clear_seq();
    if let Some(mut traffic) = traffic {
        traffic.request_clear();
    }
    if let Some(mut stunts) = stunts {
        stunts.cut();
    }
    if let Some(mut active) = active {
        active.clear();
    }
    if let Some(mut armed) = armed {
        armed.clear();
    }
}
