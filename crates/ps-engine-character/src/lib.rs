//! Procedural "liquid paint" humanoids for Bevy: skeleton, skinned mesh with
//! levels of detail, procedural animation, toon liquid material and local
//! cosmetic effects (drops, puddles).
//!
//! Purely presentational: nothing here knows about teams, weapons, hitboxes
//! or the network. The game drives characters through its own runtime
//! (Project-s `ps-client`, `character/runtime.rs`) and maps its rules
//! (teams, held weapon) onto [`style::PaletteId`] and [`anim::Hold`].
//! See `docs/characters.md`.

pub mod anim;
pub mod fx;
pub mod liquid;
pub mod mesh;
pub mod rig;
pub mod style;
pub mod weapon_mesh;

/// Registers the liquid material and the effect resources. The game schedules
/// [`fx::update_fx`] and its own character systems.
pub fn plugin(app: &mut bevy::prelude::App) {
    liquid::plugin(app);
    app.init_resource::<fx::LiquidFx>();
}
