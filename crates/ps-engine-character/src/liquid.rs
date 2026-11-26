//! Liquid paint material: `StandardMaterial` extended with `liquid.wesl`.
//!
//! Materials are shared: one per (palette, quality, use). Only a character
//! that is dying, respawning or flashing after a hit borrows a private copy
//! from a small pool, returned as soon as the effect ends.

use super::style::{LiquidStyle, Palette, PaletteId, Quality, linear};
use bevy::asset::{embedded_asset, load_embedded_asset};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::{Shader, ShaderRef};
use std::collections::HashMap;
use std::sync::OnceLock;

pub type LiquidMaterial = ExtendedMaterial<StandardMaterial, LiquidExt>;

static SHADER: OnceLock<Handle<Shader>> = OnceLock::new();

/// Uniform block of `liquid.wesl` (same field order).
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType, Reflect)]
pub struct LiquidParams {
    pub primary: Vec4,
    pub secondary: Vec4,
    pub contour: Vec4,
    /// contour intensity, highlight, roughness, opacity
    pub look: Vec4,
    /// ripple speed, deformation, light bands, quality
    pub motion: Vec4,
    /// melt, respawn front (< 0 = off), hit flash, seed
    pub state: Vec4,
}

impl LiquidParams {
    pub fn new(p: &Palette, style: &LiquidStyle, quality: Quality) -> Self {
        LiquidParams {
            primary: linear(p.primary).to_vec4(),
            secondary: linear(p.secondary).to_vec4(),
            contour: linear(p.contour).to_vec4(),
            look: Vec4::new(
                style.contour_intensity,
                if quality >= Quality::Medium {
                    style.highlight
                } else {
                    0.0
                },
                style.roughness,
                style.opacity,
            ),
            motion: Vec4::new(
                style.ripple_speed,
                style.deformation,
                style.bands,
                quality.level() as f32,
            ),
            state: Vec4::new(0.0, -1.0, 0.0, 0.0),
        }
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct LiquidExt {
    #[uniform(100)]
    pub params: LiquidParams,
}

impl MaterialExtension for LiquidExt {
    fn fragment_shader() -> ShaderRef {
        match SHADER.get() {
            Some(h) => ShaderRef::Handle(h.clone()),
            None => ShaderRef::Default,
        }
    }
}

/// What a material is used for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Use {
    Body,
    /// First-person arms (own render layer and light).
    Viewmodel,
    /// Droplets and splashes.
    Drop,
    /// Death puddle (translucent).
    Puddle,
}

/// Current look, editable live (demo panel); every material follows it.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct CharacterStyle(pub LiquidStyle);

#[derive(Resource, Default)]
pub struct LiquidMaterials {
    shared: HashMap<(PaletteId, Quality, Use), Handle<LiquidMaterial>>,
    /// Private copies handed to dying / flashing characters.
    free: Vec<Handle<LiquidMaterial>>,
    lent: usize,
}

fn base_for(u: Use, style: &LiquidStyle) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: style.roughness,
        alpha_mode: if u == Use::Puddle {
            AlphaMode::Blend
        } else {
            AlphaMode::Opaque
        },
        ..default()
    }
}

fn params_for(id: PaletteId, q: Quality, u: Use, style: &LiquidStyle) -> LiquidParams {
    let mut p = LiquidParams::new(&id.palette(), style, q);
    match u {
        Use::Body | Use::Viewmodel => {}
        Use::Drop => p.motion.y = 0.0,
        // The body stays opaque (readability); opacity applies to puddles.
        Use::Puddle => p.look.w = 0.82 * style.opacity,
    }
    p
}

impl LiquidMaterials {
    pub fn get(
        &mut self,
        assets: &mut Assets<LiquidMaterial>,
        style: &LiquidStyle,
        id: PaletteId,
        q: Quality,
        u: Use,
    ) -> Handle<LiquidMaterial> {
        self.shared
            .entry((id, q, u))
            .or_insert_with(|| {
                assets.add(LiquidMaterial {
                    base: base_for(u, style),
                    extension: LiquidExt {
                        params: params_for(id, q, u, style),
                    },
                })
            })
            .clone()
    }

    /// Lends a private body material (state can be animated per entity).
    pub fn lend(
        &mut self,
        assets: &mut Assets<LiquidMaterial>,
        style: &LiquidStyle,
        id: PaletteId,
        q: Quality,
    ) -> Handle<LiquidMaterial> {
        self.lent += 1;
        let params = params_for(id, q, Use::Body, style);
        if let Some(h) = self.free.pop()
            && let Some(mut m) = assets.get_mut(&h)
        {
            m.extension.params = params;
            m.base = base_for(Use::Body, style);
            return h;
        }
        assets.add(LiquidMaterial {
            base: base_for(Use::Body, style),
            extension: LiquidExt { params },
        })
    }

    pub fn give_back(&mut self, h: Handle<LiquidMaterial>) {
        self.lent = self.lent.saturating_sub(1);
        if self.free.len() < 16 {
            self.free.push(h);
        }
    }

    pub fn shared_count(&self) -> usize {
        self.shared.len()
    }

    pub fn lent_count(&self) -> usize {
        self.lent
    }

    /// Re-applies the style to every shared material (live tuning).
    fn restyle(&self, assets: &mut Assets<LiquidMaterial>, style: &LiquidStyle) {
        for (&(id, q, u), h) in &self.shared {
            if let Some(mut m) = assets.get_mut(h) {
                m.extension.params = params_for(id, q, u, style);
                m.base.perceptual_roughness = style.roughness;
            }
        }
    }
}

fn restyle_on_change(
    style: Res<CharacterStyle>,
    mats: Res<LiquidMaterials>,
    mut assets: ResMut<Assets<LiquidMaterial>>,
) {
    if style.is_changed() && !style.is_added() {
        mats.restyle(&mut assets, &style.0);
    }
}

pub fn plugin(app: &mut App) {
    embedded_asset!(app, "liquid.wesl");
    let handle: Handle<Shader> = load_embedded_asset!(app, "liquid.wesl");
    let _ = SHADER.set(handle);
    app.add_plugins(MaterialPlugin::<LiquidMaterial>::default())
        .init_resource::<CharacterStyle>()
        .init_resource::<LiquidMaterials>()
        .add_systems(PostUpdate, restyle_on_change);
}
