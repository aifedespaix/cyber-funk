//! Caméra libre de préparation (J8-06, `free-camera.ts`) : cadrer et déboguer, jamais en live.
//!
//! ```text
//! CfSet::Direct ─▶ freecam (après le réalisateur, avant CfSet::Present) ─▶ CameraPose ─▶ rig de cf-render
//! ```
//!
//! Le réalisateur continue de tourner dessous : à la sortie, la caméra revient en douceur vers sa pose du moment
//! plutôt que de sauter. Rien ici n'écrit de lumière : la règle 7 (limiteur photosensible) n'est pas concernée.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::math::DVec3;
use bevy::prelude::*;
use cf_render::{CameraPose, CfSet, TextInputFocus};

use crate::live::OperatorMode;

/// Touche qui prend ou rend la caméra.
const TOGGLE_KEY: KeyCode = KeyCode::F2;
/// Vitesse de base (m/s) ; Maj la multiplie.
const SPEED: f64 = 30.0;
const FAST_FACTOR: f64 = 6.0;
/// Radians par pixel de souris, bouton droit enfoncé.
const LOOK_SENSITIVITY: f32 = 0.0025;
/// Durée du retour au réalisateur (s).
const RETURN_SECONDS: f32 = 0.8;
/// Limite de tangage : évite le retournement aux pôles.
const MAX_PITCH: f32 = 1.55;

/// Où en est la caméra libre.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct FreeCam {
    phase: Phase,
    pose: CameraPose,
    yaw: f32,
    pitch: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
enum Phase {
    #[default]
    Off,
    /// La caméra est à l'opérateur.
    Held,
    /// Retour vers le réalisateur ; `t` en secondes écoulées.
    Returning { t: f32 },
}

impl FreeCam {
    /// Prend la caméra à partir de la pose courante du réalisateur.
    fn grab(&mut self, from: &CameraPose) {
        let (yaw, pitch, _) = from.rotation.to_euler(EulerRot::YXZ);
        self.pose = *from;
        self.yaw = yaw;
        self.pitch = pitch.clamp(-MAX_PITCH, MAX_PITCH);
        self.phase = Phase::Held;
    }

    /// Rend la caméra : retour en douceur, ou immédiat si on n'avait pas la main.
    fn release(&mut self) {
        if self.phase == Phase::Held {
            self.phase = Phase::Returning { t: 0.0 };
        }
    }

    /// Coupe net (passage en Live : jamais de caméra libre, pas de retour animé).
    fn cut(&mut self) {
        self.phase = Phase::Off;
    }

    fn look(&mut self, delta: Vec2) {
        self.yaw -= delta.x * LOOK_SENSITIVITY;
        self.pitch = (self.pitch - delta.y * LOOK_SENSITIVITY).clamp(-MAX_PITCH, MAX_PITCH);
        self.pose.rotation = Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0);
    }

    /// `wish` : x droite, y haut, z arrière (−z devant, repère de Sillage).
    fn fly(&mut self, wish: Vec3, fast: bool, dt: f32) {
        if wish == Vec3::ZERO {
            return;
        }
        let speed = if fast { SPEED * FAST_FACTOR } else { SPEED };
        let yaw_only = Quat::from_rotation_y(self.yaw);
        let pitched = self.pose.rotation;
        // Avant/arrière suivent le regard complet, latéral reste horizontal, montée verticale absolue.
        let dir = pitched * Vec3::new(0.0, 0.0, wish.z) + yaw_only * Vec3::new(wish.x, 0.0, 0.0) + Vec3::Y * wish.y;
        self.pose.position += DVec3::from(dir.normalize_or_zero()) * speed * f64::from(dt);
    }

    /// Fait avancer le retour ; renvoie la pose à appliquer, ou `None` si la main est rendue.
    fn returning(&mut self, director: &CameraPose, dt: f32) -> Option<CameraPose> {
        let Phase::Returning { t } = &mut self.phase else {
            return None;
        };
        *t += dt;
        if *t >= RETURN_SECONDS {
            self.phase = Phase::Off;
            return None;
        }
        let k = smoothstep(*t / RETURN_SECONDS);
        Some(blend(&self.pose, director, k))
    }
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Mélange deux poses : `k = 0` donne `a`, `k = 1` donne `b`.
fn blend(a: &CameraPose, b: &CameraPose, k: f32) -> CameraPose {
    CameraPose {
        position: a.position.lerp(b.position, f64::from(k)),
        rotation: a.rotation.slerp(b.rotation, k),
        fov_deg: a.fov_deg + (b.fov_deg - a.fov_deg) * k,
        cabin: b.cabin,
        sky_drift: a.sky_drift.lerp(b.sky_drift, k),
    }
}

/// Branche la caméra libre. Appelé par `OperatorPlugin`.
pub(crate) fn build(app: &mut App) {
    app.init_resource::<FreeCam>()
        .add_systems(Update, drive.after(CfSet::Direct).before(CfSet::Present));
}

#[allow(clippy::too_many_arguments)] // Paramètres système Bevy : un par ressource lue.
fn drive(
    time: Res<Time>,
    mode: Res<OperatorMode>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    text_focus: Res<TextInputFocus>,
    mut cam: ResMut<FreeCam>,
    mut pose: ResMut<CameraPose>,
) {
    // Inaccessible en Live : si on y bascule caméra en main, on la rend sans transition.
    if *mode != OperatorMode::Preparation {
        cam.cut();
        return;
    }
    let dt = time.delta_secs();
    let typing = text_focus.0;
    if keys.just_pressed(TOGGLE_KEY) && !typing {
        match cam.phase {
            Phase::Off | Phase::Returning { .. } => cam.grab(&pose),
            Phase::Held => cam.release(),
        }
    }
    match cam.phase {
        Phase::Off => {}
        Phase::Held => {
            if mouse.pressed(MouseButton::Right) {
                cam.look(motion.delta);
            }
            if !typing {
                let axis =
                    |plus: KeyCode, minus: KeyCode| f32::from(keys.pressed(plus)) - f32::from(keys.pressed(minus));
                // Touches physiques : W/A/S/D couvrent aussi Z/Q/S/D en AZERTY.
                let wish = Vec3::new(
                    axis(KeyCode::KeyD, KeyCode::KeyA),
                    axis(KeyCode::Space, KeyCode::ControlLeft),
                    axis(KeyCode::KeyS, KeyCode::KeyW),
                );
                cam.fly(wish, keys.pressed(KeyCode::ShiftLeft), dt);
            }
            *pose = cam.pose;
        }
        Phase::Returning { .. } => {
            let director = *pose;
            if let Some(blended) = cam.returning(&director, dt) {
                *pose = blended;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f64) -> CameraPose {
        CameraPose {
            position: DVec3::new(x, 0.0, 0.0),
            ..CameraPose::default()
        }
    }

    #[test]
    fn avance_vers_l_avant_le_long_de_moins_z() {
        let mut cam = FreeCam::default();
        cam.grab(&at(0.0));
        cam.fly(Vec3::new(0.0, 0.0, -1.0), false, 1.0);
        assert!((cam.pose.position.z + SPEED).abs() < 1e-6);
    }

    #[test]
    fn le_tangage_est_borne() {
        let mut cam = FreeCam::default();
        cam.grab(&at(0.0));
        cam.look(Vec2::new(0.0, -1.0e6));
        assert!(cam.pitch <= MAX_PITCH);
    }

    #[test]
    fn le_retour_rejoint_le_realisateur_puis_rend_la_main() {
        let mut cam = FreeCam::default();
        cam.grab(&at(100.0));
        cam.release();
        let director = at(0.0);
        let mid = cam.returning(&director, RETURN_SECONDS * 0.5).expect("en retour");
        assert!(mid.position.x > 0.0 && mid.position.x < 100.0);
        assert!(cam.returning(&director, RETURN_SECONDS).is_none());
        assert_eq!(cam.phase, Phase::Off);
    }

    #[test]
    fn le_live_coupe_net() {
        let mut cam = FreeCam::default();
        cam.grab(&at(1.0));
        cam.cut();
        assert_eq!(cam.phase, Phase::Off);
    }
}
