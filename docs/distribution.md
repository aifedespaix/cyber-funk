# Distribution : paquets d'installation

Tâche J9-03. Le workflow `.github/workflows/release.yml` fabrique, sur un tag `v*`, un paquet par système. La
configuration de paquetage est dans `packager.json` (racine).

## Choix de l'outil : cargo-packager

`cargo-dist` (désormais « dist ») ne sait produire ni `.dmg` ni AppImage : ses formats sont des archives
`.tar.xz`/`.zip`, des scripts d'installation shell/PowerShell, `.msi`, npm et Homebrew. `cargo-packager` (0.11.8,
vérifié sur crates.io et dans son schéma de configuration) couvre exactement la demande : NSIS (`.exe`), DMG,
AppImage. Sa configuration est en camelCase (`packager.json`, `deny_unknown_fields`) ; le fichier est validé contre le
`schema.json` officiel. La version est épinglée dans le workflow (`CARGO_PACKAGER_VERSION`), car le format change entre
versions.

| Système | Runner | Paquet | Contenu |
| --- | --- | --- | --- |
| Windows x86_64 | `windows-latest` | `cyber-funk_<v>_x64-setup.exe` (NSIS, installation par utilisateur) | binaire + `assets/` côte à côte |
| macOS arm64 | `macos-latest` | `.dmg` (nom fixé par cargo-packager) | `cyber-funk.app`, `assets/` dans `Contents/Resources` |
| Linux x86_64 | `ubuntu-22.04` | `cyber-funk_<v>_x86_64.AppImage` et `cyber-funk-v<v>-linux-x86_64.tar.gz` | tar.gz : binaire + `assets/` côte à côte ; AppImage : `usr/bin/assets` |

macOS Intel et Linux arm64 ne sont pas produits (pas de besoin exprimé ; ajouter une ligne de matrice au besoin).

## Les assets doivent être trouvés au démarrage

Bevy 0.20 cherche `assets/` dans `BEVY_ASSET_ROOT`, sinon `CARGO_MANIFEST_DIR`, sinon **le dossier du binaire**
(`bevy_asset::io::file::get_base_path`). Les paquets Windows, tar.gz et AppImage placent donc `assets/` à côté du
binaire et fonctionnent sans code. **Dans le `.app` macOS, `cargo-packager` ne sait placer les ressources que dans
`Contents/Resources`, alors que le binaire est dans `Contents/MacOS`** : Bevy ne les y trouvera pas tant que
l'application ne résout pas elle-même la racine des assets (par exemple `AssetPlugin { file_path }` calculé depuis
`current_exe()` : `../Resources/assets` quand le binaire est dans un `.app`). Ce changement touche `main.rs` (hors
périmètre de J9-03) : à faire dans une tâche dédiée avant de livrer le `.dmg`.

## Garde de licence : la release reste privée

La licence de la CyberCar (`assets/models/car/`) est inconnue (voir `assets/README.md`). Le workflow :

- crée toujours la release en **brouillon et pré-version** (jamais publique d'office) ;
- émet un avertissement tant que `assets/README.md` contient « À confirmer » ;
- refuse de produire un paquet si `assets/music/local/` n'est pas vide (pistes personnelles non distribuables).

Publier = relire la release brouillon sur GitHub, puis cliquer sur « Publish ». Ne le faire qu'après avoir noté la
licence du modèle dans `assets/README.md`.

## Tester sur un tag (à faire par le propriétaire)

Ce test n'a pas pu être exécuté depuis la session de développement (pas de tag poussé, pas de machine Windows ni
macOS).

1. Vérifier que la version de `Cargo.toml` et celle de `packager.json` sont identiques (le workflow le contrôle).
2. `git tag v0.1.0-test && git push origin v0.1.0-test` (le suffixe `-test` est accepté ; le tag doit commencer par
   `v` + la version du workspace).
3. Onglet Actions, workflow « Release » : les trois jobs `package` doivent être verts. Si `cargo packager` échoue,
   les journaux disent quelle clé est refusée ; ajuster `packager.json`.
4. Onglet Releases : un brouillon `v0.1.0-test` contient `.exe`, `.dmg`, `.AppImage`, `.tar.gz` et `SHA256SUMS.txt`.
5. Installer sur chaque OS et lancer l'application : une fenêtre s'ouvre, les modèles et textures se chargent
   (journal sans erreur d'asset). Sur macOS, voir la section précédente.
6. Supprimer le brouillon et le tag de test : `git push --delete origin v0.1.0-test`.

L'icône `packaging/icons/icon.png` est un **emplacement réservé** généré (anneau néon) ; à remplacer par le vrai logo
(512x512 minimum, PNG).

## Signature (sans aucun secret dans le dépôt)

Par défaut, les paquets sont **non signés**. Les identifiants vivent dans les *secrets* du dépôt GitHub (Settings,
Secrets and variables, Actions), jamais dans `packager.json` ni dans le workflow en clair.

### macOS : signature et notarisation

Sans elles, Gatekeeper bloque l'ouverture (« développeur non identifié ») ; contournement manuel : clic droit, Ouvrir.
Pour signer, il faut un compte Apple Developer (99 USD/an) et un certificat « Developer ID Application » exporté en
`.p12`. `cargo-packager` lit des variables d'environnement (vérifier les noms dans sa documentation à la version
épinglée) :

- `APPLE_CERTIFICATE` (le `.p12` en base64) et `APPLE_CERTIFICATE_PASSWORD` ; l'identité de signature se donne dans `macos.signingIdentity` de `packager.json` (ce n'est pas un secret) ;
- pour la notarisation : `APPLE_ID`, `APPLE_PASSWORD` (mot de passe d'application) et `APPLE_TEAM_ID`, ou une clé API
  App Store Connect (`APPLE_API_KEY`, `APPLE_API_ISSUER`, `APPLE_API_KEY_PATH`).

À ajouter dans l'étape « Créer les paquets » du job macOS via `env:` avec `${{ secrets.NOM }}`, une fois les secrets
créés. Ne pas les définir à vide : une variable vide peut être lue comme « présente ».

### Windows : SmartScreen

Un installateur non signé déclenche l'avertissement SmartScreen (« Windows a protégé votre PC » ; contournement :
Informations complémentaires, Exécuter quand même). La réputation s'acquiert avec le temps et les téléchargements ;
un certificat de signature de code (OV, ou EV pour une réputation immédiate) ou un service de signature cloud (Azure
Trusted Signing, SSL.com eSigner) la raccourcit. `cargo-packager` accepte une commande de signature dans la
section `windows` de la configuration (`certificateThumbprint`, `signCommand`) : brancher le certificat depuis un
secret du dépôt, jamais en clair.

### Linux

Pas de signature obligatoire. Les `SHA256SUMS.txt` de la release permettent de vérifier les téléchargements.
