# Tâches de cyber-funk

Une tâche = un agent, une session, une branche, une PR. La fiche complète (périmètre, sources Sillage, critères
d'acceptation) est dans `docs/tasks/J*.md`. **Les agents ne lisent pas ce fichier** : il sert à l'orchestrateur.

## Lancer un agent

Prompt complet (tout le reste est dans `CLAUDE.md` et la fiche) :

```
Tâche <ID> du dépôt cyber-funk. Suis CLAUDE.md et la fiche <ID> (docs/tasks/<Jx>.md).
Branche task/<ID>-<slug>, PR vers main, critères cochés avec preuve.
```

Modèle et effort : ceux de la colonne du tableau. Pour le portage, le dépôt Sillage (`aifedespaix/aifedespaix.com`) doit
être attaché à la session (ou présent en local, lecture seule).

## Règles d'économie

Mesuré sur la vague 2, par agent, à son rapport final (hors travail fait avant une interruption) : Haiku 82 k jetons
(J0-04, deux petits fichiers), Sonnet 95 à 226 k (J2-03, J9-03, J0-02), Opus 209 à 239 k (J3-01, J1-01, J2-01). Le coût
vient de la taille de la tâche et du nombre d'itérations, bien plus que du démarrage.

1. **Le modèle le moins cher qui suffit, avec escalade.** Opus seulement quand la tâche *définit* un contrat dont tout
   dépend (voir tableau). Sonnet pour porter ou remplir un contrat existant. Haiku d'abord sur les ports très
   mécaniques à patron connu (J3-05, J6-03, J7-04, J8-06). **Escalade** : si un agent échoue deux fois sur le même
   critère (clippy, tests, perf), on relance la tâche au modèle du dessus au lieu de le laisser itérer.
2. **Effort `medium` par défaut pour les ports garantis par des tests** (golden Sillage, propriétés, simulation) ;
   `high` pour le rendu, WGSL, perf et la concurrence ; `xhigh` nulle part (J1-03 et J4-03 l'ont eu, c'est fini).
3. **3 agents en parallèle au plus**, dont au plus 2 qui compilent Bevy. Onze agents d'un coup ont rempli le disque
   et atteint la limite de session deux fois : le coût n'a pas baissé, il s'est arrêté puis reparti.
4. **Pas de mise à jour de ce fichier par les agents.** Ça créait un conflit dans presque chaque PR (une ligne
   modifiée par onze branches). L'orchestrateur passe les statuts en ✅ après les fusions.
5. **`ARCHITECTURE.md` ne se lit que par sections** (celles que la fiche cite) : ~4 600 mots, soit de l'ordre de
   10 000 jetons par agent s'il est lu en entier.
6. **Regroupements** : deux tâches du même crate dans une seule session évitent un démarrage et une PR (liste
   ci-dessous). Jamais plus de deux : un contexte qui gonfle coûte à chaque tour.
7. **Revue** : la CI et les critères de la fiche font le premier tri. Une revue Opus (`code-review`, `high`) seulement
   sur ce qui touche la sécurité du live ou un contrat central : J1-03, J2-05, J2-06, J3-03, J4-02.
8. **Discipline de sortie** (dans `CLAUDE.md` §5) : sorties cargo coupées avec `tail`, fichiers lus par plages,
   vérifications workspace complètes une seule fois avant de pousser.

## Choix du modèle

| Modèle | Tâches | Quand |
| --- | --- | --- |
| **Opus 5.5** (`claude-opus-5-5`) | J2-05, J3-03, J4-02, J6-01 (+ J1-01, J1-02, J1-03, J2-01, J3-01, J4-03 déjà lancées) | Contrat central : machine à états du show, tempo et arbitrage des sources, post-process, réalisateur |
| **Sonnet 5.5** (`claude-sonnet-5-5`) | la grande majorité des tâches restantes | Remplit un contrat existant, avec des tests ou des critères mesurables |
| **Haiku 5.5** (`claude-haiku-5-5`) | J3-05, J6-03, J7-04, J8-06 (essai, escalade si échec) | Ports mécaniques à patron connu |

## Statuts

⬜ à faire · 🟨 en cours · 🟦 PR ouverte · ✅ fusionnée.
En cours au dernier point : J1-02, J4-03 (les lignes passent à ✅ après leur fusion).

| ID | Tâche | Modèle | Effort | Dépend de | Statut |
| --- | --- | --- | --- | --- | --- |
| J0-01 | [Workspace Cargo et squelette des crates](tasks/J0.md) | Opus 5.5 | `high` | — | ✅ |
| J0-02 | [Fenêtres projecteur et opérateur, journaux, ligne de commande](tasks/J0.md) | Sonnet 5.5 | `high` | J0-01 | ✅ |
| J0-03 | [Assets de base et licences](tasks/J0.md) | Haiku 5.5 | `medium` | — | ✅ |
| J0-04 | [Outillage qualité vérifié de bout en bout](tasks/J0.md) | Haiku 5.5 | `medium` | J0-01 | ✅ |
| J1-01 | [Données de référence extraites de Sillage (golden)](tasks/J1.md) | Opus 5.5 | `high` | J0-03 | ✅ |
| J1-02 | [Lecture des pistes de démo](tasks/J1.md) | Opus 5.5 | `high` | J0-01, J0-03 | ⬜ |
| J1-03 | [Capture d'entrée et thread d'analyse](tasks/J1.md) | Opus 5.5 | `xhigh` | J0-01 | ✅ |
| J1-04 | [Détecteur de kicks et tempo (portage de `beat.ts`)](tasks/J1.md) | Sonnet 5.5 | `high` | J1-01, J1-03 | ⬜ |
| J1-05 | [Sections, intensité, gain automatique du micro](tasks/J1.md) | Sonnet 5.5 | `medium` | J1-04 | ⬜ |
| J1-06 | [Pont Bevy et panneau audio de l'opérateur](tasks/J1.md) | Sonnet 5.5 | `medium` | J0-02, J1-02, J1-05 | ⬜ |
| J1-07 | [Analyse hors ligne des démos (grille de temps en cache)](tasks/J1.md) | Sonnet 5.5 | `medium` | J1-04 | ⬜ |
| J2-01 | [Modèle de données et format RON](tasks/J2.md) | Opus 5.5 | `high` | J0-01 | ✅ |
| J2-02 | [Les 6 looks de Sillage et le fondu des portes](tasks/J2.md) | Sonnet 5.5 | `medium` | J2-01 | ⬜ |
| J2-03 | [Fraîcheur, ralenti du monde, attente de la musique](tasks/J2.md) | Sonnet 5.5 | `medium` | J0-01 | ✅ |
| J2-04 | [Chef d'orchestre du drop](tasks/J2.md) | Sonnet 5.5 | `high` | J2-02, J2-03 | ⬜ |
| J2-05 | [Chef du spectacle : créneaux, cues, modes, passation](tasks/J2.md) | Opus 5.5 | `high` | J2-01, J2-02, J2-03, J2-04, J3-01 | ⬜ |
| J2-06 | [Limiteur photosensible](tasks/J2.md) | Sonnet 5.5 | `high` | J0-01 | ✅ |
| J2-07 | [Sauvegarde continue et reprise](tasks/J2.md) | Sonnet 5.5 | `medium` | J2-05 | ⬜ |
| J2-08 | [Simulateur de set sans rendu](tasks/J2.md) | Sonnet 5.5 | `medium` | J2-05 | ⬜ |
| J3-01 | [Vocabulaire `Action` et couche de correspondance](tasks/J3.md) | Opus 5.5 | `high` | J0-01 | ✅ |
| J3-02 | [Entrée MIDI, profils et MIDI learn](tasks/J3.md) | Sonnet 5.5 | `high` | J3-01 | ⬜ |
| J3-03 | [MIDI Clock et arbitrage des sources de tempo](tasks/J3.md) | Opus 5.5 | `high` | J3-02, J1-05 | ⬜ |
| J3-04 | [Ableton Link (option)](tasks/J3.md) | Sonnet 5.5 | `medium` | J3-03 | ⬜ |
| J3-05 | [Entrée OSC (option)](tasks/J3.md) | Haiku 5.5 | `medium` | J3-01 | ⬜ |
| J3-06 | [Pont Bevy des contrôles](tasks/J3.md) | Sonnet 5.5 | `medium` | J3-01, J2-05, J0-02 | ⬜ |
| J4-01 | [Couches de rendu (arrière-plan, monde, avant-plan, 2D)](tasks/J4.md) | Sonnet 5.5 | `high` | J0-02 | ⬜ |
| J4-02 | [Post-process : HDR, bloom, grade, effets du show](tasks/J4.md) | Opus 5.5 | `high` | J4-01, J2-06 | ⬜ |
| J4-03 | [Streaming du monde, pools, origine flottante, trait `World`](tasks/J4.md) | Opus 5.5 | `xhigh` | J0-01 | ⬜ |
| J4-04 | [Culling : frustum, occlusion, simulation, LOD](tasks/J4.md) | Sonnet 5.5 | `high` | J4-03, J4-05 | ⬜ |
| J4-05 | [Surcouche de diagnostic](tasks/J4.md) | Sonnet 5.5 | `medium` | J0-02 | ⬜ |
| J4-06 | [Qualité adaptative](tasks/J4.md) | Sonnet 5.5 | `medium` | J4-02, J4-05 | ⬜ |
| J4-07 | [Préchauffage des pipelines et préchargement](tasks/J4.md) | Sonnet 5.5 | `high` | J4-02, J2-05 | ⬜ |
| J4-08 | [Palettes et fondu de thème](tasks/J4.md) | Sonnet 5.5 | `medium` | J4-01, J0-03 | ⬜ |
| J5-01 | [Route et plan de route](tasks/J5.md) | Sonnet 5.5 | `high` | J4-03, J1-06 | ⬜ |
| J5-02 | [Immeubles, matériau de façade, décor](tasks/J5.md) | Sonnet 5.5 | `high` | J5-01, J4-04 | ⬜ |
| J5-03 | [Ciel, étoiles, planètes](tasks/J5.md) | Sonnet 5.5 | `medium` | J4-01, J0-03 | ⬜ |
| J5-04 | [Atmosphère : brume, pluie, météo](tasks/J5.md) | Sonnet 5.5 | `medium` | J5-01 | ⬜ |
| J5-05 | [Voiture, lumières, traînée](tasks/J5.md) | Sonnet 5.5 | `high` | J5-01, J0-03 | ⬜ |
| J5-06 | [Pilote automatique, allure, chorégraphie, vol](tasks/J5.md) | Sonnet 5.5 | `high` | J5-05, J2-05 | ⬜ |
| J5-07 | [Faisceaux et effets du monde](tasks/J5.md) | Sonnet 5.5 | `medium` | J5-02, J4-02 | ⬜ |
| J6-01 | [Cœur du réalisateur](tasks/J6.md) | Opus 5.5 | `high` | J5-06, J4-01, J2-05 | ⬜ |
| J6-02 | [Éditeur et chorégraphie](tasks/J6.md) | Sonnet 5.5 | `high` | J6-01, J2-04 | ⬜ |
| J6-03 | [Plans, lot A : voiture](tasks/J6.md) | Haiku 5.5 | `medium` | J6-01 | ⬜ |
| J6-04 | [Plans, lot B : vol, cascades, vitesse](tasks/J6.md) | Sonnet 5.5 | `medium` | J6-01, J5-06 | ⬜ |
| J6-05 | [Plans, lot C : sujets du monde](tasks/J6.md) | Sonnet 5.5 | `medium` | J6-01 | ⬜ |
| J7-01 | [Système d'enseignes et textures 2D procédurales](tasks/J7.md) | Sonnet 5.5 | `high` | J5-02, J4-07 | ⬜ |
| J7-02 | [Enseigne géante du DJ](tasks/J7.md) | Sonnet 5.5 | `high` | J7-01, J2-05 | ⬜ |
| J7-03 | [Trafic : socle](tasks/J7.md) | Sonnet 5.5 | `high` | J5-02 | ⬜ |
| J7-04 | [Événements du trafic, lot A (calmes et urbains)](tasks/J7.md) | Haiku 5.5 | `medium` | J7-03 | ⬜ |
| J7-05 | [Événements du trafic, lot B (vol et démesure)](tasks/J7.md) | Sonnet 5.5 | `medium` | J7-03 | ⬜ |
| J7-06 | [Directeur des cascades](tasks/J7.md) | Sonnet 5.5 | `high` | J5-06 | ⬜ |
| J7-07 | [Scripts de cascades](tasks/J7.md) | Sonnet 5.5 | `medium` | J7-06 | ⬜ |
| J7-08 | [Landmarks : registre et bâtiments spéciaux](tasks/J7.md) | Sonnet 5.5 | `high` | J5-02 | ⬜ |
| J7-09 | [Landmarks vitrines](tasks/J7.md) | Sonnet 5.5 | `medium` | J7-08 | ⬜ |
| J7-10 | [Météores et impacts](tasks/J7.md) | Sonnet 5.5 | `medium` | J5-02, J5-03 | ⬜ |
| J7-11 | [Pré-intro et cartons-titres](tasks/J7.md) | Sonnet 5.5 | `medium` | J5-03, J4-01, J2-05 | ⬜ |
| J8-01 | [Vue Live](tasks/J8.md) | Sonnet 5.5 | `high` | J2-05, J3-06, J4-05 | ⬜ |
| J8-02 | [Préparation : événement, créneaux, cues](tasks/J8.md) | Sonnet 5.5 | `medium` | J8-01, J2-07 | ⬜ |
| J8-03 | [Éditeur de looks avec aperçu](tasks/J8.md) | Sonnet 5.5 | `medium` | J8-02, J4-08 | ⬜ |
| J8-04 | [MIDI learn et profils de contrôle](tasks/J8.md) | Sonnet 5.5 | `medium` | J3-02, J8-01 | ⬜ |
| J8-05 | [Aperçu de la sortie dans la fenêtre opérateur](tasks/J8.md) | Sonnet 5.5 | `medium` | J8-01, J4-06 | ⬜ |
| J8-06 | [Caméra libre de préparation](tasks/J8.md) | Haiku 5.5 | `low` | J6-01 | ⬜ |
| J9-01 | [Tests d'endurance](tasks/J9.md) | Sonnet 5.5 | `high` | J8-01, J7 terminé | ⬜ |
| J9-02 | [Pertes de sources et replis](tasks/J9.md) | Sonnet 5.5 | `high` | J3-03, J2-05 | ⬜ |
| J9-03 | [Paquets d'installation](tasks/J9.md) | Sonnet 5.5 | `medium` | J0-01 | ✅ |
| J9-04 | [Guide de l'opérateur](tasks/J9.md) | Sonnet 5.5 | `medium` | J8 terminé | ⬜ |
| J9-05 | [Campagne de performance sur le matériel cible](tasks/J9.md) | Sonnet 5.5 | `high` | J9-01 | ⬜ |

## Regroupements (une session pour deux tâches)

J2-07 + J2-08 · J4-05 + J4-06 · J5-03 + J5-04 · J6-04 + J6-05 · J7-08 + J7-09.
Le modèle de la session est celui de la tâche la plus exigeante du groupe.

## À repousser après J8 (hors chemin critique)

J1-07 (grille de temps en cache des démos), J3-04 (Ableton Link), J3-05 (OSC) : utiles, pas nécessaires pour un
premier live avec MIDI Clock.

## Vagues de parallélisation

Tâches d'une même vague : indépendantes entre elles. En lancer 3 à la fois au plus (règle 3).

| Vague | Tâches |
| --- | --- |
| 1 | J0-01 (Opus), J0-03 (Haiku) |
| 2 | J0-02 (Sonnet), J0-04 (Haiku), J1-01 (Opus), J1-02 (Opus), J1-03 (Opus), J2-01 (Opus), J2-03 (Sonnet), J2-06 (Sonnet), J3-01 (Opus), J4-03 (Opus), J9-03 (Sonnet) |
| 3 | J1-04 (Sonnet), J2-02 (Sonnet), J3-02 (Sonnet), J3-05 (Haiku), J4-01 (Sonnet), J4-05 (Sonnet) |
| 4 | J1-05 (Sonnet), J1-07 (Sonnet), J2-04 (Sonnet), J4-02 (Opus), J4-04 (Sonnet), J4-08 (Sonnet), J5-03 (Sonnet) |
| 5 | J1-06 (Sonnet), J2-05 (Opus), J3-03 (Opus), J4-06 (Sonnet) |
| 6 | J2-07 (Sonnet), J2-08 (Sonnet), J3-04 (Sonnet), J3-06 (Sonnet), J4-07 (Sonnet), J5-01 (Sonnet), J7-11 (Sonnet), J9-02 (Sonnet) |
| 7 | J5-02 (Sonnet), J5-04 (Sonnet), J5-05 (Sonnet), J8-01 (Sonnet) |
| 8 | J5-06 (Sonnet), J5-07 (Sonnet), J7-01 (Sonnet), J7-03 (Sonnet), J7-08 (Sonnet), J7-10 (Sonnet), J8-02 (Sonnet), J8-04 (Sonnet), J8-05 (Sonnet) |
| 9 | J6-01 (Opus), J7-02 (Sonnet), J7-04 (Haiku), J7-05 (Sonnet), J7-06 (Sonnet), J7-09 (Sonnet), J8-03 (Sonnet) |
| 10 | J6-02 (Sonnet), J6-03 (Haiku), J6-04 (Sonnet), J6-05 (Sonnet), J7-07 (Sonnet), J8-06 (Haiku) |
| 11 | J9-01 (Sonnet), J9-04 (Sonnet) |
| 12 | J9-05 (Sonnet) |

**Chemin critique** : J0-01 → J1-03 → J1-04 → J1-05 → J1-06 → J5-01 → J5-05 → J5-06 → J6-01 → J6-02.
**Version utilisable en soirée** (ville + caméra intelligente + cues + MIDI) : J0 à J6 terminés, plus J8-01.
