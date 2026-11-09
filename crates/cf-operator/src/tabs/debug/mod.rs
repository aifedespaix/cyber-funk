//! Onglet Debug (Préparation seulement) : lancer à la main les événements du monde et voir ce qui tourne.
//!
//! ```text
//! barre du haut   mode manuel · En cours N (jauge) · recherche (/) · Tout arrêter
//! grille          Panneaux | Immeubles | Voitures | Quartiers | Caméra    dock : pad voiture, DJ,
//!                 2/3 puces, 1/3 « En cours » par colonne                 Autres, Réglages monde
//! ```
//!
//! Le dessin ([`draw`]) ne lit que des références ([`DebugView`]) et n'écrit que deux choses : les ordres de l'image
//! (`Action`, plafonnés à `MAX_ORDERS_PER_FRAME`, tous par le chemin du MIDI : le limiteur reste aux commandes) et des
//! [`Gesture`]s, appliqués ensuite aux ressources ([`apply_gestures`], [`apply_world_gestures`]). Une ressource n'est
//! donc déréférencée en écriture que sur un vrai geste : les réglages sauvegardés (écriture automatique) et le monde
//! ne sont pas réveillés à chaque image. L'onglet est retiré du Live par la coque : un clic de trop ne doit pas lancer
//! un événement devant le public.

mod camera;
mod chip;
mod columns;
mod districts;
mod dj;
mod dock;
mod graffiti;
#[cfg(test)]
mod graffiti_tests;
mod grid;
mod journal;
mod lane;
mod pad;
mod persist;
mod rows;
mod sandbox;
mod shot_names;
mod signs;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_world;
mod timeline;
mod top_bar;
mod weather;

use std::collections::BTreeSet;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::egui;
use cf_cinema::{CinemaDirector, ShotOrder};
use cf_control::Action;
use cf_render::{
    ActiveEvents, ActiveId, ArmedTriggers, CarSelection, CurrentShow, DebugSandbox, DebugTimeline, DistrictAffinity,
    DistrictGradeState, DistrictOrder, DistrictState, EventCategory, EventJournal, FacadeTuning, GradeLibrary,
    GraffitiTuning, GraffitiWords, ManualDrive, RunHandover, RunOrder, RunPhase, Settings, ShowSheet, SignCatalog,
    SignOrder, SimClock, SimControl, TimelineReplay, WeatherControl, WorldAutomation, WorldEventCatalog,
};
use cf_show::persist::AppSettings;

use crate::OperatorMode;

#[cfg(test)]
pub(crate) use camera::camera_actions;
use columns::PlanCache;
use districts::DistrictPick;
pub(crate) use dock::Section;
use dock::{DOCK_WIDTH, OTHERS_KEY, choose_car};
use grid::{GAP, in_rect, split_dock};
use persist::{family_is_open, section_is_open, set_family_open, set_section_open};

/// Les clés des sections sont distinctes (elles sont écrites dans les réglages).
#[cfg(test)]
fn section_key_is_unique() -> bool {
    let keys: BTreeSet<_> = Section::ALL.iter().map(|s| s.key()).collect();
    keys.len() == Section::ALL.len()
}

/// Les épingles quand les réglages manquent.
static NO_PINS: BTreeSet<String> = BTreeSet::new();

/// État de l'onglet (jamais sauvegardé).
#[derive(Resource, Debug, Default)]
pub(crate) struct DebugState {
    /// Texte de la recherche.
    filter: String,
    /// Le quartier que l'opérateur s'apprête à demander.
    district: DistrictPick,
    /// Rangement du catalogue, recalculé seulement quand le filtre, les épingles ou le catalogue changent.
    plan: PlanCache,
    /// Le panneau que l'opérateur s'apprête à poser (type, visuel, texte).
    sign: signs::SignPick,
    /// Gestes de l'image, vidés par leur application (tampon réutilisé).
    gestures: Vec<Gesture>,
}

/// Un geste de l'opérateur, appliqué après le dessin.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Gesture {
    /// Mode manuel (`WorldAutomation::manual`).
    SetManual(bool),
    /// Vider le registre des événements en cours.
    StopAll,
    /// Retirer du registre une colonne.
    StopColumn(EventCategory),
    /// Retirer du registre une entrée.
    Stop(ActiveId),
    /// Épingler ou désépingler (`OperatorPrefs::pinned_events`).
    TogglePin(&'static str),
    /// Ordre au moteur de quartiers.
    District(DistrictOrder),
    /// Nouveaux curseurs des façades.
    Tuning(FacadeTuning),
    /// Nouveaux réglages des graffitis et des projecteurs.
    Graffiti(GraffitiTuning),
    /// Voiture demandée.
    Car(&'static str),
    /// Dépliage d'une section de « Réglages monde ».
    SectionOpen(Section, bool),
    /// Dépliage de « Autres ».
    OthersOpen(bool),
    /// Ordre au réalisateur (plan caméra imposé, ou retour à l'auto).
    Shot(ShotOrder),
    /// Bac à sable : nettoyer, vitesse, quartier vide, pause.
    Sandbox(sandbox::SandboxGesture),
    /// Pad voiture : pilotage manuel, voie, ordres de vol, Ascension.
    Manual(pad::ManualGesture),
    /// Nouveaux réglages de météo (pluie, brume, brouillard, éclairs).
    Weather(WeatherControl),
    /// Poser un panneau devant la voiture.
    Sign(SignOrder),
    /// Passation de DJ.
    Dj(dj::DjGesture),
}

/// Ce que le dessin lit. Tout est facultatif sauf l'état et la file d'ordres : un plugin absent (monde, réglages) se dit
/// en clair dans la colonne concernée.
#[derive(Debug)]
pub(crate) struct DebugView<'a> {
    pub state: &'a mut DebugState,
    /// Mode de l'opérateur (seconde sûreté : aucun ordre de quartier hors Préparation).
    pub mode: OperatorMode,
    pub catalog: Option<&'a WorldEventCatalog>,
    pub active: Option<&'a ActiveEvents>,
    /// Le journal des demandes (bandeau du bas).
    pub journal: Option<&'a EventJournal>,
    /// La timeline de la session et l'état du rejeu.
    pub timeline: Option<&'a DebugTimeline>,
    pub replaying: bool,
    /// Le bac à sable (réglages, pause, nombre d'événements armés).
    pub sandbox: sandbox::SandboxView,
    pub manual: Option<bool>,
    /// Temps de simulation (s), celui de `ActiveEvents`.
    pub now_s: f64,
    pub settings: Option<&'a AppSettings>,
    pub district: Option<&'a DistrictState>,
    /// Où chaque événement a sa place (puces grisées hors de leur quartier).
    pub affinity: Option<&'a DistrictAffinity>,
    pub library: Option<&'a GradeLibrary>,
    pub cars: Option<&'a CarSelection>,
    /// Le pilotage manuel de la voiture (pad).
    pub drive: Option<pad::DriveView>,
    /// La météo à la main (monde absent : section grisée).
    pub weather: Option<WeatherControl>,
    /// Les panneaux que le monde sait poser (monde absent : zone grisée).
    pub signs: Option<&'a SignCatalog>,
    /// Les DJ et la passation.
    pub dj: dj::DjView<'a>,
    /// Le réalisateur caméra (liste des plans).
    pub director: Option<&'a CinemaDirector>,
    pub tuning: Option<FacadeTuning>,
    /// Réglages des graffitis et mots holographiques chargés (monde absent : section grisée).
    pub graffiti: Option<&'a GraffitiTuning>,
    pub words: Option<&'a GraffitiWords>,
    /// Ordres de l'image (`LiveOrders`).
    pub orders: &'a mut Vec<Action>,
}

impl<'a> DebugView<'a> {
    /// Les événements épinglés.
    fn pinned(&self) -> &'a BTreeSet<String> {
        self.settings.map_or(&NO_PINS, |s| &s.operator.pinned_events)
    }
}

/// L'onglet : barre du haut, puis la grille et le dock sur toute la hauteur restante.
pub(crate) fn draw(ui: &mut egui::Ui, view: &mut DebugView<'_>) {
    let bar = top_bar::TopBar {
        manual: view.manual,
        running: view.active.map(ActiveEvents::len),
    };
    let pinned = view.pinned();
    let DebugState {
        filter, plan, gestures, ..
    } = &mut *view.state;
    top_bar::draw(ui, bar, filter, gestures);
    sandbox::draw(ui, &view.sandbox, gestures);
    if let Some(catalog) = view.catalog {
        plan.refresh(catalog, filter, pinned);
    }
    ui.separator();
    let mut body = ui.available_rect_before_wrap();
    // Les bandeaux du bas (timeline au-dessus, journal dessous) ; sur une fenêtre basse, la grille garde toute la place.
    let journal_rect = (body.height() >= journal::MIN_HEIGHT_FOR_JOURNAL).then(|| {
        let top = body.bottom() - journal::JOURNAL_HEIGHT;
        let rect = egui::Rect::from_min_max(egui::pos2(body.left(), top), body.max);
        body.max.y = top - GAP;
        rect
    });
    let timeline_rect = (journal_rect.is_some() && body.height() >= timeline::MIN_HEIGHT_FOR_TIMELINE).then(|| {
        let top = body.bottom() - timeline::TIMELINE_HEIGHT;
        let rect = egui::Rect::from_min_max(egui::pos2(body.left(), top), body.max);
        body.max.y = top - GAP;
        rect
    });
    let (columns_rect, dock_rect) = split_dock(body, DOCK_WIDTH, GAP);
    let (grid_rect, camera_rect) = camera::split_camera(columns_rect, camera::CAMERA_WIDTH, GAP);
    grid::draw_grid(ui, grid_rect, view);
    camera::draw_camera_column(ui, camera_rect, view);
    in_rect(ui, dock_rect, |ui| dock::draw_dock(ui, view));
    if let Some(rect) = timeline_rect {
        let shown = timeline::TimelineView {
            timeline: view.timeline,
            now_s: view.now_s,
            replaying: view.replaying,
        };
        let (journal, now_s) = (view.journal, view.now_s);
        let gestures = &mut view.state.gestures;
        in_rect(ui, rect, |ui| {
            timeline::draw(ui, shown, gestures, |t| timeline::full_report(t, journal, now_s));
        });
    }
    if let Some(rect) = journal_rect {
        in_rect(ui, rect, |ui| journal::draw(ui, view.journal));
    }
}

/// Applique les gestes qui touchent l'automatisme, le registre et les réglages de l'interface ; les autres (quartiers,
/// façades, voiture) restent pour [`apply_world_gestures`]. Chaque ressource n'est écrite que si le geste la change.
pub(crate) fn apply_gestures(
    gestures: &mut Vec<Gesture>,
    automation: &mut Option<ResMut<WorldAutomation>>,
    active: &mut Option<ResMut<ActiveEvents>>,
    settings: &mut Option<ResMut<Settings>>,
) {
    gestures.retain(|gesture| {
        match *gesture {
            Gesture::SetManual(manual) => {
                if let Some(automation) = automation.as_mut() {
                    automation.set_if_neq(WorldAutomation { manual });
                }
            }
            Gesture::StopAll => {
                if let Some(active) = active.as_mut() {
                    active.clear();
                }
            }
            Gesture::StopColumn(category) => {
                if let Some(active) = active.as_mut() {
                    active.end_category(category);
                }
            }
            Gesture::Stop(id) => {
                if let Some(active) = active.as_mut() {
                    active.end(id);
                }
            }
            Gesture::TogglePin(id) => {
                if let Some(settings) = settings.as_mut() {
                    let pinned = &mut settings.value.operator.pinned_events;
                    if !pinned.remove(id) {
                        pinned.insert(id.to_owned());
                    }
                }
            }
            Gesture::SectionOpen(section, open) => {
                if let Some(settings) = settings.as_mut()
                    && section_is_open(&settings.value, section) != open
                {
                    set_section_open(&mut settings.value, section, open);
                }
            }
            Gesture::OthersOpen(open) => {
                if let Some(settings) = settings.as_mut()
                    && family_is_open(&settings.value, OTHERS_KEY) != open
                {
                    set_family_open(&mut settings.value, OTHERS_KEY, open);
                }
            }
            Gesture::District(_)
            | Gesture::Tuning(_)
            | Gesture::Graffiti(_)
            | Gesture::Car(_)
            | Gesture::Shot(_)
            | Gesture::Manual(_)
            | Gesture::Weather(_)
            | Gesture::Sign(_)
            | Gesture::Dj(_)
            | Gesture::Sandbox(_) => {
                return true;
            }
        }
        false
    });
}

/// Applique (et vide) les gestes restants : ordres de quartier, façades, voiture.
pub(crate) fn apply_world_gestures(
    gestures: &mut Vec<Gesture>,
    districts: &mut MessageWriter<DistrictOrder>,
    tuning: &mut Option<ResMut<FacadeTuning>>,
    graffiti: &mut Option<ResMut<GraffitiTuning>>,
    words: Option<&GraffitiWords>,
    cars: &mut Option<ResMut<CarSelection>>,
    settings: &mut Option<ResMut<Settings>>,
) {
    for gesture in gestures.drain(..) {
        match gesture {
            Gesture::District(order) => {
                districts.write(order);
            }
            Gesture::Tuning(next) => {
                if let Some(tuning) = tuning.as_mut() {
                    tuning.set_if_neq(next);
                }
            }
            Gesture::Graffiti(next) => {
                if let Some(graffiti) = graffiti.as_mut() {
                    // La ressource n'est écrite que si le geste la change.
                    if let Some(next) = graffiti::committed(graffiti, next, words) {
                        **graffiti = next;
                    }
                }
            }
            Gesture::Car(key) => {
                let Some(selection) = cars.as_mut() else {
                    continue;
                };
                match settings.as_mut() {
                    Some(settings) => {
                        // Écrire dans `value` réveille l'écriture automatique : seulement si la demande a changé.
                        let mut copy = settings.value.clone();
                        if choose_car(selection, &mut copy, key) {
                            settings.value = copy;
                        }
                    }
                    None => {
                        selection.request(key);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Les ressources propres à l'onglet (l'automatisme, le registre et l'horloge viennent de la coque, qui les lit déjà).
#[derive(SystemParam)]
pub(crate) struct DebugParams<'w> {
    state: ResMut<'w, DebugState>,
    catalog: Option<Res<'w, WorldEventCatalog>>,
    journal: Option<Res<'w, EventJournal>>,
    sandbox: Option<ResMut<'w, DebugSandbox>>,
    timeline: Option<ResMut<'w, DebugTimeline>>,
    replay: Option<ResMut<'w, TimelineReplay>>,
    armed: Option<Res<'w, ArmedTriggers>>,
    settings: Option<ResMut<'w, Settings>>,
    district: Option<Res<'w, DistrictState>>,
    affinity: Option<Res<'w, DistrictAffinity>>,
    grades: Option<Res<'w, DistrictGradeState>>,
    districts: MessageWriter<'w, DistrictOrder>,
    tuning: Option<ResMut<'w, FacadeTuning>>,
    graffiti: Option<ResMut<'w, GraffitiTuning>>,
    words: Option<Res<'w, GraffitiWords>>,
    cars: Option<ResMut<'w, CarSelection>>,
    manual: Option<ResMut<'w, ManualDrive>>,
    weather: Option<ResMut<'w, WeatherControl>>,
    signs: Option<Res<'w, SignCatalog>>,
    sign_orders: MessageWriter<'w, SignOrder>,
    sheet: Option<Res<'w, ShowSheet>>,
    current_show: Option<Res<'w, CurrentShow>>,
    run_phase: Option<Res<'w, RunPhase>>,
    run_handover: Option<Res<'w, RunHandover>>,
    runs: MessageWriter<'w, RunOrder>,
    director: Option<Res<'w, CinemaDirector>>,
    shots: MessageWriter<'w, ShotOrder>,
}

/// Les ressources partagées avec la coque.
#[allow(missing_debug_implementations)] // Des accès aux ressources, rien à afficher.
pub(crate) struct Shared<'a, 'w> {
    pub automation: &'a mut Option<ResMut<'w, WorldAutomation>>,
    pub active: &'a mut Option<ResMut<'w, ActiveEvents>>,
    pub clock: Option<&'a SimClock>,
    pub sim: &'a mut Option<ResMut<'w, SimControl>>,
}

/// Dessine l'onglet dans le panneau central puis applique les gestes de l'image.
pub(crate) fn show(
    root: &mut egui::Ui,
    params: &mut DebugParams,
    shared: Shared<'_, '_>,
    mode: OperatorMode,
    orders: &mut Vec<Action>,
) {
    let DebugParams {
        state,
        catalog,
        journal,
        sandbox,
        timeline,
        replay,
        armed,
        settings,
        district,
        affinity,
        grades,
        districts,
        tuning,
        graffiti,
        words,
        cars,
        manual,
        weather,
        signs,
        sign_orders,
        sheet,
        current_show,
        run_phase,
        run_handover,
        runs,
        director,
        shots,
    } = params;
    egui::CentralPanel::default().show(root, |ui| {
        let mut view = DebugView {
            state,
            mode,
            catalog: catalog.as_deref(),
            active: shared.active.as_deref(),
            journal: journal.as_deref(),
            timeline: timeline.as_deref(),
            replaying: replay.as_deref().is_some_and(TimelineReplay::running),
            sandbox: sandbox::SandboxView {
                sandbox: sandbox.as_deref().cloned(),
                paused: shared.sim.as_deref().map(SimControl::paused),
                armed: armed.as_deref().map_or(0, ArmedTriggers::len),
            },
            manual: shared.automation.as_deref().map(|a| a.manual),
            now_s: shared.clock.map_or(0.0, SimClock::sim_s),
            settings: settings.as_deref().map(|s| &s.value),
            district: district.as_deref(),
            affinity: affinity.as_deref(),
            library: grades.as_deref().map(DistrictGradeState::library),
            cars: cars.as_deref(),
            drive: manual.as_deref().map(pad::DriveView::of),
            weather: weather.as_deref().copied(),
            signs: signs.as_deref(),
            dj: dj::DjView {
                sheet: sheet.as_deref(),
                slot: current_show.as_deref().map(|s| s.slot),
                handover_to: run_handover.as_deref().and_then(|h| h.0),
                run: run_phase.as_deref().copied(),
            },
            director: director.as_deref(),
            tuning: tuning.as_deref().copied(),
            graffiti: graffiti.as_deref(),
            words: words.as_deref(),
            orders,
        };
        draw(ui, &mut view);
    });
    if state.gestures.is_empty() {
        return;
    }
    let gestures = &mut state.gestures;
    gestures.retain(|gesture| match gesture {
        Gesture::Shot(order) => {
            shots.write(*order);
            false
        }
        Gesture::Sign(order) => {
            sign_orders.write(*order);
            false
        }
        Gesture::Dj(dj::DjGesture::Handover(slot)) => {
            runs.write(RunOrder::Handover { slot: *slot });
            false
        }
        Gesture::Weather(next) => {
            if let Some(weather) = weather.as_mut() {
                weather.set_if_neq(*next);
            }
            false
        }
        Gesture::Manual(action) => {
            if let Some(manual) = manual.as_mut() {
                pad::apply(*action, manual);
            }
            false
        }
        Gesture::Sandbox(action) => {
            let mut targets = sandbox::Targets {
                sandbox,
                sim: shared.sim,
                automation: shared.automation,
                timeline,
                replay,
                now_s: shared.clock.map_or(0.0, SimClock::sim_s),
            };
            sandbox::apply(*action, &mut targets);
            false
        }
        _ => true,
    });
    apply_gestures(gestures, shared.automation, shared.active, settings);
    apply_world_gestures(gestures, districts, tuning, graffiti, words.as_deref(), cars, settings);
}
