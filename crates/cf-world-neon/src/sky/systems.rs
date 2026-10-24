//! Les systèmes du ciel : naissance au démarrage, animation (astres, étoiles filantes), matériaux d'après l'image.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use cf_render::{AudioFeatures, CurrentShowFx, ScenePalette};

use super::material::{
    AuroraMaterial, DomeMaterial, PlanetMaterial, PlanetParams, RingMaterial, SkyParams, StarsMaterial, StreakMaterial,
};
use super::params::{AURORAS, SkyFrame, build_aurora, build_dome, build_planet, build_ring, build_stars, build_streak};
use super::shooting::{POOL, ShootingStars};
use super::spec::{PLANETS, RADIUS_M, RING_PLANET_TEXTURE, RING_TILT_X_RAD};
use super::stars::{star_field, star_mesh};
use super::textures;
use super::{PlanetTextures, RingPlanet, ShootingStarRequest, SkyLook, SkyPart, SkyPlanet, SkyStreak};

/// Un coup de kick sur ce nombre lance une étoile filante (`b.count % 24 === 0` de `sillage-scene.ts`).
const KICKS_PER_SHOOTING_STAR: u32 = 24;

/// L'état animé du ciel : étoiles filantes, rotation des astres, dernier compteur de kicks vu.
///
/// Écrit par le seul `step_sky`.
#[derive(Resource, Debug, Clone)]
pub struct SkyState {
    shooting: ShootingStars,
    spin_rad: [f32; PLANETS.len()],
    last_kick_count: Option<u32>,
}

impl Default for SkyState {
    fn default() -> Self {
        Self {
            shooting: ShootingStars::new(),
            spin_rad: [0.0; PLANETS.len()],
            last_kick_count: None,
        }
    }
}

impl SkyState {
    /// Remet le ciel animé à son état de départ : astres à leur angle initial, pool vide, minuteur d'étoile filante
    /// relancé. À appeler à l'ouverture d'un show ; les captures scriptées s'en servent pour que chaque scène parte du
    /// même instant.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Les étoiles filantes.
    #[must_use]
    pub fn shooting(&self) -> &ShootingStars {
        &self.shooting
    }

    /// Angle de rotation de l'astre `index` sur son axe, en rad, dans [0, 2π).
    #[must_use]
    pub fn spin_rad(&self, index: usize) -> f32 {
        self.spin_rad.get(index).copied().unwrap_or(0.0)
    }
}

/// Les matériaux du ciel, rangés par type.
#[derive(SystemParam)]
pub(super) struct SkyMaterials<'w> {
    dome: ResMut<'w, Assets<DomeMaterial>>,
    stars: ResMut<'w, Assets<StarsMaterial>>,
    aurora: ResMut<'w, Assets<AuroraMaterial>>,
    planet: ResMut<'w, Assets<PlanetMaterial>>,
    ring: ResMut<'w, Assets<RingMaterial>>,
    streak: ResMut<'w, Assets<StreakMaterial>>,
}

/// Les poignées des matériaux, pour les réécrire à chaque image sans rien créer.
#[derive(Resource, Debug)]
pub(super) struct SkyRig {
    dome: Handle<DomeMaterial>,
    stars: Handle<StarsMaterial>,
    auroras: [Handle<AuroraMaterial>; AURORAS.len()],
    /// Indice dans [`PLANETS`] et matériau de chaque astre chargé.
    planets: Vec<(usize, Handle<PlanetMaterial>)>,
    /// Indice dans [`PLANETS`] et matériau de chaque anneau.
    rings: Vec<(usize, Handle<RingMaterial>)>,
    streaks: [Handle<StreakMaterial>; POOL],
}

/// Écrit les uniformes d'un matériau, seulement s'ils ont changé : Bevy ne re-prépare pas un asset qu'on n'a pas touché.
fn write<M: SkyParams>(assets: &mut Assets<M>, handle: &Handle<M>, params: M::Params) {
    if assets.get(handle).is_some_and(|m| *m.params() == params) {
        return;
    }
    if let Some(mut material) = assets.get_mut(handle) {
        material.set_params(params);
    }
}

/// Crée le ciel, une fois, au démarrage : le dôme, les étoiles, les aurores, les astres (et leurs anneaux), le pool de
/// traits.
///
/// Les textures sont décodées ici, à l'ouverture (règle 8 de `CLAUDE.md`). Un astre dont la texture ne se décode pas est
/// omis, avec un avertissement : le ciel continue sans lui.
pub(super) fn spawn_sky(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: SkyMaterials,
) {
    let dome = materials.dome.add(DomeMaterial::default());
    commands.spawn((
        Name::new("ciel · dôme"),
        SkyPart,
        Mesh3d(meshes.add(Sphere::new(RADIUS_M).mesh().uv(40, 24))),
        MeshMaterial3d(dome.clone()),
    ));

    let stars = materials.stars.add(StarsMaterial::default());
    commands.spawn((
        Name::new("ciel · étoiles"),
        SkyPart,
        Mesh3d(meshes.add(star_mesh(&star_field()))),
        MeshMaterial3d(stars.clone()),
    ));

    // Aurores lointaines : basses sur l'horizon, très étirées, discrètes.
    let auroras = AURORAS.each_ref().map(|spec| {
        let handle = materials.aurora.add(AuroraMaterial::default());
        commands.spawn((
            Name::new("ciel · aurore"),
            SkyPart,
            Mesh3d(meshes.add(Rectangle::new(spec.size.x, spec.size.y).mesh().build())),
            MeshMaterial3d(handle.clone()),
            Transform::from_translation(spec.at),
        ));
        handle
    });

    let sphere = meshes.add(Sphere::new(1.0).mesh().uv(64, 40));
    let mut planets = Vec::new();
    let mut rings = Vec::new();
    let mut loaded = PlanetTextures::default();
    for (index, spec) in PLANETS.iter().enumerate() {
        let image = match textures::load(spec.texture) {
            Ok(image) => image,
            Err(error) => {
                // Comme Sillage : un astre dont la texture manque est simplement omis.
                tracing::warn!(%error, planet = spec.texture, "astre omis du ciel");
                continue;
            }
        };
        let map = images.add(image);
        loaded.0.push(map.clone());
        let material = materials.planet.add(PlanetMaterial {
            params: PlanetParams::default(),
            map: Some(map),
        });
        let center = spec.center();
        commands.spawn((
            Name::new(format!("ciel · astre {}", spec.texture)),
            SkyPlanet(index),
            Mesh3d(sphere.clone()),
            MeshMaterial3d(material.clone()),
            Transform {
                translation: center,
                rotation: spec.rotation(0.0),
                scale: Vec3::splat(spec.radius_m),
            },
        ));
        planets.push((index, material));

        if let Some(ring) = spec.ring {
            let mesh = Annulus::new(spec.radius_m * ring.inner, spec.radius_m * ring.outer)
                .mesh()
                .resolution(128)
                .build()
                .rotated_by(Quat::from_rotation_x(RING_TILT_X_RAD));
            let material = materials.ring.add(RingMaterial::default());
            commands.spawn((
                Name::new(format!("ciel · anneau {}", spec.texture)),
                SkyPart,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material.clone()),
                Transform::from_translation(center).with_rotation(Quat::from_rotation_z(spec.tilt_rad)),
            ));
            rings.push((index, material));
            if spec.texture == RING_PLANET_TEXTURE
                && let Some(giant) = RingPlanet::from_spec(spec)
            {
                commands.insert_resource(giant);
            }
        }
    }
    commands.insert_resource(loaded);

    let quad = meshes.add(Rectangle::new(1.0, 1.0).mesh().build());
    let streaks = std::array::from_fn::<_, POOL, _>(|i| {
        let handle = materials.streak.add(StreakMaterial::default());
        commands.spawn((
            Name::new("ciel · étoile filante"),
            SkyStreak(i),
            Mesh3d(quad.clone()),
            MeshMaterial3d(handle.clone()),
            Visibility::Hidden,
        ));
        handle
    });

    commands.insert_resource(SkyRig {
        dome,
        stars,
        auroras,
        planets,
        rings,
        streaks,
    });
}

/// `CfSet::Simulate` : tourne les astres, lance et fait voler les étoiles filantes.
///
/// Une étoile part toute seule toutes les quelques secondes, une sur les `KICKS_PER_SHOOTING_STAR` kicks (sauf flashs
/// coupés ou mode respiration d'un show), et sur [`ShootingStarRequest`]. La cadence maximale de
/// [`ShootingStars::launch`] borne le tout.
pub(super) fn step_sky(
    time: Res<Time>,
    look: Res<SkyLook>,
    features: Option<Res<AudioFeatures>>,
    show_fx: Option<Res<CurrentShowFx>>,
    mut requests: MessageReader<ShootingStarRequest>,
    mut state: ResMut<SkyState>,
    mut planets: Query<(&SkyPlanet, &mut Transform), Without<SkyStreak>>,
    mut streaks: Query<(&SkyStreak, &mut Transform, &mut Visibility), Without<SkyPlanet>>,
) {
    let dt = time.delta_secs();
    let state = &mut *state;

    if let Some(features) = features {
        let count = features.0.count;
        // Sans spectacle (démo), les kicks lancent des étoiles ; avec un spectacle, seulement si les flashs sont permis
        // et hors mode respiration : une étoile qui part sur le temps est un événement lumineux rythmé.
        let allowed = show_fx.is_none_or(|fx| fx.flashes && !fx.breathing);
        if let Some(last) = state.last_kick_count
            && allowed
            && count / KICKS_PER_SHOOTING_STAR > last / KICKS_PER_SHOOTING_STAR
        {
            state.shooting.launch();
        }
        state.last_kick_count = Some(count);
    }
    for _ in requests.read() {
        state.shooting.launch();
    }
    state.shooting.update(dt, look.calm);

    for (planet, mut transform) in &mut planets {
        let spec = &PLANETS[planet.0];
        let spin = &mut state.spin_rad[planet.0];
        *spin = (*spin + spec.spin_step_rad(dt)) % std::f32::consts::TAU;
        transform.rotation = spec.rotation(*spin);
    }
    for (streak, mut transform, mut visibility) in &mut streaks {
        let flying = &state.shooting.streaks()[streak.0];
        let target = if flying.is_free() {
            Visibility::Hidden
        } else {
            let (translation, rotation, scale) = flying.pose();
            *transform = Transform {
                translation,
                rotation,
                scale,
            };
            Visibility::Visible
        };
        if *visibility != target {
            *visibility = target;
        }
    }
}

/// `CfSet::Present` : remplit les matériaux (temps, pouls, palette, fondu). N'écrit que ce qui a changé.
pub(super) fn update_materials(
    time: Res<Time<Real>>,
    look: Res<SkyLook>,
    features: Option<Res<AudioFeatures>>,
    palette: Res<ScenePalette>,
    rig: Res<SkyRig>,
    state: Res<SkyState>,
    mut materials: SkyMaterials,
) {
    let frame = SkyFrame {
        time_s: look.clock_s.unwrap_or_else(|| time.elapsed_secs_f64()) as f32,
        pulse: features.map_or(0.0, |f| f.0.pulse),
        reveal: look.reveal.clamp(0.0, 1.0),
    };
    write(&mut materials.dome, &rig.dome, build_dome(&palette, &frame));
    write(&mut materials.stars, &rig.stars, build_stars(&palette, &frame));
    for (handle, spec) in rig.auroras.iter().zip(&AURORAS) {
        write(
            &mut materials.aurora,
            handle,
            build_aurora(spec, look.calm, &palette, &frame),
        );
    }
    for (index, handle) in &rig.planets {
        write(&mut materials.planet, handle, build_planet(*index, &palette, &frame));
    }
    for (index, handle) in &rig.rings {
        if let Some(params) = build_ring(*index, &palette, &frame) {
            write(&mut materials.ring, handle, params);
        }
    }
    for (handle, streak) in rig.streaks.iter().zip(state.shooting.streaks()) {
        write(
            &mut materials.streak,
            handle,
            build_streak(streak.tint, streak.alpha(), &palette, &frame),
        );
    }
}
