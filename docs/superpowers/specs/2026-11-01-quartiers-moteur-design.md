# Moteur de quartiers (conception)

Chantier 3.1 sur 4 du thème « rendre le monde néon moins propre » (suite des façades vivantes, chantier 1).
Les autres morceaux auront chacun leur document : 3.2 bâtiments et mobilier par quartier, 3.3 graffitis,
3.4 transitions spectaculaires (pont, échangeur, immeubles qui rétrécissent, panne puis réveil).
Le chantier 2 (caméra, grain, brume, poussière) est séparé.

## Intention

Le monde est un seul décor néon qui défile. L'utilisateur veut que l'app se comporte comme un organisme qui
s'adapte au son : au fil du set, la ville change de quartier (centre néon, banlieue graffiti, résidentiel), de
façon fluide, sans rupture, en gardant l'ambiance cyberpunk néon. La couleur est le premier levier pour sentir
qu'on change de lieu. Tout est automatique ; un panneau de debug sert à forcer un quartier ou une transition
pour inspecter le rendu sans attendre un bon moment musical.

## Ce que l'utilisateur a décidé

- Plusieurs quartiers, d'abord : **centre néon** (l'actuel), **banlieue** (graffitis), **résidentiel**
  (lampadaires, bouches d'égout, petites maisons répétitives). Chaque type a plusieurs variantes.
- Changer de quartier doit se **sentir** : palette de couleurs bien distincte par quartier. Deux quartiers du
  même type peuvent avoir des couleurs différentes : jeux de couleurs cohérents **prédéfinis, tirés au hasard**.
- Transition « dégradé » : un vrai fondu court, **10 secondes maximum**.
- Le changement est **entièrement automatique**, guidé par l'analyse sonore (changement de style détecté, ou
  autre signal). **Jamais en plein drop ni en pleine musique forte.** Un quartier peut se rejouer s'il n'y a
  pas assez de choix, mais on doit sentir la différence.
- **De vraies règles** de décision, lisibles et testables.
- Le lien son → quartier est un **penchant**, pas une règle stricte (garde de la surprise).
- Panneau de debug (F4) **amélioré** : choisir un quartier, choisir la transition, geler l'automatique.
- Les événements du show (ex. fin de la danse des robots) peuvent aussi déclencher un changement.

## Objectifs

1. **Quartier courant et plan**: une notion de quartier (type + variante de palette + graine) attachée à des
   segments de route, fonction pure de la distance, planifiée en avance.
2. **Gouverneur de décision**: règles pures, testables sans Bevy, qui décident quand et vers quoi changer.
3. **Étalonnage par quartier**: jeux prédéfinis (RON) par type, tirage aléatoire sans répétition immédiate. Un
   étalonnage est une modulation **modérée** appliquée par-dessus le thème du show (voir Précisions, point 4).
4. **Différence visible dès ce chantier**: par la palette, la densité et les hauteurs d'immeubles, et le mélange
   de tempéraments de façade propres à chaque type (les formes de bâtiments et le mobilier viennent en 3.2).
5. **Transition en dégradé**: mélange le long de la route, ≤ 10 s, déterministe.
6. **Abstraction de transition**: un type `TransitionKind` avec les 7 variantes retenues (dégradé, brume, zone
   tampon, pont, rétrécissement, panne puis réveil, échangeur) ; ce chantier n'implémente que le dégradé, les
   autres retombent sur le dégradé tant qu'elles ne sont pas faites (3.4 et chantier 2).
7. **Debug F4 refait**: voir plus bas.

## Hors périmètre

- Nouvelles formes de bâtiments, maisons, lampadaires, bouches d'égout, textures (3.2).
- Graffitis et polices (3.3).
- Pont/périphérique, échangeur, rétrécissement, panne puis réveil (3.4) ; brume (chantier 2) ; zone tampon (3.2).
- Réglage manuel des quartiers dans les looks/shows : non, tout est automatique. Seul le debug force.
- Nouveau détecteur de style audio : on réutilise les sections existantes (voir décisions).

## Anti-objectifs

- Aucune rupture visible, aucun flash ni forte variation de luminance qui contourne le limiteur
  (`cf-show`, règle 7). Toute vague de lumière de transition passe par `FlashLimiter::trigger`.
- Aucun changement déclenché pendant un drop, juste avant, ni en section Peak.
- Pas d'allocation par image en régime établi (règle 4), pas de compilation de shader pendant un show
  (règle 8), pas de variantes de shader.
- Pas de fichier de plus de 400 lignes (règle 11) ; `cf-show` reste sans Bevy (règle 5).
- Le golden `tests/golden/scenery.json` ne change pas pour le quartier « centre néon par défaut ».
- Pas de hasard par image : le tirage des quartiers et des palettes est seedé et reproductible en test.

## Contraintes (état du code, relevé)

- Palette (`cf-render/src/palette/`) : `ScenePalette` (11 rôles `SceneColor`), `PaletteMorph::morph_to`,
  `ThemeFader` (fondu d'une mesure), message `PaletteRequest{theme, timing: Downbeat|Now}`, unique écrivain
  `drive_palette`. Le thème peut aussi venir du show (`CurrentShow.theme/theme_seq`) et ré-écraser.
- Audio : `Section {Calm, Groove, Peak}` avec hystérésis et délai mini 6 s (`cf-analysis/src/sections.rs`),
  `AudioFeatures.section/section_changed/intensity/bass/high/bpm`. Pas de signal « nouveauté » dédié.
- Drops : `ShowRuntime.director.drop_plan().is_some()` = drop imminent (plan préparé pendant la montée, TTL 30 s) ;
  `CurrentShow.0.drop_now` = drop qui frappe (une image). Pas d'état « drop en cours » durable : à construire
  (fenêtre de N mesures après `drop_now`).
- Génération : `Row::generate(index, &SceneryConfig)` pure, une rangée = 26 m, fenêtre de streaming de
  18 rangées devant (~468 m) et 15 derrière ; seuls les tronçons à venir sont modifiables sans `replan_all`.
  `SceneryConfig` est le véhicule de configuration du décor. La graine du monde n'entre pas dans les immeubles.
- Route droite ; `RoutePlan.bridges` (arches) existe et peut servir de repère mais n'est pas une transition.
- Événements : `WorldEventCatalog` (cf-render) alimente F4 ; un `district:xxx` ajouté apparaît automatiquement
  dans une famille « Quartiers ». Fin de la danse des robots : fin du `LandmarkEvent` mecha
  (`TrafficDirector::active_names()`).
- Panneau F4 (`cf-operator/src/debug_panel.rs`, 343 lignes) : fenêtre egui unique, seulement en Préparation,
  structure plate, pas de `ScrollArea` globale, persistance par chaînes dans `collapsed_panels`
  (`debug-open:<famille>`, `tuning:*`).

## Décisions de conception déjà prises

- Le quartier est une fonction pure de l'index de rangée via un **plan de quartiers** (segments consécutifs
  `{début d'index, type, variante, graine}`), allongé à l'avant de la voiture. Jamais d'état par immeuble.
- Le gouverneur vit côté `cf-show` (sans Bevy), alimenté par un instantané (section, intensité, basses/aigus,
  drop imminent, drop récent, temps depuis le dernier changement) et un tirage seedé. Il rend une décision :
  `Stay` ou `Change{ to, transition }`, avec une raison lisible pour le debug.
- Mélange de transition : dans la zone de dégradé, chaque emplacement choisit l'ancien ou le nouveau quartier
  par un hachage de sa position avec une probabilité qui monte linéairement, déterministe.
- La palette est un levier à part : le fondu de palette démarre sur le temps fort suivant (`Downbeat`).

## Choix laissés au constructeur

- Les types d'événements du gouverneur, les valeurs exactes des seuils (voir ci-dessous les valeurs proposées).
- Le nombre de jeux de palette par type (proposition : 6 à 8) et leur écriture (RON dans `assets/`).
- La structure de persistance du panneau F4 (propre structure typée recommandée plutôt que d'ajouter des chaînes
  dans `collapsed_panels`).
- Comment mettre l'état du quartier à disposition du reste du monde (ressource Bevy `DistrictState`).

## Règles de décision proposées (appel du rédacteur, valeurs réglables)

1. **Fenêtre autorisée** : section `Calm`, ou `section_changed` vers `Calm`/`Groove` ; jamais en `Peak`, jamais si un
   drop est imminent, jamais pendant les 8 mesures qui suivent un drop.
2. **Temps minimum** dans un quartier : 150 s. **Temps maximum** : 8 min, après quoi le premier moment autorisé
   déclenche un changement.
3. **Choix du prochain quartier** : différent du courant ; évite les 2 derniers quand il y a assez de choix ; sinon
   une répétition est tolérée avec une **autre variante de palette** (jamais la même que la précédente).
4. **Penchant sonore** : poids de chaque type selon l'énergie (intensité élevée → centre néon ; basses lourdes et
   peu d'aigus → banlieue ; calme → résidentiel), mélangé à 25 % de hasard.
5. **Événements** : la fin d'un événement marquant (ex. danse des robots) ouvre une fenêtre de changement
   anticipée, sous les mêmes interdits drop/Peak.
6. **Annulation** : si un drop se prépare pendant une transition, elle se termine avant le drop (le dégradé dure ≤ 10 s)
   ; elle ne démarre pas si moins de 10 s de calme sont garanties.
7. **Dégradé** : durée fixe 10 s maximum, réduite à la distance parcourue à la vitesse courante (longueur de mélange
   = vitesse × durée, bornée).

## Debug F4 amélioré

- Une `ScrollArea` globale, sections repliables : Façades, Voiture, **Quartiers**, Événements.
- Section Quartiers : quartier courant, variante de palette, progression de la transition, durée de la dernière
  décision et raison (« bloqué : drop imminent », « fenêtre ouverte : section Calm »).
- Commandes : choisir le type, choisir la variante de palette, choisir la transition, bouton « Forcer »,
  interrupteur « Geler l'automatique », curseur de durée de transition (1 à 10 s), historique des 5 derniers quartiers.
- Un forçage manuel contourne les règles du gouverneur (c'est un outil de debug) mais pas le limiteur de flashs.
- Familles « Quartiers » dans le catalogue d'événements (`district:<type>`) pour le déclenchement par MIDI/trigger.
- Réservé à la préparation, comme aujourd'hui ; non sauvegardé dans les looks.

## Critères d'acceptation

- Le gouverneur ne propose jamais de changement en section Peak, avec drop imminent ou dans la fenêtre après drop
  (test sur séquences de référence).
- Respect du temps minimum et maximum, du « jamais le même quartier deux fois de suite », de la variante de palette
  différente ; reproductible avec la même graine.
- Le dégradé dure ≤ 10 s et sa position suit la voiture ; deux exécutions donnent la même disposition.
- Les trois types ont des palettes visiblement distinctes ; deux quartiers du même type ont des variantes différentes.
- Le golden `scenery.json` est inchangé pour le quartier par défaut.
- Le panneau F4 permet de forcer un quartier, une variante, une transition et de geler l'automatique ; l'état courant
  et la raison des décisions s'affichent.
- Aucune régression du budget d'image mesuré avant/après sur la même séquence ; pas d'allocation par image.
- `cargo fmt`, `clippy -D warnings`, `cargo nextest run --workspace` verts.
- Vérification visuelle et captures dans `docs/captures/` faites par un humain (non réalisable par l'agent).

## Précisions après relecture (appels du rédacteur)

1. **Branchement dans la génération** : un champ `districts: DistrictPlan` (ou équivalent) de `SceneryConfig`, par
   défaut « centre néon sur toute la route », donc `Row::generate` donne les mêmes résultats et le golden reste
   inchangé. Le tirage des quartiers utilise la graine du monde, pas celle des immeubles (qui reste inchangée).
2. **Décision tardive** : le plan ne modifie que les rangées pas encore générées. La zone de dégradé commence après
   une marge d'au moins 2 rangées au-delà de la dernière rangée générée ; si la voiture accélère, les rangées déjà
   créées restent telles quelles (le dégradé démarre simplement plus loin). Pas de `replan_all` en régime établi.
3. **Horloge** : le gouverneur compte en **secondes de temps audio** ; « 8 mesures après un drop » devient 8 mesures
   si le BPM est connu, sinon 16 s. Sans show, le gouverneur ne propose rien (sauf forçage debug). Le suivi de la
   fenêtre « drop récent » est dans `cf-show`, alimenté par l'instantané.
5. **Limiteur** : le dégradé n'a pas besoin d'une vague de lumière ; le fondu de palette suffit. Les jeux de palette
   d'un même type gardent une luminance moyenne voisine, et un test vérifie qu'aucun fondu entre deux palettes ne
   dépasse l'écart de luminance toléré par le limiteur.
6. **Différence visible sans 3.2** : de nouveaux paramètres de `SceneryConfig` par type de quartier (hauteurs,
   densité, mélange de tempéraments). Valeurs initiales choisies par moi, à régler à l'œil par un humain.
7. **Forçage** : un forçage reçu pendant une transition la remplace (elle repart vers la nouvelle cible depuis l'état
   courant) ; pendant un drop il est accepté (outil de debug). Les 6 transitions non implémentées retombent sur le
   dégradé, et F4 affiche « (repli : dégradé) ».
8. **Fin d'événement** : le gouverneur reçoit un champ générique `event_ended: bool` dans l'instantané (une seule
   image), alimenté par le monde ; pas de dépendance à l'événement robot précis.
9. **Persistance F4** : une structure typée distincte de `collapsed_panels` ; le gel et le forçage ne sont **pas**
   sauvegardés d'une session à l'autre. `debug_panel.rs` est découpé en modules.
10. **Étalonnages** : j'écris moi-même 6 à 8 étalonnages par type (RON), à valider à l'œil. Une « variante » est un
    index dans l'ensemble du type.

## Décision de l'utilisateur sur la palette (remplace le point 4 ci-dessus)

Le **changement de DJ** fait le changement de couleur drastique (thème du show, inchangé). Le **changement de quartier**
est moins drastique : il ne remplace jamais la palette du show, il la **module** par-dessus.

- Le thème du show reste la base ; le système de palette existant n'est pas modifié dans son rôle.
- Un étalonnage de quartier = rotation de teinte bornée (ordre de ±30°, à régler), variation de saturation et de
  luminosité bornée, et poids différent des rôles d'accent (ex. l'ambre domine en banlieue, la glace en résidentiel).
  Il est appliqué comme une couche de sortie, comme le `hue_shift_deg` du look, avec son propre fondu.
- Un changement de DJ pendant un quartier garde l'étalonnage du quartier appliqué au nouveau thème.
- Test : l'écart de couleur entre « thème du show seul » et « thème + étalonnage » reste sous un seuil, et il est
  toujours plus petit que l'écart entre deux thèmes du show.
- Les quartiers de même type diffèrent par leur variante d'étalonnage ; trois types × 6 à 8 variantes.
- Les couleurs d'étalonnage partent de mon goût cyberpunk tant que l'utilisateur ne donne pas de références.
