//! Choix d'une piste de démo par son nom, pour `measure_sync { track }`.
//!
//! Le lecteur de démo se pilote par indice (`AudioCommand::DemoSelect`), pas par nom ; la sonde fait la traduction à
//! partir de la liste que le pont audio publie (`AudioStatus::tracks`, dans l'ordre du lecteur).

use cf_audio::playlist::Track;

/// Cherche la piste désignée par `name` et rend son indice.
///
/// Sans casse. Un nom qui est exactement un titre, ou « artiste - titre », gagne ; sinon un fragment doit ne
/// correspondre qu'à une piste (jamais un choix silencieux parmi plusieurs). Les pistes illisibles ne sont pas
/// candidates.
///
/// # Errors
/// Pas de playlist, nom vide, inconnu ou ambigu : le message liste les pistes pour corriger.
pub fn select_track(tracks: &[Track], name: &str) -> Result<usize, String> {
    if tracks.is_empty() {
        return Err("aucune piste de démo dans la playlist de l'application".into());
    }
    let wanted = name.trim().to_lowercase();
    let list = || tracks.iter().map(label).collect::<Vec<_>>().join(", ");
    if wanted.is_empty() {
        return Err(format!("nom de piste vide ; pistes : {}", list()));
    }
    let playable = || tracks.iter().enumerate().filter(|(_, t)| t.error.is_none());
    let exact: Vec<usize> = playable()
        .filter(|(_, t)| t.title.to_lowercase() == wanted || label(t).to_lowercase() == wanted)
        .map(|(i, _)| i)
        .collect();
    let found = if exact.is_empty() {
        playable()
            .filter(|(_, t)| label(t).to_lowercase().contains(&wanted))
            .map(|(i, _)| i)
            .collect()
    } else {
        exact
    };
    match found.as_slice() {
        [one] => Ok(*one),
        [] => Err(format!("piste inconnue : {name:?} ; pistes : {}", list())),
        several => Err(format!(
            "piste ambiguë : {name:?} correspond à {} pistes ; pistes : {}",
            several.len(),
            list()
        )),
    }
}

/// « Artiste - Titre », ou le titre seul si le fichier ne suit pas cette forme.
fn label(track: &Track) -> String {
    if track.artist.is_empty() {
        track.title.clone()
    } else {
        format!("{} - {}", track.artist, track.title)
    }
}
