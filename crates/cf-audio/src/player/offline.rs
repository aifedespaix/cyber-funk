//! Décodage hors ligne d'une piste entière en PCM mono, pour l'analyse de fond (grille de temps en cache).
//!
//! Même décodeur et même rééchantillonneur que la lecture, donc la même durée et les mêmes échantillons que ceux que
//! l'auditeur entend. Le PCM n'est jamais gardé en entier : il passe par blocs au visiteur.

use std::path::Path;

use super::error::Result;
use super::source::TrackSource;

/// Décode `path` et donne au visiteur le mono (moyenne gauche/droite) à la fréquence `rate_hz`, par blocs successifs.
/// Rend le nombre total d'échantillons livrés.
///
/// # Errors
/// Fichier introuvable ou illisible, format non géré, erreur de décodage.
pub fn decode_mono(path: &Path, rate_hz: u32, mut visit: impl FnMut(&[f32])) -> Result<u64> {
    let mut source = TrackSource::open(path, rate_hz)?;
    let mut stereo: Vec<f32> = Vec::new();
    let mut mono: Vec<f32> = Vec::new();
    let mut total = 0_u64;
    loop {
        stereo.clear();
        let more = source.read(&mut stereo)?;
        mono.clear();
        mono.extend(stereo.as_chunks::<2>().0.iter().map(|f| f32::midpoint(f[0], f[1])));
        if !mono.is_empty() {
            total += mono.len() as u64;
            visit(&mono);
        }
        if !more {
            return Ok(total);
        }
    }
}
