# Tâches de cyber-funk

Chaque tâche est faite pour être confiée à **un agent, dans une session, sur une branche, pour une PR**.
La fiche complète (périmètre, sources Sillage, critères d'acceptation) est dans `docs/tasks/J*.md`.

## Lancer un agent

1. Choisir une tâche dont toutes les dépendances sont ✅.
2. Ouvrir une session avec le **modèle** et l'**effort** indiqués, dépôts `aifedespaix/cyber-funk` **et**
   `aifedespaix/aifedespaix.com` attachés (le second sert de source pour le portage).
3. Prompt :

```
Tâche <ID> du dépôt cyber-funk.
Lis CLAUDE.md, puis la fiche <ID> dans docs/tasks/<Jx>.md, charge les skills qu'elle cite,
lis les fichiers Sillage indiqués (dépôt aifedespaix/aifedespaix.com, dossier apps/main/),
puis réalise la tâche. Branche task/<ID>-<slug>, PR vers main avec le modèle de PR,
critères d'acceptation cochés un par un avec leur preuve, statut mis à jour dans docs/TASKS.md.
```

4. Relire la PR. Pour les tâches critiques (audio temps réel, chef du spectacle, post-process, streaming), faire
   passer une revue par un agent **Opus 5.5** (skill `code-review`, effort `high`) avant de fusionner.

## Pourquoi ce modèle-là

| Modèle | Quand | Nombre de tâches |
| --- | --- | --- |
| **Opus 5.5** (`claude-opus-5-5`) | La tâche **définit** un contrat : trait, format de fichier, protocole entre threads, machine à états, render graph. Une erreur ici se paie dans toutes les tâches suivantes. | 30 |
| **Sonnet 5.5** (`claude-sonnet-5-5`) | La tâche **remplit** un contrat déjà posé : porter un plan caméra, un événement, une cascade, un écran egui. Beaucoup de volume, peu de décisions. | 35 |
| **Haiku 5.5** (`claude-haiku-5-5`) | Aucune décision : copier des assets, vérifier un outil. | 2 |

Effort : `xhigh` pour la concurrence et les cœurs de système (thread d'analyse, chef du spectacle, post-process,
streaming, réalisateur, MIDI Clock) ; `high` par défaut ; `medium`/`low` pour le mécanique.

## Statuts

⬜ à faire · 🟨 en cours (indiquer la branche) · 🟦 PR ouverte · ✅ fusionnée

| ID | Tâche | Modèle | Effort | Dépend de | Statut |
| --- | --- | --- | --- | --- | --- |
| J0-01 | [Workspace Cargo et squelette des crates](tasks/J0.md) | Opus 5.5 | `high` | — | 🟦 |
| J0-02 | [Fenêtres projecteur et opérateur, journaux, ligne de commande](tasks/J0.md) | Sonnet 5.5 | `high` | J0-01 | ⬜ |
| J0-03 | [Assets de base et licences](tasks/J0.md) | Haiku 5.5 | `medium` | — | ⬜ |
| J0-04 | [Outillage qualité vérifié de bout en bout](tasks/J0.md) | Haiku 5.5 | `medium` | J0-01 | ⬜ |
| J1-01 | [Données de référence extraites de Sillage (golden)](tasks/J1.md) | Opus 5.5 | `high` | J0-03 | ⬜ |
| J1-02 | [Lecture des pistes de démo](tasks/J1.md) | Opus 5.5 | `high` | J0-01, J0-03 | ⬜ |
| J1-03 | [Capture d'entrée et thread d'analyse](tasks/J1.md) | Opus 5.5 | `xhigh` | J0-01 | ⬜ |
| J1-04 | [Détecteur de kicks et tempo (portage de `beat.ts`)](tasks/J1.md) | Opus 5.5 | `high` | J1-01, J1-03 | ⬜ |
| J1-05 | [Sections, intensité, gain automatique du micro](tasks/J1.md) | Sonnet 5.5 | `high` | J1-04 | ⬜ |
| J1-06 | [Pont Bevy et panneau audio de l'opérateur](tasks/J1.md) | Sonnet 5.5 | `medium` | J0-02, J1-02, J1-05 | ⬜ |
| J1-07 | [Analyse hors ligne des démos (grille de temps en cache)](tasks/J1.md) | Sonnet 5.5 | `medium` | J1-04 | ⬜ |
| J2-01 | [Modèle de données et format RON](tasks/J2.md) | Opus 5.5 | `high` | J0-01 | ⬜ |
| J2-02 | [Les 6 looks de Sillage et le fondu des portes](tasks/J2.md) | Sonnet 5.5 | `high` | J2-01 | ⬜ |
| J2-03 | [Fraîcheur, ralenti du monde, attente de la musique](tasks/J2.md) | Sonnet 5.5 | `medium` | J0-01 | ⬜ |
| J2-04 | [Chef d'orchestre du drop](tasks/J2.md) | Opus 5.5 | `high` | J2-02, J2-03 | ⬜ |
| J2-05 | [Chef du spectacle : créneaux, cues, modes, passation](tasks/J2.md) | Opus 5.5 | `xhigh` | J2-01, J2-02, J2-03, J2-04, J3-01 | ⬜ |
| J2-06 | [Limiteur photosensible](tasks/J2.md) | Sonnet 5.5 | `high` | J0-01 | ⬜ |
| J2-07 | [Sauvegarde continue et reprise](tasks/J2.md) | Sonnet 5.5 | `medium` | J2-05 | ⬜ |
| J2-08 | [Simulateur de set sans rendu](tasks/J2.md) | Sonnet 5.5 | `high` | J2-05 | ⬜ |
| J3-01 | [Vocabulaire `Action` et couche de correspondance](tasks/J3.md) | Opus 5.5 | `high` | J0-01 | ⬜ |
| J3-02 | [Entrée MIDI, profils et MIDI learn](tasks/J3.md) | Opus 5.5 | `high` | J3-01 | ⬜ |
| J3-03 | [MIDI Clock et arbitrage des sources de tempo](tasks/J3.md) | Opus 5.5 | `xhigh` | J3-02, J1-05 | ⬜ |
| J3-04 | [Ableton Link (option)](tasks/J3.md) | Sonnet 5.5 | `high` | J3-03 | ⬜ |
| J3-05 | [Entrée OSC (option)](tasks/J3.md) | Sonnet 5.5 | `medium` | J3-01 | ⬜ |
| J3-06 | [Pont Bevy des contrôles](tasks/J3.md) | Sonnet 5.5 | `medium` | J3-01, J2-05, J0-02 | ⬜ |
| J4-01 | [Couches de rendu (arrière-plan, monde, avant-plan, 2D)](tasks/J4.md) | Opus 5.5 | `high` | J0-02 | ⬜ |
| J4-02 | [Post-process : HDR, bloom, grade, effets du show](tasks/J4.md) | Opus 5.5 | `xhigh` | J4-01, J2-06 | ⬜ |
| J4-03 | [Streaming du monde, pools, origine flottante, trait `World`](tasks/J4.md) | Opus 5.5 | `xhigh` | J0-01 | ⬜ |
| J4-04 | [Culling : frustum, occlusion, simulation, LOD](tasks/J4.md) | Opus 5.5 | `high` | J4-03, J4-05 | ⬜ |
| J4-05 | [Surcouche de diagnostic](tasks/J4.md) | Sonnet 5.5 | `high` | J0-02 | ⬜ |
| J4-06 | [Qualité adaptative](tasks/J4.md) | Sonnet 5.5 | `high` | J4-02, J4-05 | ⬜ |
| J4-07 | [Préchauffage des pipelines et préchargement](tasks/J4.md) | Opus 5.5 | `high` | J4-02, J2-05 | ⬜ |
| J4-08 | [Palettes et fondu de thème](tasks/J4.md) | Sonnet 5.5 | `high` | J4-01, J0-03 | ⬜ |
| J5-01 | [Route et plan de route](tasks/J5.md) | Opus 5.5 | `high` | J4-03, J1-06 | ⬜ |
| J5-02 | [Immeubles, matériau de façade, décor](tasks/J5.md) | Opus 5.5 | `high` | J5-01, J4-04 | ⬜ |
| J5-03 | [Ciel, étoiles, planètes](tasks/J5.md) | Sonnet 5.5 | `high` | J4-01, J0-03 | ⬜ |
| J5-04 | [Atmosphère : brume, pluie, météo](tasks/J5.md) | Sonnet 5.5 | `high` | J5-01 | ⬜ |
| J5-05 | [Voiture, lumières, traînée](tasks/J5.md) | Sonnet 5.5 | `high` | J5-01, J0-03 | ⬜ |
| J5-06 | [Pilote automatique, allure, chorégraphie, vol](tasks/J5.md) | Opus 5.5 | `high` | J5-05, J2-05 | ⬜ |
| J5-07 | [Faisceaux et effets du monde](tasks/J5.md) | Sonnet 5.5 | `medium` | J5-02, J4-02 | ⬜ |
| J6-01 | [Cœur du réalisateur](tasks/J6.md) | Opus 5.5 | `xhigh` | J5-06, J4-01, J2-05 | ⬜ |
| J6-02 | [Éditeur et chorégraphie](tasks/J6.md) | Opus 5.5 | `high` | J6-01, J2-04 | ⬜ |
| J6-03 | [Plans, lot A : voiture](tasks/J6.md) | Sonnet 5.5 | `high` | J6-01 | ⬜ |
| J6-04 | [Plans, lot B : vol, cascades, vitesse](tasks/J6.md) | Sonnet 5.5 | `high` | J6-01, J5-06 | ⬜ |
| J6-05 | [Plans, lot C : sujets du monde](tasks/J6.md) | Sonnet 5.5 | `high` | J6-01 | ⬜ |
| J7-01 | [Système d'enseignes et textures 2D procédurales](tasks/J7.md) | Opus 5.5 | `high` | J5-02, J4-07 | ⬜ |
| J7-02 | [Enseigne géante du DJ](tasks/J7.md) | Opus 5.5 | `high` | J7-01, J2-05 | ⬜ |
| J7-03 | [Trafic : socle](tasks/J7.md) | Opus 5.5 | `high` | J5-02 | ⬜ |
| J7-04 | [Événements du trafic, lot A (calmes et urbains)](tasks/J7.md) | Sonnet 5.5 | `high` | J7-03 | ⬜ |
| J7-05 | [Événements du trafic, lot B (vol et démesure)](tasks/J7.md) | Sonnet 5.5 | `high` | J7-03 | ⬜ |
| J7-06 | [Directeur des cascades](tasks/J7.md) | Opus 5.5 | `high` | J5-06 | ⬜ |
| J7-07 | [Scripts de cascades](tasks/J7.md) | Sonnet 5.5 | `high` | J7-06 | ⬜ |
| J7-08 | [Landmarks : registre et bâtiments spéciaux](tasks/J7.md) | Sonnet 5.5 | `high` | J5-02 | ⬜ |
| J7-09 | [Landmarks vitrines](tasks/J7.md) | Sonnet 5.5 | `medium` | J7-08 | ⬜ |
| J7-10 | [Météores et impacts](tasks/J7.md) | Sonnet 5.5 | `high` | J5-02, J5-03 | ⬜ |
| J7-11 | [Pré-intro et cartons-titres](tasks/J7.md) | Sonnet 5.5 | `high` | J5-03, J4-01, J2-05 | ⬜ |
| J8-01 | [Vue Live](tasks/J8.md) | Opus 5.5 | `high` | J2-05, J3-06, J4-05 | ⬜ |
| J8-02 | [Préparation : événement, créneaux, cues](tasks/J8.md) | Sonnet 5.5 | `high` | J8-01, J2-07 | ⬜ |
| J8-03 | [Éditeur de looks avec aperçu](tasks/J8.md) | Sonnet 5.5 | `high` | J8-02, J4-08 | ⬜ |
| J8-04 | [MIDI learn et profils de contrôle](tasks/J8.md) | Sonnet 5.5 | `medium` | J3-02, J8-01 | ⬜ |
| J8-05 | [Aperçu de la sortie dans la fenêtre opérateur](tasks/J8.md) | Opus 5.5 | `medium` | J8-01, J4-06 | ⬜ |
| J8-06 | [Caméra libre de préparation](tasks/J8.md) | Sonnet 5.5 | `low` | J6-01 | ⬜ |
| J9-01 | [Tests d'endurance](tasks/J9.md) | Opus 5.5 | `high` | J8-01, J7 terminé | ⬜ |
| J9-02 | [Pertes de sources et replis](tasks/J9.md) | Opus 5.5 | `high` | J3-03, J2-05 | ⬜ |
| J9-03 | [Paquets d'installation](tasks/J9.md) | Sonnet 5.5 | `medium` | J0-01 | ⬜ |
| J9-04 | [Guide de l'opérateur](tasks/J9.md) | Sonnet 5.5 | `medium` | J8 terminé | ⬜ |
| J9-05 | [Campagne de performance sur le matériel cible](tasks/J9.md) | Opus 5.5 | `high` | J9-01 | ⬜ |

## Vagues de parallélisation

Tâches d'une même vague = indépendantes entre elles (dépendances toutes dans les vagues précédentes). Si deux tâches
d'une vague touchent le même crate (ex. J1-02 et J1-03 dans `cf-audio`), les lancer en parallèle reste possible mais
la seconde PR devra intégrer la première (fusion de `main` dans sa branche).

| Vague | Tâches |
| --- | --- |
| 1 | J0-01 (Opus), J0-03 (Haiku) |
| 2 | J0-02 (Sonnet), J0-04 (Haiku), J1-01 (Opus), J1-02 (Opus), J1-03 (Opus), J2-01 (Opus), J2-03 (Sonnet), J2-06 (Sonnet), J3-01 (Opus), J4-03 (Opus), J9-03 (Sonnet) |
| 3 | J1-04 (Opus), J2-02 (Sonnet), J3-02 (Opus), J3-05 (Sonnet), J4-01 (Opus), J4-05 (Sonnet) |
| 4 | J1-05 (Sonnet), J1-07 (Sonnet), J2-04 (Opus), J4-02 (Opus), J4-04 (Opus), J4-08 (Sonnet), J5-03 (Sonnet) |
| 5 | J1-06 (Sonnet), J2-05 (Opus), J3-03 (Opus), J4-06 (Sonnet) |
| 6 | J2-07 (Sonnet), J2-08 (Sonnet), J3-04 (Sonnet), J3-06 (Sonnet), J4-07 (Opus), J5-01 (Opus), J7-11 (Sonnet), J9-02 (Opus) |
| 7 | J5-02 (Opus), J5-04 (Sonnet), J5-05 (Sonnet), J8-01 (Opus) |
| 8 | J5-06 (Opus), J5-07 (Sonnet), J7-01 (Opus), J7-03 (Opus), J7-08 (Sonnet), J7-10 (Sonnet), J8-02 (Sonnet), J8-04 (Sonnet), J8-05 (Opus) |
| 9 | J6-01 (Opus), J7-02 (Opus), J7-04 (Sonnet), J7-05 (Sonnet), J7-06 (Opus), J7-09 (Sonnet), J8-03 (Sonnet) |
| 10 | J6-02 (Opus), J6-03 (Sonnet), J6-04 (Sonnet), J6-05 (Sonnet), J7-07 (Sonnet), J8-06 (Sonnet) |
| 11 | J9-01 (Opus), J9-04 (Sonnet) |
| 12 | J9-05 (Opus) |

**Chemin critique** : J0-01 → J1-03 → J1-04 → J1-05 → J1-06 → J5-01 → J5-05 → J5-06 → J6-01 → J6-02.
**Version utilisable en soirée** (ville + caméra intelligente + cues + MIDI) : J0 à J6 terminés, plus J8-01.
