//! Les noms des plans de caméra, en clair, pour la timeline et la colonne « Plans caméra ».
//!
//! Le réalisateur ne donne que des identifiants (`"cockpit"`, `"wheel"`…). L'opérateur doit savoir *quelle* caméra est à
//! l'antenne pour dire laquelle est mal placée : le nom français et, entre parenthèses, une indication de position quand
//! elle aide (dans la voiture, au sol, en l'air). Un identifiant inconnu s'affiche tel quel.

/// Nom et position d'un plan : `(id, nom, où)`.
const SHOTS: [(&str, &str, &str); 37] = [
    ("ad-glance", "Coup d'œil à une pub", "ciel"),
    ("ascension-cam", "Ascension", "ciel"),
    ("barrel", "Tonneau", "autour de la voiture"),
    ("bikers", "Motards", "route"),
    ("bullet-time", "Bullet time", "autour de la voiture"),
    ("bumper", "Pare-chocs", "sur la voiture"),
    ("chase", "Poursuite", "derrière la voiture"),
    ("city-wide", "Ville large", "ciel"),
    ("cockpit", "Cockpit", "DANS la voiture"),
    ("crane", "Grue", "autour de la voiture"),
    ("dj-sign", "Enseigne DJ", "route"),
    ("dodge-pass", "Esquive", "autour de la voiture"),
    ("dolly-zoom", "Travelling compensé", "derrière la voiture"),
    ("dutch", "Plan penché", "autour de la voiture"),
    ("flyby", "Survol", "autour de la voiture"),
    ("fpv-dive", "FPV piqué", "ciel"),
    ("god-view", "Vue du ciel", "ciel"),
    ("handover", "Passation DJ", "route"),
    ("landmark-reveal", "Révélation d'un immeuble", "route"),
    ("meteor-impact-watch", "Impact de météore", "ciel"),
    ("meteor-ride", "Chevauchée de météore", "ciel"),
    ("moon-pass", "Passage devant la lune", "ciel"),
    ("orbit", "Orbite", "autour de la voiture"),
    ("pursuit", "Filature", "derrière la voiture"),
    ("push-in", "Travelling avant", "derrière la voiture"),
    ("ride-along", "À côté de la voiture", "autour de la voiture"),
    ("ringworld", "Monde-anneau", "ciel"),
    ("side-track", "Travelling latéral", "autour de la voiture"),
    ("sky-tilt", "Contre-plongée", "autour de la voiture"),
    ("spiral", "Spirale", "autour de la voiture"),
    ("stunt-cam", "Cascade", "autour de la voiture"),
    ("takeoff", "Décollage", "autour de la voiture"),
    ("top-down", "Vue du dessus", "ciel"),
    ("touchdown", "Atterrissage", "autour de la voiture"),
    ("vertigo", "Vertigo", "autour de la voiture"),
    ("waiting", "Attente", "ciel"),
    ("wheel", "Roue", "sur la voiture"),
];

/// Le nom d'un plan : « Cockpit », ou l'identifiant lui-même s'il est inconnu.
pub(crate) fn shot_name(id: &str) -> &str {
    SHOTS
        .iter()
        .find(|(known, ..)| *known == id)
        .map_or(id, |(_, name, _)| name)
}

/// Le nom et la position : « Cockpit (DANS la voiture) ».
pub(crate) fn shot_title(id: &str) -> String {
    match SHOTS.iter().find(|(known, ..)| *known == id) {
        Some((_, name, place)) => format!("{name} ({place})"),
        None => id.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chaque_plan_du_catalogue_a_son_nom() {
        let director = cf_cinema::CinemaDirector::new(cf_cinema::shots::catalog(), false, 1);
        let missing: Vec<_> = director.shot_ids().filter(|id| shot_name(id) == *id).collect();
        assert!(missing.is_empty(), "plans sans nom français : {missing:?}");
        for (id, ..) in SHOTS {
            assert!(director.shot_ids().any(|known| known == id), "{id} n'existe plus");
        }
    }

    #[test]
    fn un_identifiant_inconnu_s_affiche_tel_quel() {
        assert_eq!(shot_name("inconnu"), "inconnu");
        assert_eq!(shot_title("inconnu"), "inconnu");
        assert_eq!(shot_title("cockpit"), "Cockpit (DANS la voiture)");
    }
}
