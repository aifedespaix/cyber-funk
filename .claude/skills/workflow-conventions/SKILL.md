---
name: workflow-conventions
description: Use when choosing which Claude model/effort to run a cyber-funk task with, when starting a task from docs/TASKS.md, or when preparing the commit/PR for a task.
---

# Workflow des agents - cyber-funk

## Choisir le modèle

| Type de travail | Modèle | Effort |
| --- | --- | --- |
| Architecture, contrats entre crates, concurrence (threads audio, lock-free), render graph, machine à états du show | **Opus 5.5** (`claude-opus-5-5`) | `high` à `xhigh` |
| Portage d'un module Sillage au contrat déjà fixé (plans caméra, événements, cascades, landmarks), UI egui, plugins Bevy « classiques » | **Sonnet 5.5** (`claude-sonnet-5-5`) | `medium` à `high` |
| Config, assets, doc, petites corrections mécaniques, renommages | **Haiku 5.5** (`claude-haiku-5-5`) | `low` à `medium` |
| Revue d'une PR critique (audio temps réel, show, rendu) | **Opus 5.5** | `high` (skill `code-review`) |

Règle simple : si la tâche **définit** un contrat (trait, format de fichier, protocole entre threads) → Opus. Si elle
**remplit** un contrat existant → Sonnet. Si elle ne demande aucune décision → Haiku.

Chaque fiche de `docs/tasks/J*.md` donne déjà le modèle et l'effort recommandés.

## Lancer une tâche (prompt type)

```
Tâche J1-04 du dépôt cyber-funk.
Lis CLAUDE.md, puis la fiche J1-04 dans docs/tasks/J1.md, charge les skills qu'elle cite,
lis les fichiers Sillage indiqués (dépôt aifedespaix/aifedespaix.com), puis réalise la tâche.
Branche task/J1-04-beat-detector, PR vers main avec le modèle de PR, critères d'acceptation cochés.
```

Le dépôt Sillage (`aifedespaix/aifedespaix.com`) doit être attaché à la session de l'agent pour les tâches de portage.

## Avant de pousser

1. `cargo fmt --all`
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
3. `cargo nextest run --workspace` (ou `cargo test --workspace`)
4. `cargo deny check` si des dépendances ont changé
5. Relire son diff comme un relecteur hostile : allocations dans une boucle chaude ? `unwrap` ? constante Sillage
   modifiée sans raison ? fichier > 400 lignes ?
6. Mettre à jour le statut dans `docs/TASKS.md`.

## Parallélisme

`docs/TASKS.md` indique les dépendances. Deux tâches sans dépendance entre elles et qui ne touchent pas le même crate
peuvent tourner en parallèle. Deux tâches sur le même crate : en série, ou bien coordonner les fichiers touchés.

## Skills externes utiles

- Context7 (`upstash/context7`) : docs à jour de Bevy, wgpu, cpal, midir. **Indispensable** : l'API de Bevy change
  à chaque version et la mémoire des modèles est souvent en retard d'une ou deux versions.
- `obra/superpowers` : méthode (TDD, débogage systématique) pour les tâches Opus longues.
