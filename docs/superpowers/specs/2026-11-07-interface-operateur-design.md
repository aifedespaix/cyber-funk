# Interface opérateur à onglets (conception)

Refonte de la fenêtre opérateur (`cf-operator`, egui) en trois onglets, avec persistance des DJ, un onglet Debug en
grille, un journal en direct copiable pour un agent, le choix de l'écran de rendu et l'icône d'application.

Maquette de référence, validée par l'utilisateur : `docs/mockups/operator-ui-poc.html` (jetable, ne sert qu'à lire
les intentions d'ergonomie ; les noms d'événements et de plans y sont illustratifs).

## Intention

L'opérateur prépare une soirée de plusieurs DJ, la pilote en live, et doit pouvoir tester isolément chaque événement du
monde pendant la mise au point. Aujourd'hui tout est dans une fenêtre unique et un panneau « Debug monde » (F4) en
sections repliables, sans vue d'ensemble de ce qui tourne ni de journal lisible.

Succès :
- Un DJ édité, réordonné, ajouté ou supprimé est retrouvé tel quel au relancement.
- En Debug, on voit d'un coup d'œil un maximum d'événements cliquables **et** ce qui est en cours avec sa progression.
- Un bug vu en live se transmet à un agent en deux clics (copie des N dernières lignes du journal).
- Le rendu peut partir sur l'écran de son choix, mémorisé.

## Ce que l'utilisateur a décidé

Onglets : **Réglages**, **Statistiques**, **Debug**.

Retenus parmi les propositions : barre de statut permanente avec bouton Noir (2), « Passer au DJ suivant » avec
confirmation et compte à rebours (4), pause/reprise de la simulation toujours visible (5), repère « Marquer ce
moment » dans le journal (6), « Copier le contexte complet » (7), profils de soirée (10), import/export de la liste
des DJ (11), raccourcis affichés et aide `?` (12).

Écartés pour cette version : plafond de charge cumulée (3), rejouer le dernier événement (8), presets d'événements (9),
verrou de l'onglet Debug (1) : voir « Debug en Live » ci-dessous pour la règle par défaut.

## Architecture (approche B : coque + onglets + services)

Tout reste dans `cf-operator`, sans nouvelle crate. Le graphe de dépendances ne change pas. Les crates sans Bevy
(`cf-show`, `cf-debuglog`) reçoivent seulement des types de données ; Bevy et egui restent dans `cf-operator`.

```
cf-operator/src/
  shell/        barre de statut, barre d'onglets, raccourcis globaux, aide `?`
  tabs/
    settings/   DJ (liste + fiche), profils, import/export, écran de sortie
    stats/      tuiles, graphes, journal en direct
    debug/      grille 4 colonnes, dock caméra + voiture, barre de mode
  services/
    logbuf.rs   tampon circulaire des lignes de journal (lecture seule pour l'UI)
    active.rs   registre des événements actifs (lecture seule pour l'UI)
    screens.rs  liste des écrans et choix de la sortie
```

Les panneaux actuels (`audio_panel`, `learn_panel`, `diagnostics`, `debug_panel`, `controls_panel`) sont **déplacés**
dans l'onglet qui leur correspond et gardent leur logique ; seul leur emballage change. Chaque fichier reste sous 400
lignes (règle 11).

### Barre de statut permanente (visible dans tous les onglets et en Live)
- État de la source audio, fréquence d'images, tempo.
- Mode d'automatisation (Auto / Manuel), nombre d'événements en cours.
- DJ en cours, temps restant, bouton **Passer au suivant** (confirmation en deux temps, comme le reset de `sim_bar`)
  et compte à rebours avant le changement automatique.
- **Pause / reprise** de la simulation (reprise de `sim_bar`, qui devient une partie de cette barre).
- Bouton **Noir**, toujours actif, aussi au clavier (Échap).

### Onglet Réglages
- Liste des DJ de la soirée : glisser-déposer et `Alt+↑/↓` pour réordonner, ajout, suppression.
- Fiche du DJ sélectionné : les champs réels du `DjSlot` (nom, logo, thème, horaires, mode, profil audio, profils de
  contrôle, cues). *Écart (tâche 9)* : « style » et « couleur d'enseigne » de la maquette n'existent pas dans le
  modèle et ne sont pas créés ; un DJ est un `DjSlot` de l'`Event`, pas une liste dans `AppSettings`.
  Indicateur « modifications non enregistrées », `Annuler`, `Enregistrer` (`Ctrl+S`). Changer de DJ avec une fiche
  modifiée demande confirmation.
- **Profils de soirée** : plusieurs listes de DJ nommées, choisies dans un sélecteur, dupliquables.
- **Import / export** de la liste des DJ : ouvrir ou enregistrer un fichier d'événement RON choisi (`EventDocument::open` / `save_as`).
- **Écran de sortie** : liste des écrans (nom, résolution, fréquence), bouton « Afficher le numéro sur chaque écran »
  pendant 3 s, plein écran oui/non. Changement appliqué immédiatement.

### Onglet Statistiques
- Tuiles : images/s (avec p99), tempo et confiance, événements joués, état du limiteur de flashs.
- Énergie audio sur 60 s, spectre, événements par type, temps d'antenne par DJ.
- **Journal de session en direct** : lignes plafonnées (100 / 200 / 500), filtres par niveau et par texte, pause,
  vider. Clic sur le nom d'un événement : filtre dessus.
- **Copier pour un agent** : les N dernières lignes visibles (20 / 50 / 100 / 200) avec un en-tête (session, date,
  niveaux, filtre, format). Variante « warn + erreurs ». Variante **contexte complet** qui joint l'état courant :
  événements actifs, quartier et transition, plan caméra, DJ, mode d'automatisation, version.
- **Marquer ce moment** (raccourci `M`) : insère une ligne `mark` dans le journal, pour que l'agent sache où
  regarder.

### Onglet Debug
- Barre du haut : bascule **Mode manuel**, jauge d'événements en cours, recherche (`/`), « Tout arrêter ».
- Grille de **4 colonnes** : Panneaux, Immeubles, Voitures, Quartiers. Dans chaque colonne, hauteur partagée **2/3
  boutons de lancement, 1/3 « En cours »**, chacun défilant seul.
- Boutons en puces compactes : remplissage de la puce pendant l'événement, `×N` si lancé N fois, point jaune pour un
  événement à flash. Clic droit : épingler (les épinglés remontent en tête, mémorisés).
- « En cours » par colonne : nom, pourcentage, temps restant, tag `auto` quand l'orchestrateur l'a lancé, ✕ pour
  arrêter, Stop de colonne.
- Quartiers : destination + type de transition (les transitions non implémentées sont affichées barrées et retombent
  sur Dégradé, comme `TransitionKind::implemented`), « Changer de quartier », « Geler ».
- Dock à droite : plans caméra (avec le type de transition d'entrée : coupe, fondu, envolée, panoramique) et pad voiture
  (monter, descendre, gauche, droite, backflip, frontflip, tonneaux, stabiliser ; flèches maintenues).

### Correspondance familles du catalogue → colonnes
Le `WorldEventCatalog` range par famille (`family`). Une table dans `tabs/debug/` associe chaque famille à une des
quatre colonnes ; une famille non listée va dans une colonne « Autres » repliée et un test échoue si le catalogue
contient une famille sans correspondance (évite l'oubli silencieux).

## Mode manuel (point à vérifier avant d'écrire du code)

`SlotMode::Manual` (cf-show) ne règle que l'avancée des **cues**. Les événements du monde sont déclenchés par plusieurs
directeurs indépendants (`stunts`, `traffic`, `djsign`, `signs`, `landmark` dans `cf-world-neon`). Le mode manuel de
l'interface est donc une ressource distincte, **`WorldAutomation { manual: bool }`**, que chacun de ces directeurs lit
avant de lancer quoi que ce soit de lui-même. Les clics de l'opérateur passent toujours par `Action::Trigger`.

Manuel n'affaiblit pas le limiteur de flashs (règle 7) : il ne fait que couper les déclenchements spontanés.

## Persistance

- `AppSettings` (`cf-show/src/persist/settings.rs`) reçoit de nouveaux champs, chacun `#[serde(default)]` +
  `lenient`, donc sans incidence sur les fichiers existants. `SETTINGS_VERSION` passe à 2.
  - `active_profile: Option<String>` et `last_event_path`. Un **profil de soirée** n'est pas une liste de DJ dans les
    réglages : c'est un fichier d'événement RON dans `<config>/cyber-funk/profiles/<nom>.ron`. Les DJ sont déjà les
    `DjSlot` d'un `Event`, éditables et enregistrables par `PrepSession` ; une seconde représentation serait redondante.
  - `output_monitor: Option<String>` : **le nom** de l'écran, pas son numéro (un écran débranché change les numéros) ;
    si absent ce jour-là, retomber sur l'écran principal sans effacer la valeur.
  - `fullscreen: bool`, `pinned_events: BTreeSet<String>`, dernier onglet ouvert. Regroupés dans `OperatorPrefs`.
- Écriture atomique existante (`write_atomic`), à chaque « Enregistrer » et à la fermeture, jamais depuis un thread
  audio.
- Le contenu actuel de `DebugPanelPrefs` est conservé tel quel (compatibilité des fichiers déjà écrits).

## Écran de rendu et icône

- L'écran vient des `Monitor` de Bevy (déjà lus par `crates/cyber-funk/src/projector.rs` pour `--projector-monitor`).
  Le choix de l'interface réutilise `resolve()` ; l'option en ligne de commande garde la priorité sur le réglage.
- Icône : on génère depuis `assets/images/logo-app.svg` les PNG (16, 32, 48, 64, 128, 256, 512) et un `.ico`
  Windows, versionnés dans `assets/images/icons/`. La fenêtre reçoit l'icône à la création, l'exécutable la reçoit par
  un `build.rs` du binaire. Un script dans `tools/` régénère les fichiers ; les sorties sont commitées pour qu'un
  build ne dépende pas d'un outil externe.

## Journal et événements actifs (services)

- `logbuf` : tampon circulaire de capacité fixe, alimenté depuis `cf-debuglog` (qui garde son fichier
  `events.jsonl`). Jamais de relecture du fichier par l'UI. Ajout hors thread audio uniquement.
  `cf-debuglog` expose un petit crochet de type « abonné » (sans Bevy) ; `cf-operator` lit le tampon.
- `active` : ressource `ActiveEvents` (id, catégorie, début, durée prévue, origine auto/manuelle) publiée par les
  directeurs du monde au lancement et à la fin. Le pourcentage est calculé par l'UI (`now - début) / durée`).
  Un événement sans durée connue affiche une barre indéterminée.

## Live

- Les points de la barre de statut (Noir, DJ suivant, pause, mode) sont disponibles en Live.
- **L'onglet Debug est masqué en Live** (règle actuelle de F4 : un clic de trop ne doit pas lancer un événement devant le
  public). On y revient en repassant en Préparation.
- Le journal, « Marquer ce moment » et « Copier » restent accessibles en Live : ils ne lancent rien.
- La vue Live existante (`live/`, gros éléments lisibles dans le noir) n'est pas refaite ; la barre de statut s'y
  ajoute.

## Responsive et accessibilité

- Contenu pensé en 16/9, fenêtre redimensionnable. En dessous de 1100 px de large la colonne « écran de sortie » des
  Réglages passe dans un repli ; en dessous de 640 × 400 la fenêtre est bloquée (déjà le cas, `MIN_SIZE`).
- Tout est opérable au clavier : onglets `1 2 3`, recherche `/`, enregistrer `Ctrl+S`, repère `M`, aide `?`.
- Focus visible, cibles cliquables d'au moins 24 px, couleurs jamais seules porteuses d'information (tags et libellés
  en plus des teintes).

## Tests

- **Unitaires (sans fenêtre)** : table familles → colonnes complète ; sérialisation RON des profils et lecture
  tolérante d'un `AppSettings` ancien (version 1) ; réordonnancement des DJ ; formatage de la copie du journal (en-tête,
  tronquage à N, filtres) ; tampon circulaire (capacité, ordre) ; résolution de l'écran par nom avec repli.
- **Bevy (applications minimales)** : `WorldAutomation { manual: true }` empêche chaque directeur de lancer un événement
  spontané ; un clic émet `Action::Trigger` ; `ActiveEvents` se remplit et se vide.
- **Visuel** : captures de la fenêtre sous `Xvfb` à deux tailles (1280 × 720 et 1920 × 1080) pour chaque onglet, jointes à
  la PR ; ce qui ne peut pas être vérifié sans écran réel (choix d'écran multiple, icône du bureau) est dit dans la PR.
- `cargo fmt`, `clippy -D warnings`, `nextest` avant chaque commit (règle 10).

## Hors périmètre

Plafond de charge cumulée, rejouer le dernier événement, presets d'événements, séquences par glisser-déposer,
historique long ou export des statistiques, refonte de la vue Live. Un nouveau type d'événement ou de plan caméra
n'est pas créé : l'interface liste ce que le catalogue et le répertoire de plans exposent déjà.

## Risques

- **Déplacement de code** (panneaux existants vers les onglets) : à faire en commits séparés, sans changement de
  comportement, pour relire facilement.
- **`WorldAutomation`** touche plusieurs directeurs de `cf-world-neon` : un directeur oublié continuerait de lancer des
  événements en mode manuel. Le test Bevy ci-dessus énumère les directeurs ; en ajouter un exige de l'ajouter au test.
- **Durées inconnues** pour certains événements : barre indéterminée plutôt qu'un faux pourcentage.
- **Décalage entre l'interface et le monde** si un événement est refusé (limiteur, plafond de file de
  `MAX_ORDERS_PER_FRAME`) : l'entrée « en cours » ne doit apparaître que lorsque le monde confirme le lancement.

## Découpage proposé (pour le plan d'implémentation)

1. Coque : barre de statut, onglets, raccourcis, aide ; panneaux existants déplacés sans changement.
2. Services : tampon de journal, `ActiveEvents`, `WorldAutomation` et son branchement dans les directeurs.
3. Onglet Debug en grille.
4. Onglet Statistiques et journal copiable.
5. Onglet Réglages : DJ, persistance, profils, import/export.
6. Écran de sortie et icône d'application.
