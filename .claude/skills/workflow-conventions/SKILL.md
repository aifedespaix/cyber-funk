---
name: workflow-conventions
description: Orchestrator only (not for task agents) - choosing the model and effort for a cyber-funk task, launching agents in parallel, escalating, reviewing and updating task statuses. Use when starting or supervising tasks from docs/TASKS.md.
---

# Workflow de l'orchestrateur - cyber-funk

La source de vérité est `docs/TASKS.md` (modèle et effort de chaque tâche, règles d'économie, vagues). Ce skill n'en garde
que la mécanique.

## Lancer

1. Prendre une tâche dont toutes les dépendances sont ✅ (une vague, **3 agents au plus**, dont 2 qui compilent Bevy).
2. Agent avec le modèle et l'effort du tableau, worktree isolé, prompt de trois lignes (voir `docs/TASKS.md`).
3. Dépôts attachés : `cyber-funk` et, pour le portage, `aifedespaix.com` (lecture seule).

## Modèle : la règle

- **Opus** : la tâche *définit* un contrat (trait, format de fichier, protocole entre threads, machine à états).
- **Sonnet** : elle *remplit* un contrat, avec des tests ou des critères mesurables. C'est le défaut.
- **Haiku** : port mécanique à patron connu, d'abord en essai.
- **Escalade** : deux échecs sur le même critère (clippy, test, perf) → relancer au modèle du dessus. Ne pas laisser un
  agent itérer.
- **Effort** : `medium` pour un port gardé par des tests, `high` pour rendu / WGSL / perf / concurrence, `xhigh` jamais
  sans raison écrite.

## Superviser

- Interruptions (limite de session) : le travail reste dans le worktree ; reprendre avec `SendMessage` vers l'agent.
  Avant de reprendre, vérifier le disque (`df -h`) et la place du dossier `target`.
- Revue : CI + critères de la fiche d'abord. Revue Opus (`code-review`, `high`) seulement pour J1-03, J2-05, J2-06,
  J3-03, J4-02.
- Après chaque fusion : passer la ligne de `docs/TASKS.md` à ✅ (les agents n'y touchent pas).
- Conflits `Cargo.lock` : merger `main` dans la branche et laisser cargo le regénérer.

## Docs à jour avant de lancer

Context7 (`upstash/context7`) donne les API à jour de Bevy, wgpu, cpal et midir ; sans lui, lire les sources dans
`~/.cargo/registry/src/*/bevy*-<version>/` plutôt que de se fier à sa mémoire.
