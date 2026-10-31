//! Le choix de la voiture, vu de la fenêtre opérateur.
//!
//! La fenêtre opérateur ne connaît pas le monde (`cf-world-neon`) : elle lit et écrit cette ressource, et le monde la
//! tient à jour (quelles voitures sont décodées, laquelle est posée) et met en œuvre la demande.

use bevy::prelude::*;

/// Une voiture proposée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarOption {
    /// Identifiant stable (`"dirty"`, `"cyber"`), celui des réglages sauvegardés.
    pub key: &'static str,
    /// Nom affiché.
    pub label: &'static str,
    /// Le modèle est décodé : la voiture peut être posée sans rien charger.
    pub ready: bool,
}

/// Les voitures proposées, celle qui est posée et celle qu'on demande.
#[derive(Resource, Debug, Clone, Default)]
pub struct CarSelection {
    /// Les voitures, dans l'ordre d'affichage.
    pub options: Vec<CarOption>,
    /// La voiture posée en ce moment (`key`).
    pub active: &'static str,
    /// La voiture demandée (`key`) ; égale à `active` quand rien n'est en attente.
    pub requested: &'static str,
}

impl CarSelection {
    /// Demande cette voiture. Renvoie `true` si la demande a changé : inconnue, pas encore décodée ou déjà demandée
    /// sont refusées sans effet.
    pub fn request(&mut self, key: &str) -> bool {
        let Some(option) = self.options.iter().find(|option| option.key == key && option.ready) else {
            return false;
        };
        if self.requested == option.key {
            return false;
        }
        self.requested = option.key;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection() -> CarSelection {
        CarSelection {
            options: vec![
                CarOption {
                    key: "dirty",
                    label: "Dirty car",
                    ready: true,
                },
                CarOption {
                    key: "cyber",
                    label: "CyberCar",
                    ready: false,
                },
            ],
            active: "dirty",
            requested: "dirty",
        }
    }

    #[test]
    fn une_voiture_inconnue_ou_pas_prete_est_refusee() {
        let mut selection = selection();
        assert!(!selection.request("hovercraft"));
        assert!(!selection.request("cyber"), "pas encore décodée");
        assert_eq!(selection.requested, "dirty");
    }

    #[test]
    fn une_voiture_prete_est_demandee_une_seule_fois() {
        let mut selection = selection();
        selection.options[1].ready = true;
        assert!(selection.request("cyber"));
        assert_eq!(selection.requested, "cyber");
        assert!(!selection.request("cyber"), "déjà demandée");
    }

    #[test]
    fn redemander_la_voiture_en_place_ne_change_rien() {
        let mut selection = selection();
        assert!(!selection.request("dirty"));
    }
}
