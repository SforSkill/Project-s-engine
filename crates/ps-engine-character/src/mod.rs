//! Liquid-paint characters: procedural skinned humanoid, toon "liquid"
//! material, procedural animation and purely local cosmetic effects.
//!
//! Nothing here feeds back into gameplay: hitboxes stay those of
//! `engine-hitreg` (driven by the networked pose only) and every effect is
//! computed on the client from the state it already receives.
//! See `docs/characters.md`.

pub mod anim;
pub mod fx;
pub mod liquid;
pub mod mesh;
pub mod rig;
pub mod runtime;
pub mod style;
pub mod weapon_mesh;

pub use runtime::{CharEvent, CharState, CharacterFeed, FirstPerson};

/// Registers materials, resources and shared meshes. The per-frame systems
/// [`runtime::drive`] and [`fx::update_fx`] are scheduled by `main.rs`, right
/// after the camera (the first-person arms hang from the viewmodel).
pub fn plugin(app: &mut bevy::prelude::App) {
    use bevy::prelude::*;
    liquid::plugin(app);
    app.init_resource::<CharacterFeed>()
        .init_resource::<runtime::Characters>()
        .init_resource::<fx::LiquidFx>()
        .add_systems(Startup, runtime::setup_assets);
}
