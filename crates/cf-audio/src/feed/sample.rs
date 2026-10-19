//! Conversion des formats d'échantillons d'entrée et mixage en mono, appelés depuis le callback temps réel.
//!
//! Tout ici est `#[inline]`, sans allocation ni branchement coûteux : ces fonctions tournent sur chaque trame reçue.

/// Format d'échantillon accepté en entrée de capture.
///
/// La tâche J1-03 demande i16, u16 et f32 ; i32 s'y ajoute parce que beaucoup d'interfaces USB professionnelles
/// n'exposent que ce format sous ALSA (accès direct `hw:`).
pub trait InputSample: Copy + Send + 'static {
    /// Valeur normalisée, 0 au repos, pleine échelle à ±1.
    fn to_f32(self) -> f32;
}

impl InputSample for f32 {
    #[inline]
    fn to_f32(self) -> f32 {
        // Un pilote défaillant peut livrer NaN ou l'infini : une seule valeur suffirait à empoisonner la FFT et toutes
        // les features suivantes. On la remplace par du silence plutôt que de la propager.
        if self.is_finite() { self } else { 0.0 }
    }
}

impl InputSample for i16 {
    #[inline]
    fn to_f32(self) -> f32 {
        f32::from(self) / 32_768.0
    }
}

impl InputSample for u16 {
    #[inline]
    fn to_f32(self) -> f32 {
        // Non signé : le silence est au milieu de la plage (32 768).
        (f32::from(self) - 32_768.0) / 32_768.0
    }
}

impl InputSample for i32 {
    #[inline]
    fn to_f32(self) -> f32 {
        // La perte de précision (24 bits de mantisse) est très en dessous du bruit de fond d'une entrée réelle.
        self as f32 / 2_147_483_648.0
    }
}

/// Moyenne des canaux d'une trame entrelacée (mixage mono).
///
/// La moyenne, plutôt que la somme, garde la pleine échelle à ±1 quel que soit le nombre de canaux. Une trame vide
/// donne 0.
#[inline]
pub fn mono_frame<S: InputSample>(frame: &[S]) -> f32 {
    match frame {
        [] => 0.0,
        [only] => only.to_f32(),
        [left, right] => f32::midpoint(left.to_f32(), right.to_f32()),
        _ => frame.iter().map(|s| s.to_f32()).sum::<f32>() / frame.len() as f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn integer_formats_map_to_unit_range() {
        assert!(close(0_i16.to_f32(), 0.0));
        assert!(close(i16::MIN.to_f32(), -1.0));
        assert!(close(i16::MAX.to_f32(), 32_767.0 / 32_768.0));
        assert!(close(32_768_u16.to_f32(), 0.0));
        assert!(close(0_u16.to_f32(), -1.0));
        assert!(close(u16::MAX.to_f32(), 32_767.0 / 32_768.0));
        assert!(close(0_i32.to_f32(), 0.0));
        assert!(close(i32::MIN.to_f32(), -1.0));
        assert!(close(i32::MAX.to_f32(), 1.0));
    }

    #[test]
    fn float_passes_through_and_rejects_non_finite() {
        assert!(close(0.25_f32.to_f32(), 0.25));
        assert!(close(1.5_f32.to_f32(), 1.5));
        assert!(close(f32::NAN.to_f32(), 0.0));
        assert!(close(f32::INFINITY.to_f32(), 0.0));
        assert!(close(f32::NEG_INFINITY.to_f32(), 0.0));
    }

    #[test]
    fn mono_mix_averages_channels() {
        assert!(close(mono_frame::<f32>(&[]), 0.0));
        assert!(close(mono_frame(&[0.5_f32]), 0.5));
        assert!(close(mono_frame(&[1.0_f32, 0.0]), 0.5));
        assert!(close(mono_frame(&[1.0_f32, -1.0]), 0.0));
        assert!(close(mono_frame(&[0.4_f32, 0.4, 0.4, 0.4]), 0.4));
        assert!(close(mono_frame(&[i16::MIN, i16::MIN, i16::MIN]), -1.0));
    }
}
