# Prompt de session : chantiers visuels de cyber-funk

Tu poursuis le développement visuel de cyber-funk (application de VJing 3D temps réel, Rust et Bevy). Ce même
prompt est envoyé à une session neuve après l'autre : tu n'as aucun souvenir des sessions d'avant. Toute la
mémoire est dans le dépôt.

## Ce que tu fais dans cette session

**Un chantier, un seul, en entier.**

1. Lis `CLAUDE.md`, puis `docs/worldbuilding/CHANTIERS.md` : les règles communes, les arbitrages, le tableau
   d'état et le journal.
2. Choisis ton chantier : celui qui est 🟨 s'il y en a un (reprends au point noté dans le journal), sinon le
   premier ⬜ dans l'ordre. Si tous sont ✅, ne code rien : réponds que le plan est terminé et liste ce que le
   tableau signale encore à vérifier sur le matériel cible.
3. Passe-le à 🟨 dans le tableau, commit, push. Une session interrompue laisse ainsi une trace.
4. Lis seulement ce que la fiche du chantier cite : les sections des documents sources, le code existant, les
   skills. Lis le code existant **avant** d'écrire : la règle du plan est d'étendre ce qui existe, pas de le doubler.
5. Réalise tout le « À faire », puis vérifie chaque critère d'acceptation avec sa preuve.
6. Termine comme décrit plus bas.

Ne commence pas le chantier suivant, même s'il reste du temps : relis plutôt le tien, complète ses tests et ses
captures.

## Git

Tu travailles directement sur `main` : commit et push sur `main` sont autorisés, sans branche ni pull request.
Cela remplace, pour ces chantiers, la règle « une tâche = une branche = une PR » de `CLAUDE.md`. Si la session
t'impose une autre branche de travail, développe dessus puis pousse le résultat sur `main`
(`git push origin HEAD:main`).

- `git pull --rebase origin main` avant de commencer et avant chaque push.
- Commits fréquents, en français, un par étape cohérente. Un commit poussé compile toujours.
- Avant chaque push : `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo nextest run --workspace` (ou `cargo test --workspace`). Tout est vert, sinon tu ne pousses pas.
  Entre deux, itère sur ton crate (`-p <crate>`).
- Ne modifie pas `docs/TASKS.md` ni `prompt.md`.

## Ce qui ne se négocie pas

- Les règles 🔴 de `CLAUDE.md` §1, en particulier : limiteur photosensible pour toute variation de luminance,
  aucune allocation par image, aucun chargement d'asset ni compilation de shader pendant un show, pas de
  `NoFrustumCulling` sans justification, `cf-audio`, `cf-analysis`, `cf-control` et `cf-show` sans Bevy, fichiers de
  400 lignes au plus.
- Tu ne désactives, n'ignores et ne supprimes aucun test pour passer au vert. Si un test de parité avec Sillage
  casse parce que le chantier change volontairement le rendu, tu mets à jour le test ou sa référence dans le même
  commit et tu l'écris dans le message.
- Les formules et constantes des documents sont des valeurs de départ. Si une formule est fausse ou ne s'applique
  pas au code réel, corrige-la, note l'écart dans le journal, et avance. Ne reste pas bloqué sur un document.
- Si tu es bloqué deux fois sur le même critère, arrête d'itérer dessus : note-le dans le journal avec ce que tu as
  essayé, et finis le reste du chantier.

## Vérifier à l'image

Il n'y a ni écran ni carte graphique : utilise Xvfb et le rendu logiciel, comme les captures déjà présentes dans
`docs/captures/`. Dépose tes captures dans `docs/captures/C<n>/` et regarde-les avant de conclure : une capture
noire, vide ou identique à l'avant n'est pas une preuve. Dis clairement ce que le rendu logiciel n'a pas permis de
juger (cadence, bloom fin, couleurs sur projecteur).

## Finir la session

1. Mets à jour le tableau d'état de `CHANTIERS.md` : ✅ si tous les critères sont tenus ou explicitement
   consignés comme non tenus, sinon 🟨. Renseigne le dernier commit et ce qui reste à vérifier sur le matériel cible.
2. Ajoute une entrée en haut du journal : date, chantier, ce qui est fait, décisions prises (avec leur raison),
   écarts aux documents, critères non tenus, et le point de reprise précis si le chantier reste 🟨.
3. Coche les critères d'acceptation dans la fiche du chantier.
4. Commit et push sur `main`.
5. Ta réponse finale, courte : le chantier traité, son statut, ce qui a été vérifié et comment, ce qui ne l'a pas
   été, et le chantier que la prochaine session prendra.

Si le contexte ou le temps vient à manquer avant la fin, fais les étapes 1 à 4 tout de suite avec le statut 🟨 :
un chantier à moitié fait mais bien consigné vaut mieux qu'un chantier fini dont la session suivante ne sait rien.
