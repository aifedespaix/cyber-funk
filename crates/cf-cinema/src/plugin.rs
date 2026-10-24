//! La couche Bevy du réalisateur, mince : le monde remplit [`CinemaWorld`] en `CfSet::Simulate`, le réalisateur écrit
//! la [`CameraPose`] en `CfSet::Direct`, le rig de `cf-render` l'applique en `CfSet::Present`.
//!
//! ```text
//! CfSet::Simulate   le monde (NeonWorld…) ─▶ Res<CinemaWorld>
//! WorldSet::Subjects                        ─▶ Res<Subjects>
//! CfSet::Direct     direct::<W> : CinemaDirector::update(CinemaFrame) ─▶ Res<CameraPose>
//! CfSet::Present    rig de caméras (cf-render)
//! ```

use std::marker::PhantomData;

use bevy::prelude::*;
use cf_render::{CameraPose, CfSet, ShowRuntime, Subjects};

use crate::choreo::{NoShow, ShowSide};
use crate::director::CinemaDirector;
use crate::shots;
use crate::world::{CinemaFrame, CinemaWorld, LaunchMeteor, Obstacles};

/// Graine par défaut du réalisateur (rejouable) ; l'application peut en tirer une autre par soirée.
pub const DEFAULT_SEED: u32 = 0x5111_a6e0;

/// Étape du réalisateur dans `CfSet::Direct` : les systèmes qui lisent la pose de l'image se placent après.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CinemaSet;

/// Installe le réalisateur (catalogue, ressources). Le système qui l'appelle est ajouté par le monde, avec
/// [`CinemaWorldPlugin`] : c'est lui qui sait répondre aux questions de collision.
#[derive(Debug, Clone, Copy)]
pub struct CinemaPlugin {
    /// Graine du hasard du réalisateur.
    pub seed: u32,
    /// Mode calme : transitions plus lentes, pas de coupe « waouh ».
    pub calm: bool,
}

impl Default for CinemaPlugin {
    fn default() -> Self {
        Self {
            seed: DEFAULT_SEED,
            calm: false,
        }
    }
}

impl Plugin for CinemaPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(CinemaDirector::new(shots::catalog(), self.calm, self.seed))
            .init_resource::<CinemaWorld>()
            .init_resource::<CameraPose>()
            .add_message::<LaunchMeteor>()
            .configure_sets(Update, CinemaSet.in_set(CfSet::Direct));
    }
}

/// Branche le réalisateur sur le monde `W`, dont la ressource répond aux questions de collision. À ajouter par le
/// plugin du monde (ou l'application), après [`CinemaPlugin`].
#[derive(Debug)]
pub struct CinemaWorldPlugin<W>(PhantomData<fn() -> W>);

impl<W> Default for CinemaWorldPlugin<W> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<W: Obstacles + Resource> Plugin for CinemaWorldPlugin<W> {
    fn build(&self, app: &mut App) {
        app.init_resource::<CinemaWorld>()
            .init_resource::<CameraPose>()
            .add_message::<LaunchMeteor>()
            // `CinemaSet` est déjà dans `CfSet::Direct` (voir `CinemaPlugin`) : le redire ferait une arête redondante, que
            // Bevy signale en avertissement à chaque lancement.
            .add_systems(Update, direct::<W>.in_set(CinemaSet));
    }
}

/// `CfSet::Direct` : la pose de l'image. Démarre depuis la pose actuelle de la caméra au premier appel.
fn direct<W: Obstacles + Resource>(
    world: Res<CinemaWorld>,
    obstacles: Option<Res<W>>,
    subjects: Option<Res<Subjects>>,
    director: Option<ResMut<CinemaDirector>>,
    mut runtime: Option<ResMut<ShowRuntime>>,
    mut pose: ResMut<CameraPose>,
    mut no_show: Local<NoShow>,
    mut launches: MessageWriter<LaunchMeteor>,
) {
    let (Some(obstacles), Some(mut director)) = (obstacles, director) else {
        return;
    };
    let list = subjects.as_deref().map_or(&[][..], Subjects::as_slice);
    let frame = CinemaFrame::new(&world, &*obstacles, list);
    // Le vrai chef du spectacle quand un show est chargé (portes de rang, ordres, drop préparé, `strobe` selon les
    // flashs) ; sinon un spectacle vide : tout est permis, aucun ordre.
    let show: &mut dyn ShowSide = match runtime.as_mut() {
        Some(runtime) => runtime.director_mut(),
        None => &mut *no_show,
    };
    if !director.active() {
        let from = *pose;
        director.start_with(&from, &frame, show);
    }
    let next = *director.update_with(&frame, show);
    if director.take_meteor_launch() {
        launches.write(LaunchMeteor);
    }
    if *pose != next {
        *pose = next;
    }
}

#[cfg(test)]
mod tests {
    use bevy::math::DVec3;

    use super::*;
    use crate::world::NoObstacles;

    #[derive(Resource, Default)]
    struct Empty;

    impl Obstacles for Empty {
        fn collides(&self, at: DVec3, radius: f64) -> bool {
            NoObstacles.collides(at, radius)
        }
    }

    #[test]
    fn le_reealisateur_ecrit_la_pose_lue_par_le_rig() {
        let mut app = App::new();
        app.add_plugins((
            cf_render::CorePlugin,
            CinemaPlugin::default(),
            CinemaWorldPlugin::<Empty>::default(),
        ))
        .init_resource::<Empty>();
        app.world_mut().resource_mut::<CinemaWorld>().car = DVec3::new(0.0, 0.0, -1000.0);
        app.world_mut().resource_mut::<CinemaWorld>().dt = 0.016;
        for _ in 0..400 {
            app.update();
        }
        let pose = *app.world().resource::<CameraPose>();
        assert!(app.world().resource::<CinemaDirector>().active());
        // Le catalogue complet tourne : on vérifie le câblage (un plan est à l'antenne, la pose est saine et écartée de la
        // voiture), pas le plan tiré, qui dépend du tirage. Les bornes par plan sont dans `shots::tests`.
        assert!(app.world().resource::<CinemaDirector>().shot_id().is_some());
        let rel = pose.position - DVec3::new(0.0, 0.0, -1000.0);
        assert!(crate::pose::is_finite(&pose));
        assert!(rel.length() > 1.0, "{rel}");
    }

    #[test]
    fn la_chevauchee_d_un_astre_emet_l_ordre_lance_un_astre() {
        let mut app = App::new();
        app.add_plugins((
            cf_render::CorePlugin,
            CinemaPlugin::default(),
            CinemaWorldPlugin::<Empty>::default(),
        ))
        .init_resource::<Empty>();
        // Un catalogue d'un seul plan : la chevauchée, qui lance son astre faute d'en trouver un en vol.
        app.insert_resource(CinemaDirector::new(vec![Box::new(shots::MeteorRide)], false, 1));
        app.world_mut().resource_mut::<CinemaWorld>().car = DVec3::new(0.0, 0.0, -1000.0);
        app.world_mut().resource_mut::<CinemaWorld>().dt = 0.016;
        app.update();
        let launches = app.world().resource::<Messages<LaunchMeteor>>().len();
        assert_eq!(launches, 1, "un seul ordre par plan fabriqué");
        app.update();
        app.update();
        // Le message se lit sur deux images : on s'assure surtout qu'aucun second ordre n'est parti.
        assert!(app.world().resource::<Messages<LaunchMeteor>>().len() <= 1);
    }
}
