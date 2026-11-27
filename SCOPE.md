# Périmètre de Project-s-engine

## 1. Mission
Fournir les briques techniques de présentation d'un FPS natif sur Bevy/wgpu, réutilisables hors de Project S : aujourd'hui, les personnages procéduraux « peinture liquide ».

## 2. Responsabilités
| Crate | Contenu |
|---|---|
| `ps-engine-character` | Squelette (`rig`), maillage humanoïde skinné avec niveaux de détail (`mesh`), animation procédurale (`anim` : locomotion, postures, tenue d'objet, mort), matériau toon liquide WGSL/WESL (`liquid`), palettes et qualité (`style`), maillages d'objets tenus (`weapon_mesh`), effets cosmétiques locaux : gouttes, flaques (`fx`) |

## 3. Ne contient jamais
Comptes, matchmaking, sanctions, classement, saisons, serveurs officiels, économie, modes, équipes, armes du jeu, hitboxes, réseau. Le jeu traduit ses règles en paramètres de présentation (`PaletteId`, `Hold`).

## 4. Dépendances autorisées
Bevy (features de rendu 3D uniquement), `serde`.

## 5. Dépendances interdites
Toute crate Project S (protocole, simulation, netcode, serveur, client). `boundaries.toml`.

## 6. Interfaces publiques
`plugin(app)`, `anim::{Animator, AnimInput, Action, Hold}`, `mesh::{humanoid, Lod, Parts, MeshData}`, `rig::{Joint, Pose}`, `liquid::{LiquidMaterial, LiquidMaterials, CharacterStyle}`, `style::{Palette, PaletteId, Quality, LiquidStyle}`, `fx::{LiquidFx, Emit, Rng, update_fx}`, `weapon_mesh`.

## 7. Contrats
Repère du squelette : y en haut, -z vers l'avant, unités du jeu (le client applique l'échelle de rendu). Les poses sont dessinées autour des hitboxes du jeu ; le client vérifie cette cohérence par un test (`ps-client`, `character/hitbox_fit.rs`).

## 8. Performance
Un maillage partagé par niveau de détail, skinning GPU, effets plafonnés et recyclés, aucun calcul dans la boucle de simulation. Shaders légers (un seul matériau étendu).

## 9. Sécurité
Purement local et cosmétique : n'influence jamais hitboxes, réseau ou gameplay.

## 10. Compilation
`cargo build --workspace` (dépendances système Bevy sous Linux : ALSA, udev, Wayland, xkbcommon).

## 11. Tests
`cargo test --workspace --release` : longueurs de membres, pieds au-dessus du sol, mains sur l'arme, chute à la mort, couleurs de la direction artistique, contraste des contours.

## 12. Versionnement
SemVer, tag `vX.Y.Z`. La version de Bevy est épinglée ; une montée de Bevy est une version mineure (majeure après 1.0).

## 13. Déploiement
Bibliothèque intégrée au client.

## 14. Limites connues
Le reste de la présentation (caméras, couches de rendu, viewmodel, HUD, audio, maillage des cartes) est encore dans le client, entremêlé à l'état du jeu. Il rejoindra ce dépôt module par module, quand chacun aura une interface indépendante des règles (prochain candidat : l'audio procédural).
