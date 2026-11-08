# Intro refaite autour du portique et changement de DJ en voiture (projet 3)

Date : 2026-11-08. Suite de `2026-11-08-sequence-demarrage-arret-design.md` (projet 2, déjà livré : `RunOrder`,
`RunPhase`, `RunSequencer`, plan `waiting`, `collapse` des façades, extinction « télé » de la voiture).

> Ce qui vient de l'utilisateur est écrit tel quel. Ce qui est **[mon choix]** est une décision que j'ai prise sans qu'il
> l'ait demandée : à relire en priorité.

## 1. Intention

Le show se joue pendant un ou plusieurs sets de DJs. L'utilisateur veut que le passage d'un DJ à l'autre soit un vrai
moment de spectacle, et que l'ouverture soit une vraie intro :

1. **Attente** : la caméra tourne lentement autour d'un point de la route, sans immeuble ni boîte (déjà en place).
2. **Démarrer** (bouton de l'interface de gestion) : la voiture se matérialise sur la route par une belle animation et
   roule (10-15 s). La caméra la suit et trouve un bon plan.
3. **Portique** : la voiture passe sous un gros panneau qui surplombe la route. Le nom du DJ actuel y est écrit (5 s).
4. **Quartier** : après le panneau, les éléments du quartier se matérialisent (10-15 s), puis tout se déroule comme
   aujourd'hui.
5. **Changer de DJ** (bouton de l'interface de gestion) : voir §3.
6. Le cycle se répète à chaque nouveau DJ.

Succès : l'opérateur voit une séquence marquante, sans à-coup, répétable autant de fois qu'il y a de DJs, et le public
comprend « un nouveau DJ prend la route » sans lire de texte à l'écran.

## 2. Objectifs, non-objectifs, anti-objectifs

Objectifs : nouvel ordre de l'intro (voiture, puis portique, puis quartier) ; transition de DJ pilotée par l'opérateur ;
ordre des DJs modifiable ; palette cohérente par DJ ; tout dans le monde 3D.

Non-objectifs : annulation d'une séquence en cours (décision du projet 2, conservée) ; sélecteur de couleur libre ;
mode d'affichage supplémentaire ; changement de DJ automatique sur l'horloge ; plusieurs voitures simultanées en dehors
de la transition.

Anti-objectifs (échec même si ça marche) : un flash qui ne passe pas par le limiteur photosensible (règle 7) ;
un chargement d'asset ou une compilation de shader déclenchés par un clic (règle 8) ; une allocation par image
(règle 4) ; un immeuble dont la base remonte au-dessus de la route (mémoire « bâtiments profonds sous la route ») ;
un texte plein écran pour annoncer le DJ (décision §8 du projet 2 : le nom vit sur des panneaux du monde).

## 3. Comportement

### 3.1 Intro (remplace l'ordre de la phase `Starting`)

Ordre voulu : voiture, portique, quartier. Aujourd'hui les immeubles arrivent avant le nom ; on inverse.

| Étape | Durée visée | Déroulé |
|---|---|---|
| Matérialisation | ~4 s | La voiture se forme sur la route, phares qui clignotent puis s'allument. Musique en fondu. |
| Roulage | 10-15 s au total avec l'étape suivante | La voiture accélère ; la caméra quitte le plan d'attente et cadre un bon plan de suivi. |
| Portique | 5 s | Le portique est déjà visible au loin ; la voiture passe dessous ; le nom du DJ s'allume en glitch puis se stabilise. |
| Quartier | 10-15 s | Après le passage, les éléments du quartier se matérialisent (cascade existante), dans la palette du DJ. |
| Fin | | `Running`, le show s'arme comme aujourd'hui. |

**[mon choix]** Les durées exactes vivent dans la table de constantes de `run/timeline.rs` (comme au projet 2),
fixes (pas d'accélération par l'énergie), pour que l'opérateur puisse planifier.

### 3.2 Le portique

Décidé avec l'utilisateur : « les deux ont l'air bien, ce serait bien un truc qui marque bien la chose », il m'a laissé
trancher. **[mon choix]** : un **portique métallique** planté dans la route, avec un **nom en néon géant**, plus une
touche d'hologramme à l'apparition.

- Le nom s'allume en glitch (comme un téléchargement), puis se stabilise en néon dans la palette du DJ.
- Il est planté comme un vrai portique (pieds sous la route) et se replie ou s'efface une fois la voiture passée.
- Il réutilise ce qui existe : `djsign/` (lettres allumées une à une) et la peinture de `signs/`. Le nom reste sur un
  panneau du monde, jamais en surimpression.

### 3.3 Changement de DJ (déclenché depuis l'interface de gestion)

Style demandé : **cyberpunk** (réponse de l'utilisateur à « passage de relais propre ou petit bug dramatique »).

1. Un événement démarre : la voiture actuelle roule au sol et se range **sur le côté droit** de la route.
2. Pendant ce temps, le quartier **se replie à l'inverse de l'intro** (réutilise `collapse` du projet 2).
3. Une **nouvelle voiture**, aux couleurs du nouveau DJ, arrive **par l'arrière gauche** et vient à son niveau.
4. La voiture de droite **bugue / clignote pour s'éteindre** et ralentit. La nouvelle voiture passe devant.
5. La caméra se met à **suivre la nouvelle voiture**.
6. On relance l'animation du portique avec le nom du nouveau DJ, puis le quartier réapparaît **avec le thème de
   couleurs du nouveau DJ**, et « c'est reparti ».

Cela vaut pour chaque DJ suivant, sans limite autre que la liste.

**[mon choix]** Le glitch de la voiture mourante est géométrique et lumineux local ; toute variation de luminance forte
passe par `ShowFxInput` → `FlashLimiter` (règle 7), avec `reduce_motion` qui supprime le clignotement.

**[mon choix]** Le cycle de reprise (voiture, portique, quartier) est le même qu'au démarrage ; seule l'arrivée de la
voiture diffère (matérialisation au démarrage, arrivée par l'arrière gauche au changement).

### 3.4 Opérateur : choisir le DJ suivant

Décidé avec l'utilisateur : l'ordre est **prévu à l'avance et modifiable par l'opérateur**.

- Bouton principal **« DJ suivant »** : lance la transition vers le DJ en tête de la liste.
- L'opérateur peut **réordonner** les DJs restants avant de lancer. Une transition déjà en cours ne se modifie pas.
- **[mon choix, à confirmer]** Il peut aussi cliquer sur un DJ précis pour le placer en tête.
- Au dernier DJ le bouton est grisé ; il reste « Arrêter ».
- La liste existe déjà (`DjSlot` dans `cf-show`, `tabs/settings/dj_list.rs`, `cockpit/`) : on s'appuie dessus, on n'en
  crée pas une seconde.

### 3.5 Palettes

Décidé avec l'utilisateur : **« palette de présélections cohérente et jolie »**.

- Une dizaine de palettes cyberpunk assorties. Chacune fixe d'un coup la voiture, le quartier et le néon du nom.
  **[mon choix]** Elles étendent les 6 thèmes existants (`themes.ron`, `ThemeLibrary`, `Theme` dans `cf-show`) plutôt
  qu'un nouveau système : `DjIdentity.theme` existe déjà.
- Attribution automatique à chaque DJ qui n'en a pas, **jamais la même pour deux DJs consécutifs**. L'opérateur peut la
  changer dans la fiche du DJ. **[mon choix]** Pas de sélecteur de couleur libre.
- Le fondu du quartier utilise le mécanisme existant (`PaletteMorph`, `ThemeFader`).

## 4. Contraintes

- Règles `CLAUDE.md` : 4, 7, 8, 11 (fichiers < 400 lignes), commentaires en français, identifiants en anglais, pas de
  nouveau `unwrap`. Pas de `NoFrustumCulling` sans commentaire.
- Contrat dans `cf-render` : `RunOrder` reçoit une variante de changement de DJ ; `cf-operator` ne dépend pas de
  `cf-world-neon`. **[mon choix]** `RunPhase` gagne des phases de transition (`Handover`), traitées comme `Starting` /
  `Stopping` : ordres ignorés pendant leur déroulé, bouton grisé.
- Les deux voitures coexistent pendant la transition : la seconde est préchargée à `Startup` (ou le rig existant est
  dupliqué) ; **aucun spawn d'asset pendant le show**. Le focus de streaming (`StreamFocus`) bascule de l'une à
  l'autre sans trou.
- Photosensibilité : aucune variation plein écran > 3 Hz ; `reduce_motion` allonge les rampes ×1,5.

## 5. Tests attendus

- Machine pure (sans Bevy) : l'ordre des étapes de l'intro ; la transition de DJ (ordres ignorés en transitoire,
  reprise identique au démarrage, répétable sur N DJs, retour exact à l'état de départ sans dérive ni NaN).
- Palettes : jamais la même palette pour deux DJs consécutifs ; la palette choisie à la main l'emporte.
- Façade : la base ne dépasse jamais `-BASE_DEPTH_M` au repli comme à la reconstruction.
- Limiteur : aucune étape ne produit un delta de luminance au-delà du seuil.
- Opérateur : « DJ suivant » émet le bon ordre ; la liste réordonnée est respectée ; grisé au dernier DJ et en
  transitoire.
- Non vérifiable ici (pas d'écran) : le rendu et le ressenti, à dire dans le compte rendu final.

## 6. Précisions après relecture (tous **[mon choix]**)

- **Durée du changement de DJ** : ~25 s en tout (repli du quartier et arrivée de la 2e voiture ~10 s, dépassement et
  extinction ~5 s, portique 5 s, reconstruction ~10 s). Constantes réglables dans `run/timeline.rs`, fixes.
- **Qui porte la caméra et le focus de streaming** : l'ancienne voiture jusqu'au dépassement ; au dépassement (étape 5)
  `StreamFocus` et le plan du réalisateur passent à la nouvelle, d'un coup, sans trou de streaming. La nouvelle voiture
  est à vitesse de croisière quand elle passe devant.
- **Deux voitures qui alternent** : deux rigs préchargés à `Startup`, qui échangent leurs rôles (A/B) à chaque
  changement. L'ancienne est masquée et remise à la pose de repos à la fin ; pas de respawn. La machine pure sort donc
  les étapes (`car_presence`, vitesse) pour les deux rigs.
- **Cible du changement** : `RunOrder::NextDj(SlotId)` ; la liste et son ordre restent dans `cf-show`, le monde reçoit
  l'identifiant et lit nom et thème dans le show courant. Le choix « cliquer un DJ pour le mettre en tête » n'envoie
  que cet identifiant, il ne coûte rien de plus.
- **Palettes** : fournies avec le code (données `cf-show`/`themes.ron`), les 6 thèmes existants restent inchangés.
  L'attribution automatique est calculée à l'ouverture du show et enregistrée dans le fichier RON ; `DjIdentity.theme`
  reste optionnel.
- **Palette pendant le repli** : changée à l'instant où le quartier est entièrement replié (donc invisible), jamais en
  fondu visible ; la voiture et le portique prennent la nouvelle palette au même moment. Aucun saut de luminance visible,
  donc rien de spécial pour le limiteur ici.
- **Portique** : placé relativement à la voiture au début de l'étape (quelques dizaines de mètres devant), entité
  autonome avec `Aabb` borné, hors flux du streaming ; pas de `NoFrustumCulling`.

## 6 bis. Arrêt, redémarrage et DJ actuel (décidé par l'utilisateur)

- Après « Arrêter » puis « Démarrer », le show repart avec **le DJ en cours**, pas avec le premier de la liste.
- Dans la liste, l'opérateur peut **désigner n'importe quel DJ comme « actuel »**, y compris avant de démarrer. Si
  il remet le premier DJ comme actuel, le show repart avec le premier. Le DJ actuel est donc un simple pointeur
  modifiable ; il n'y a pas de réinitialisation automatique.
- Conséquence **[mon choix]** : « DJ suivant » passe au DJ qui suit l'actuel dans la liste ; « cliquer un DJ » le
  désigne comme prochain pendant un set, ou comme actuel à l'arrêt. `reset_show` ne touche pas ce pointeur. Le bouton
  « DJ suivant » est grisé quand l'actuel est le dernier de la liste.

## 7. Laissé au constructeur

Courbes et trajectoires exactes (arrivée par l'arrière gauche, rangement à droite, dépassement) ; forme précise du
portique et du glitch du nom ; choix duplication du rig de voiture ou seconde voiture dédiée ; nombre et valeurs des
palettes ; placement des nouvelles phases dans `RunPhase` ; nom et place des nouveaux boutons dans le cockpit.
