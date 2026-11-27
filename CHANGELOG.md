# Changelog

Toutes les versions de `Project-s-engine`. Format : [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/), versions [SemVer](https://semver.org/lang/fr/).

## [0.1.1] - 2026-11-27

### Ajouté
- `rig::Pose::skin_matrix` et `rig::JointXf::matrix` publics (contrôle du skinning côté CPU, utilisé par le test peau ↔ hitboxes du client).

## [0.1.0] - 2026-11-27

Première version, extraite de `game-client/src/character` de `SforSkill/Projet-S` (historique conservé). Rendu inchangé.

### Modifié
- Le module devient la crate `ps-engine-character`, sans dépendance au jeu : `Hold::from_kind`, `CharacterSettings` (palettes par équipe) et le pilotage par l'état réseau (`runtime.rs`) restent dans le client.
- `fx::Rng::next` → `next_unit`.
