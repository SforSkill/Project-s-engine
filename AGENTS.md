# Project-s-engine (contexte agents)

Briques de présentation Bevy génériques. Lire [`SCOPE.md`](SCOPE.md).

- Aucune connaissance du jeu : pas d'équipe, d'arme, de hitbox ni de réseau (`boundaries.toml`). Le client fait la traduction.
- Direction artistique : silhouettes humaines stables, matière liquide opaque, rendu 3D à l'aspect 2D, contours légèrement nuancés, couleurs d'équipe distinctes (`docs/characters.md`).
- Gros fichiers à lire par plage : `mesh.rs`, `anim.rs`.

Vérification : `cargo fmt --all -- --check && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --release --locked`.
