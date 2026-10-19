//! Origine flottante : la logique pense en absolu (`f64`), le rendu en relatif (`f32`) autour d'une origine proche.
//!
//! Repris de `world-origin.ts` (Sillage) : « la voiture file vers −z sans jamais revenir, et `carZ` atteint vite
//! −100 000 puis −400 000 en fin de set. Les shaders qui reçoivent des positions monde en float32 y perdent le
//! centimètre (≈ 3 cm à 300 km) et leurs `fract` / `mod` / bruits se mettent à trembler. »
//!
//! Sillage corrigeait shader par shader (uniform `uOrigin`). Ici le remède est global : tout ce qui est rendu vit en
//! coordonnées relatives à [`FloatingOrigin`], et l'origine saute par multiples de [`ORIGIN_STEP`] quand le point suivi
//! ([`StreamFocus`]) s'en éloigne. Les shaders reçoivent donc d'office des positions monde petites (≤ 15,4 km) : plus
//! besoin d'uniform d'origine, et les motifs réguliers restent continus au saut (voir [`ORIGIN_STEP`]).
//!
//! Qui fait quoi :
//! - la logique écrit des [`AbsolutePosition`] (`f64`) ; le système [`sync_absolute_positions`] en déduit la
//!   translation du `Transform` (`f32`) en fin d'image, avant la propagation des transforms ;
//! - les systèmes qui gardent des positions rendues en cache (particules, traînées) les décalent avec un
//!   [`OriginTracker`] ;
//! - une entité qui porte une [`AbsolutePosition`] doit être une racine (sans `ChildOf`) : ses enfants restent en
//!   coordonnées locales et suivent leur parent.

use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::query::QuerySingleError;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crate::world::WorldSet;

/// Pas de l'origine, en m (repris tel quel de `world-origin.ts`).
///
/// « `ORIGIN_STEP` est un multiple de 9, 24, 100, 8 et 5 : les motifs réguliers de la route (tirets, joints, bandes,
/// gouttes) se prolongent sans à-coup au changement d'origine. Seuls les bruits aléatoires (grain, flaques, brume) se
/// redistribuent d'un coup, environ toutes les quatorze minutes à pleine allure. »
///
/// Un shader qui veut un motif continu au saut choisit donc une période qui divise 28 800 m.
pub const ORIGIN_STEP: f64 = 28_800.0;

/// Marge au-delà du demi-pas avant de recentrer, en m.
///
/// Sillage n'en avait pas (son origine ne servait qu'aux shaders, gratuite à changer). Ici un recentrage réécrit la
/// position de chaque racine du monde : sans marge, une caméra qui oscille autour de la frontière (±14,4 km) ferait
/// basculer l'origine à chaque image. 1 000 m gardent la position rendue sous [`MAX_RENDER_OFFSET_M`].
pub const ORIGIN_HYSTERESIS_M: f64 = 1_000.0;

/// Distance maximale entre le point suivi et l'origine, par axe, en m : 14 400 + 1 000 = 15 400 m.
///
/// En dessous de 16 384 m (2¹⁴), l'écart entre deux `f32` voisins vaut 2⁻¹⁰ m ≈ 1 mm : c'est la précision du monde
/// rendu près de la caméra, quelle que soit la distance parcourue. À 400 km sans origine flottante, il vaut 2⁻⁵ m
/// ≈ 3 cm, assez pour faire trembler un objet à quelques mètres de l'objectif.
pub const MAX_RENDER_OFFSET_M: f64 = ORIGIN_STEP / 2.0 + ORIGIN_HYSTERESIS_M;

/// Origine des motifs la plus proche de `x` (un axe) : multiple de [`ORIGIN_STEP`] (`originFor` de Sillage).
#[must_use]
pub fn origin_for(x: f64) -> f64 {
    // `+ 0.0` comme le `+ 0` de Sillage : évite une origine « −0 » près du départ.
    (x / ORIGIN_STEP).round() * ORIGIN_STEP + 0.0
}

/// Nouvelle origine si `focus` s'est trop éloigné de `origin` sur un axe (voir [`ORIGIN_HYSTERESIS_M`]), sinon `None`.
///
/// La nouvelle origine est l'origine des motifs la plus proche du point suivi, axe par axe.
#[must_use]
pub fn recenter_target(origin: DVec3, focus: DVec3) -> Option<DVec3> {
    let far = (focus - origin).abs().max_element() > MAX_RENDER_OFFSET_M;
    far.then(|| DVec3::new(origin_for(focus.x), origin_for(focus.y), origin_for(focus.z)))
}

/// L'origine du monde rendu, en coordonnées absolues (m).
///
/// Écrite par un seul système ([`recenter_origin`], en tout début d'image) : toute l'image convertit donc avec la même
/// origine. Elle ne change que lors d'un recentrage, ce qui rend `Res::is_changed` exploitable par les lecteurs.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct FloatingOrigin {
    origin: DVec3,
    shifts: u32,
}

impl FloatingOrigin {
    /// Position absolue (m) du zéro du monde rendu.
    #[must_use]
    pub fn origin(&self) -> DVec3 {
        self.origin
    }

    /// Nombre de recentrages depuis le démarrage (diagnostic).
    #[must_use]
    pub fn shifts(&self) -> u32 {
        self.shifts
    }

    /// Position absolue → position rendue (`f32`, relative à l'origine).
    #[must_use]
    pub fn to_render(&self, absolute: DVec3) -> Vec3 {
        (absolute - self.origin).as_vec3()
    }

    /// Position rendue → position absolue.
    #[must_use]
    pub fn to_absolute(&self, render: Vec3) -> DVec3 {
        self.origin + render.as_dvec3()
    }

    /// Recentre si besoin ; renvoie le décalage appliqué aux positions rendues (`nouvelle = ancienne + décalage`).
    pub fn recenter(&mut self, focus: DVec3) -> Option<Vec3> {
        let target = recenter_target(self.origin, focus)?;
        let shift = (self.origin - target).as_vec3();
        self.origin = target;
        self.shifts = self.shifts.wrapping_add(1);
        Some(shift)
    }
}

/// Position absolue (m, `f64`) d'une entité racine du monde. La translation de son `Transform` en est déduite.
///
/// La rotation et l'échelle du `Transform` restent à la charge du propriétaire de l'entité.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default, Deref, DerefMut)]
#[require(Transform)]
pub struct AbsolutePosition(pub DVec3);

/// Le point que suivent la fenêtre de tronçons et l'origine flottante : la voiture, ou la caméra pour un monde sans
/// voiture. Une seule entité le porte, avec une [`AbsolutePosition`].
#[derive(Component, Debug, Clone, Copy, Default)]
#[require(AbsolutePosition)]
pub struct StreamFocus;

/// Pour un système qui garde des positions rendues (`f32`) en cache : renvoie le décalage à leur appliquer depuis
/// son dernier passage. À garder dans un `Local<OriginTracker>`.
///
/// Robuste à plusieurs recentrages entre deux passages (il compare les origines, il ne compte pas les événements).
#[derive(Debug, Clone, Copy, Default)]
pub struct OriginTracker {
    last: Option<DVec3>,
}

impl OriginTracker {
    /// Décalage à ajouter aux positions rendues gardées depuis le dernier appel, ou `None` si l'origine n'a pas bougé
    /// (et au premier appel, qui ne fait que mémoriser l'origine).
    pub fn take_shift(&mut self, origin: &FloatingOrigin) -> Option<Vec3> {
        let now = origin.origin();
        let before = self.last.replace(now)?;
        (before != now).then(|| (before - now).as_vec3())
    }
}

/// Début d'image : recentre l'origine si le point suivi s'en est trop éloigné.
///
/// Sans point suivi (ou s'il y en a plusieurs, ce qui est signalé une fois), l'origine ne bouge pas.
pub fn recenter_origin(
    mut origin: ResMut<FloatingOrigin>,
    focus: Query<&AbsolutePosition, With<StreamFocus>>,
    mut warned: Local<bool>,
) {
    let focus = match focus.single() {
        Ok(focus) => focus,
        Err(QuerySingleError::MultipleEntities(_)) => {
            if !std::mem::replace(&mut *warned, true) {
                warn!("plusieurs entités portent StreamFocus : origine et tronçons restent immobiles");
            }
            return;
        }
        Err(QuerySingleError::NoEntities(_)) => return,
    };
    // Lecture sans marquer la ressource modifiée : `is_changed` ne doit signaler que les vrais recentrages.
    if recenter_target(origin.origin(), focus.0).is_some() {
        origin.recenter(focus.0);
    }
}

/// Fin d'image : translation des `Transform` = position absolue − origine.
///
/// Ne réécrit que les entités dont la position absolue a changé, sauf l'image d'un recentrage où tout est réécrit.
pub fn sync_absolute_positions(
    origin: Res<FloatingOrigin>,
    mut query: Query<(Ref<AbsolutePosition>, &mut Transform), Without<ChildOf>>,
) {
    let all = origin.is_changed();
    for (absolute, mut transform) in &mut query {
        if all || absolute.is_changed() {
            transform.translation = origin.to_render(absolute.0);
        }
    }
}

/// Origine flottante : ressource, recentrage en début d'image et synchronisation des `Transform` en fin d'image.
///
/// Ajouté automatiquement par `WorldPlugin`.
#[derive(Debug, Default)]
pub struct OriginPlugin;

impl Plugin for OriginPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FloatingOrigin>()
            .configure_sets(Update, WorldSet::Origin.before(crate::CfSet::Input))
            .configure_sets(PostUpdate, WorldSet::Sync.before(TransformSystems::Propagate))
            .add_systems(Update, recenter_origin.in_set(WorldSet::Origin))
            .add_systems(PostUpdate, sync_absolute_positions.in_set(WorldSet::Sync));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_for_reprend_sillage() {
        assert!(origin_for(0.0).abs() < f64::EPSILON);
        assert!(origin_for(-14_000.0).abs() < f64::EPSILON);
        assert!((origin_for(-14_500.0) + ORIGIN_STEP).abs() < f64::EPSILON);
        assert!((origin_for(-400_000.0) + 14.0 * ORIGIN_STEP).abs() < f64::EPSILON);
        // Pas de « −0 » au départ.
        assert!(origin_for(-1.0).is_sign_positive());
    }

    #[test]
    fn le_pas_prolonge_les_motifs_de_la_route() {
        // Périodes des motifs de `road.ts` : tirets (9), joints (24), bandes (100), gouttes (8), grain (5).
        for period in [9.0, 24.0, 100.0, 8.0, 5.0] {
            assert!(
                (ORIGIN_STEP % period).abs() < f64::EPSILON,
                "{period} ne divise pas ORIGIN_STEP"
            );
        }
    }

    #[test]
    fn hysteresis_pas_de_bascule_a_la_frontiere() {
        let mut origin = FloatingOrigin::default();
        // On oscille autour de la frontière du demi-pas : aucune bascule tant qu'on reste dans la marge.
        for k in 0..200 {
            let z = -ORIGIN_STEP / 2.0 + if k % 2 == 0 { 400.0 } else { -400.0 };
            assert!(origin.recenter(DVec3::new(0.0, 0.0, z)).is_none());
        }
        assert_eq!(origin.shifts(), 0);
    }

    #[test]
    fn quatre_cents_km_restent_proches_de_l_origine() {
        let mut origin = FloatingOrigin::default();
        let mut z = 0.0;
        while z > -400_000.0 {
            z -= 37.3;
            let focus = DVec3::new(3.0, 12.0, z);
            origin.recenter(focus);
            let offset = (focus - origin.origin()).abs().max_element();
            assert!(offset <= MAX_RENDER_OFFSET_M, "point suivi à {offset} m de l'origine");
            assert!((origin.origin().z % ORIGIN_STEP).abs() < f64::EPSILON);
        }
        // 400 km / 28,8 km : 14 recentrages.
        assert_eq!(origin.shifts(), 14);
    }

    /// Le tremblement à 400 km, chiffré : écart (caméra → objet) calculé comme le GPU, en `f32` dans le repère rendu,
    /// comparé à l'exact en `f64`. Un pixel 1080p à 60° de champ vaut ~1 mrad, soit 5 mm à 5 m de l'objectif.
    #[test]
    fn precision_a_400_km_avec_et_sans_origine_flottante() {
        let worst = |origin: &FloatingOrigin, start: f64| {
            let mut worst = 0.0_f64;
            for k in 0..2_000 {
                let car = DVec3::new(0.37, 0.61, start - f64::from(k) * 0.731);
                let camera = car + DVec3::new(1.3, 2.7, 6.1);
                let exact = car - camera;
                let rendered = origin.to_render(car) - origin.to_render(camera);
                worst = worst.max((rendered.as_dvec3() - exact).length());
            }
            worst
        };
        let start = -400_000.0;
        let mut floating = FloatingOrigin::default();
        floating.recenter(DVec3::new(0.0, 0.0, start));
        let with = worst(&floating, start);
        let without = worst(&FloatingOrigin::default(), start);
        assert!(with < 0.001, "avec origine flottante : {with} m");
        assert!(without > 0.01, "sans origine flottante : {without} m");
    }

    #[test]
    fn le_tracker_rend_le_decalage_une_seule_fois() {
        let mut origin = FloatingOrigin::default();
        let mut tracker = OriginTracker::default();
        assert!(tracker.take_shift(&origin).is_none());
        origin.recenter(DVec3::new(0.0, 0.0, -20_000.0));
        let shift = tracker.take_shift(&origin).expect("un recentrage a eu lieu");
        assert!((shift.z - 28_800.0).abs() < f32::EPSILON);
        assert!(tracker.take_shift(&origin).is_none());
    }

    #[derive(Component)]
    struct Probe;

    #[test]
    fn transforms_suivent_l_origine() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, OriginPlugin));
        app.world_mut()
            .spawn((AbsolutePosition(DVec3::new(0.0, 0.0, -20_000.0)), StreamFocus));
        let probe = app
            .world_mut()
            .spawn((AbsolutePosition(DVec3::new(1.0, 2.0, -20_010.0)), Probe))
            .id();
        app.update();
        // Recentrage dès la première image : -20 000 est au-delà de la marge.
        let origin = app.world().resource::<FloatingOrigin>().origin();
        assert!((origin.z + ORIGIN_STEP).abs() < f64::EPSILON);
        let translation = app.world().get::<Transform>(probe).expect("transform").translation;
        assert!((translation - Vec3::new(1.0, 2.0, 8_790.0)).length() < 1e-3);
    }
}
