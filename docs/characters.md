# Personnages « peinture liquide »

Les cubes des joueurs sont remplacés par des humanoïdes low-poly à facettes. Leur
matériau imite une peinture liquide. Tout est dans `crates/game-client/src/character/`.
La décision et ses compromis sont décrits dans
[ADR 0005](decisions/0005-liquid-characters.md).

## Garanties compétitives

- **Hitboxes inchangées** : `engine-hitreg` les calcule à partir de la pose
  réseau (`origin`, `yaw`, `duck`). Le modèle visuel n'en lit rien et n'y écrit
  rien.
- **Effets purement locaux** : les gouttes et les flaques n'ont ni collision ni
  hitbox, et ne projettent pas d'ombre. Rien n'est envoyé sur le réseau.
- **Même lisibilité à toutes les qualités** : silhouette, taille, contour (rim)
  et flash d'impact sont identiques du niveau Faible au niveau Ultra. Seuls les
  détails secondaires changent : ondulations internes, arêtes, nombre de gouttes.
- **Palettes équitables** : elles ne changent que la teinte. Le contraste
  contour/corps est vérifié par le test `contours_stand_out_from_bodies`, et deux
  camps n'ont jamais la même palette (test `teams_never_share_a_palette`).
- **Aucune vision à travers les murs** : le matériau reste opaque et soumis au
  test de profondeur normal.
- **Réapparition** : un simple reflet. Le corps n'est jamais partiellement
  invisible, ce qui serait un avantage.

## Modules

| Fichier | Rôle |
|---|---|
| `rig.rs` | Squelette de 22 articulations, pose de liaison (unités du jeu, -Z devant) |
| `mesh.rs` | Maillage humanoïde procédural (3 LOD, version « avant-bras » pour la 1re personne), goutte, flaque |
| `anim.rs` | Animation procédurale : repos, marche, course, pas latéral, recul, accroupi, saut/chute/atterrissage, visée (tangage), tir, rechargement, changement d'arme, couteau, grenade, pose/désamorçage, impact, mort, réapparition. IK à deux os pour les bras et les jambes, transitions lissées |
| `liquid.rs` + `liquid.wesl` | Matériau `LiquidMaterial` (extension de `StandardMaterial`) : normales lissées avec une légère facette (`FACET_SMOOTHING` dans `mesh.rs`), bandes de lumière toon, Fresnel discret, reflets, ondulations, fonte/flash/reflet par personnage |
| `style.rs` | Palettes, niveaux de qualité, `CharacterSettings` (sauvés dans `settings.toml`) et `LiquidStyle`, l'unique endroit où se règlent les paramètres du shader |
| `fx.rs` | Gouttes et flaques en pool, plafonnées par la qualité, émises à moins de 40 m de la caméra |
| `runtime.rs` | Systèmes ECS : `CharacterFeed` (rempli par `game.rs`) → personnages, LOD, bras en 1re personne, transitions, effets |
| `weapon_mesh.rs` | Armes low-poly solides (couleurs gunmetal, matériau distinct du liquide) |

`game.rs` ne fait que remplir `CharacterFeed` à partir de l'état réseau déjà reçu
(joueurs distants, état local, événements de tir, d'impact, de pas et
d'atterrissage). Le gameplay et le protocole ne changent pas.

## Réglages joueur

Paramètres → **Personnages** :
- qualité (Faible, Moyenne, Élevée, Ultra) ;
- mode de couleur : « Par camp » (défense / attaque) ou « Alliés / ennemis »
  (couleurs choisies par le joueur, quel que soit le camp ; en deathmatch, tous
  les autres joueurs sont des ennemis) ;
- les deux palettes : bleu, rouge, émeraude, violet, jaune, cyan, orange, blanc,
  noir.

Bleu (`#2563EB`, `#1D4ED8`, `#93C5FD`) et rouge (`#EF4444`, `#B91C1C`,
`#FCA5A5`) reprennent exactement les valeurs de la direction artistique (test
`brief_colours_are_exact`).

## Performance

- Maillages partagés par tous les personnages. Les matériaux sont partagés par
  (palette, qualité, usage). Un matériau n'est « prêté » à un personnage que
  pendant un effet (fonte, flash, réapparition), puis rendu.
- LOD par `VisibilityRange` : 2 172 / 1 604 / 660 triangles, distances selon la
  qualité. Les bascules sont nettes, car le shader n'implémente pas le fondu
  tramé.
- Pools de gouttes : 0 / 48 / 128 / 256 selon la qualité ; 12 flaques au
  maximum, 5 s chacune.
- Mesure de la démo (`report.json`, 33 personnages animés, fenêtre 1600×900, GPU
  local) : environ 2,7 ms par image en moyenne et moins de 3,5 ms au p95, aux
  quatre niveaux.

## Démo

```
Project-S.exe --character-demo
Project-S.exe --character-demo --shots <dossier>   # captures + report.json, puis quitte
```

La démo propose :
- une rotation à 360° (automatique, ou souris : glisser pour tourner, molette
  pour zoomer) ;
- toutes les animations, armes et effets ;
- les palettes, le mode de couleur et la qualité ;
- les curseurs du shader ;
- la superposition des hitboxes ;
- l'alignement en distance (LOD) et les zones sombre et claire ;
- un aperçu de la première personne ;
- une foule pour le test de charge ;
- des statistiques : triangles, gouttes, flaques, matériaux.

Elle ne modifie pas `settings.toml`.

## Limites connues

- Les pieds glissent un peu à la course : la cadence n'est pas calée sur la
  vitesse exacte.
- Pendant la fonte, le corps fondu projette encore son ombre et écrit le
  prépasse de profondeur.
- L'animation des personnages lointains n'est pas ralentie.
- Les effets de mort et d'impact dépendent des événements déjà reçus par le
  client : un impact non répliqué n'éclabousse pas.
- Les assets sont procéduraux (code Rust versionné, qui sert de source). Pas de
  fichier Blender ni d'export glTF pour l'instant.
- Les bras en première personne n'ont pas d'animation dédiée : ce sont les
  avant-bras de la pose « repos » du modèle, ancrés sur la poignée de l'arme.
