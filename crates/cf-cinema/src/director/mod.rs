//! Le réalisateur (`cinema/director.ts`) : enchaîne les plans sur les temps musicaux et gère les transitions. Le choix
//! éditorial vit dans [`Editor`] ; les plans ne connaissent ni la musique ni le catalogue.
//!
//! Chaque image : repères musicaux → synchro voiture → plan (ou transition) → décision de coupe → bornes (devant,
//! derrière, plafond) → anti-collision → garde (dernier bon cadrage). La pose finie est écrite dans la `CameraPose` que
//! lit le rig de `cf-render`.

mod cuts;
#[cfg(test)]
mod cuts_tests;
mod guard;
mod kind;
mod launch;
#[cfg(test)]
mod show_tests;
mod stats;
#[cfg(test)]
mod tests;

use bevy::math::DVec3;
use cf_render::CameraPose;

use crate::car_sync::{CarSync, SyncTarget};
use crate::choreo::{CutLimiter, NoShow, ShowSide};
use crate::cues::{CueList, CueTracker};
use crate::cutter::{Cutter, Decision, OnAir};
use crate::editor::{Editor, PickMode};
use crate::kit::{FOV_MAX, FOV_MIN};
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::transition::{Transition, transition_seconds};
use crate::types::{CueKind, TransitionKind};
use crate::world::CinemaFrame;

pub use crate::choreo::MAX_CUTS_PER_SECOND;
pub use guard::{CAMERA_RADIUS, MAX_AHEAD, MAX_BEHIND, MAX_HEIGHT};
pub use stats::DirectorStats;

/// Un plan et sa pose : la pose reste attachée à son plan quand les rôles (sortant / entrant) s'échangent.
#[derive(Default)]
struct Slot {
    shot: Option<Box<dyn Shot>>,
    spec: Option<usize>,
    pose: CameraPose,
    started_at: f64,
    /// Pose de l'image précédente et position de la voiture : de quoi repérer le saut qu'un plan s'inflige à lui-même.
    prev: Option<(CameraPose, DVec3)>,
}

impl Slot {
    fn clear(&mut self) {
        self.shot = None;
        self.spec = None;
        self.prev = None;
    }
}

/// Le réalisateur. Ressource Bevy : un seul écrivain, le système de `CfSet::Direct`.
#[derive(bevy::prelude::Resource)]
pub struct CinemaDirector {
    catalog: Vec<Box<dyn ShotSpec>>,
    calm: bool,
    running: bool,
    current: Slot,
    incoming: Slot,
    out: CameraPose,
    cues: CueTracker,
    transition: Transition,
    sync: CarSync,
    last_good: CameraPose,
    editor: Editor,
    cutter: Cutter,
    rng: CinemaRng,
    last_cut: f64,
    stats: DirectorStats,
    /// Plafond de cadence des coupes, partagé par tous les plans et toutes les transitions.
    limiter: CutLimiter,
    /// Le plan courant a produit une pose invalide : il est remplacé à l'image suivante.
    poisoned: bool,
    /// Un plan demande un astre au monde (`launchMeteor`).
    meteor_launches: u32,
}

impl std::fmt::Debug for CinemaDirector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CinemaDirector")
            .field("shots", &self.catalog.len())
            .field("running", &self.running)
            .field("shot", &self.shot_id())
            .field("transition", &self.transition.active.then_some(self.transition.kind))
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}

impl CinemaDirector {
    /// Réalisateur d'un catalogue. `seed` : graine du hasard, pour rejouer une séquence à l'identique ; `calm` :
    /// mode calme (transitions plus lentes, pas de coupe « waouh »).
    #[must_use]
    pub fn new(catalog: Vec<Box<dyn ShotSpec>>, calm: bool, seed: u32) -> Self {
        let editor = Editor::new(&catalog, calm, CinemaRng::new(seed));
        Self {
            catalog,
            calm,
            running: false,
            current: Slot::default(),
            incoming: Slot::default(),
            out: CameraPose::default(),
            cues: CueTracker::default(),
            transition: Transition::default(),
            sync: CarSync::default(),
            last_good: CameraPose::default(),
            editor,
            cutter: Cutter::new(CinemaRng::new(seed ^ 0x85eb_ca6b)),
            rng: CinemaRng::new(seed ^ 0x9e37_79b9),
            last_cut: f64::NEG_INFINITY,
            stats: DirectorStats::default(),
            limiter: CutLimiter::default(),
            poisoned: false,
            meteor_launches: 0,
        }
    }

    /// Le réalisateur tourne.
    #[must_use]
    pub fn active(&self) -> bool {
        self.running
    }

    /// Id du plan courant (celui qui arrive, pendant une transition).
    #[must_use]
    pub fn shot_id(&self) -> Option<&'static str> {
        if !self.running {
            return None;
        }
        let slot = if self.transition.active {
            &self.incoming
        } else {
            &self.current
        };
        slot.spec.and_then(|i| self.catalog.get(i)).map(|s| s.id())
    }

    /// Une transition est en cours (son genre).
    #[must_use]
    pub fn transition(&self) -> Option<TransitionKind> {
        self.transition.active.then_some(self.transition.kind)
    }

    /// Ce que le réalisateur a fait depuis son démarrage.
    #[must_use]
    pub fn stats(&self) -> DirectorStats {
        self.stats
    }

    /// Secousse (0 → 1) que la voiture inflige à l'écran cette image (atterrissage, déploiement) ; lue une fois.
    pub fn take_shake(&mut self) -> f32 {
        self.sync.take_shake()
    }

    /// Démarre depuis la pose actuelle de la caméra : un fondu, jamais un saut.
    pub fn start(&mut self, from: &CameraPose, frame: &CinemaFrame<'_>) {
        self.start_with(from, frame, &mut NoShow::default());
    }

    /// Comme [`Self::start`], avec le chef du spectacle (portes, fraîcheur commune, ordres).
    pub fn start_with(&mut self, from: &CameraPose, frame: &CinemaFrame<'_>, show: &mut dyn ShowSide) {
        self.cues.reset();
        self.sync.reset();
        self.cutter.reset(show);
        self.editor.reset(frame.time, show);
        self.limiter.reset();
        self.last_cut = frame.time;
        self.current.clear();
        self.current.pose = *from;
        self.out = *from;
        self.last_good = *from;
        self.transition.active = false;
        self.stats = DirectorStats::default();
        self.poisoned = false;
        tracing::info!(
            target: "cinema",
            intensity = frame.intensity,
            section = ?frame.section,
            live = frame.live,
            "démarrage du réalisateur"
        );
        self.running = self.queue(
            frame,
            show,
            Decision::of(PickMode::Start),
            Some(TransitionKind::Blend),
            false,
        );
    }

    /// Arrête le réalisateur (la pose reste où elle est).
    pub fn stop(&mut self) {
        if self.running {
            tracing::info!(target: "cinema", shot = self.shot_id(), "arrêt du réalisateur");
        }
        self.running = false;
        self.transition.active = false;
        self.current.clear();
        self.incoming.clear();
    }

    /// La pose à appliquer cette image.
    pub fn update(&mut self, frame: &CinemaFrame<'_>) -> &CameraPose {
        self.update_with(frame, &mut NoShow::default())
    }

    /// Comme [`Self::update`], avec le chef du spectacle : il fournit les portes du look, le drop préparé et les ordres
    /// de l'opérateur, et reçoit la fraîcheur des plans montrés.
    pub fn update_with(&mut self, frame: &CinemaFrame<'_>, show: &mut dyn ShowSide) -> &CameraPose {
        if !self.running {
            return &self.out;
        }
        let cues = self.cues.update(frame);
        self.sync.watch(frame);
        if self.sync.take_event().is_some() {
            self.cutter.react(frame.time);
        }
        for cue in cues.as_slice() {
            if !matches!(cue.kind, CueKind::Beat | CueKind::Bar) {
                tracing::debug!(target: "cinema::music", kind = ?cue.kind, section = ?cue.section, "repère");
            }
            match cue.kind {
                CueKind::Build => self.editor.build_at = frame.time,
                CueKind::Break => self.editor.break_at = frame.time,
                _ => {}
            }
            if let Some(shot) = self.current.shot.as_mut() {
                shot.cue(cue, frame);
            }
            if self.transition.active
                && let Some(shot) = self.incoming.shot.as_mut()
            {
                shot.cue(cue, frame);
            }
        }

        if self.transition.active {
            self.run_transition(frame, false);
        } else {
            self.run_shot(frame, show, &cues);
        }
        let target = self.sync_target();
        self.sync.apply(frame, &mut self.out, target);
        self.out.fov_deg = self.out.fov_deg.clamp(FOV_MIN, FOV_MAX);
        self.confine(frame);
        self.avoid_walls(frame);
        self.guard(frame);
        &self.out
    }

    fn sync_target(&self) -> Option<SyncTarget> {
        let index = if self.transition.active {
            self.incoming.spec
        } else {
            self.current.spec
        }?;
        let spec = self.catalog.get(index)?;
        Some(SyncTarget {
            id: spec.id(),
            follows_car: spec.follows_car(),
        })
    }

    fn run_shot(&mut self, frame: &CinemaFrame<'_>, show: &mut dyn ShowSide, cues: &CueList) {
        let Some(shot) = self.current.shot.as_mut() else { return };
        let alive = shot.update(frame, &mut self.current.pose) && !std::mem::take(&mut self.poisoned);
        let holding = shot.holding();
        self.current.steady(&mut self.limiter, &mut self.stats, frame);
        self.out = self.current.pose;
        // À l'arrêt de la musique, on garde le plan : seule la fin naturelle d'un plan le remplace.
        let decision = if !alive {
            tracing::debug!(
                target: "cinema",
                shot = self.shot_id(),
                after_s = frame.time - self.current.started_at,
                "plan terminé"
            );
            Some(Decision::of(PickMode::Ended))
        } else if frame.live {
            let Some(index) = self.current.spec else { return };
            let Some(spec) = self.catalog.get(index) else { return };
            let on_air = OnAir {
                index,
                spec: spec.as_ref(),
                holding,
                started_at: self.current.started_at,
            };
            self.cutter
                .decide(frame, cues, &on_air, &mut self.editor, &self.catalog, show)
        } else {
            None
        };
        if let Some(decision) = decision {
            let on_beat = cues.has(CueKind::Beat);
            self.queue(frame, show, decision, None, on_beat);
        }
    }

    /// Prépare le plan suivant et lance la transition ; renvoie faux si rien n'est disponible.
    fn queue(
        &mut self,
        frame: &CinemaFrame<'_>,
        show: &mut dyn ShowSide,
        decision: Decision,
        forced: Option<TransitionKind>,
        on_beat: bool,
    ) -> bool {
        let index = match decision.spec {
            Some(index) => index,
            None => match self
                .editor
                .pick(frame, &*show, &self.catalog, decision.mode, self.current.spec)
            {
                Some(index) => index,
                None => return false,
            },
        };
        let Some(spec) = self.catalog.get(index) else {
            return false;
        };
        let mut shot = spec.create(frame, &mut self.rng);
        if let Some((scale, seconds)) = shot.take_time_scale() {
            show.request_time_scale(scale, seconds);
        }
        self.meteor_launches += u32::from(shot.take_meteor_launch());
        shot.update(frame, &mut self.incoming.pose);
        self.incoming.shot = Some(shot);
        self.incoming.spec = Some(index);
        self.incoming.prev = None;
        self.incoming.steady(&mut self.limiter, &mut self.stats, frame);

        let distance = self.current.pose.position.distance(self.incoming.pose.position);
        let cabin = self.incoming.pose.cabin;
        let kind = decision
            .kind
            .or(forced)
            .unwrap_or_else(|| self.choose_kind(frame, index, decision.mode, distance, cabin));
        let kind = self.cap_cut(kind, frame.time, distance);
        let Some(spec) = self.catalog.get(index) else {
            return false;
        };
        self.editor
            .register(index, spec.as_ref(), frame, show, decision.counted);
        if kind == TransitionKind::Cut {
            self.last_cut = frame.time;
        }
        let seconds = transition_seconds(kind, frame, self.calm);
        tracing::debug!(
            target: "cinema",
            shot = spec.id(),
            tier = ?spec.tier(),
            mode = ?decision.mode,
            entry = ?kind,
            seconds,
            distance,
            energy = frame.intensity,
            subjects = frame.subjects.len(),
            "plan"
        );
        self.stats.shots += 1;
        self.stats.last_shot_at = frame.time;
        self.stats.last_mode = Some(decision.mode);
        self.stats.last_kind = Some(kind);
        self.stats.last_on_beat = on_beat;
        self.stats.last_beat_no = self.cutter.beat_no();
        self.transition.begin(kind, seconds, distance);
        self.run_transition(frame, true);
        true
    }

    /// Met à jour les deux plans (ils restent vivants) et mélange ; `fresh` : le plan entrant vient d'être mis à jour.
    fn run_transition(&mut self, frame: &CinemaFrame<'_>, fresh: bool) {
        if !fresh {
            if let Some(shot) = self.current.shot.as_mut() {
                shot.update(frame, &mut self.current.pose);
                self.current.steady(&mut self.limiter, &mut self.stats, frame);
            }
            if let Some(shot) = self.incoming.shot.as_mut() {
                shot.update(frame, &mut self.incoming.pose);
                self.incoming.steady(&mut self.limiter, &mut self.stats, frame);
            }
        }
        if !self
            .transition
            .step(frame.dt, &self.current.pose, &self.incoming.pose, &mut self.out)
        {
            return;
        }
        std::mem::swap(&mut self.current, &mut self.incoming);
        self.incoming.clear();
        self.current.started_at = frame.time;
        self.cutter.shot_started();
    }
}
