//! Liquid droplets and death puddles: purely cosmetic, client-side, pooled.
//!
//! Droplets are never targets nor obstacles (no collider, no hitbox, no
//! shadow) and nothing is sent over the network. Entities are recycled from
//! a pool whose size is capped by the quality level.

use super::liquid::{CharacterStyle, LiquidMaterial, LiquidMaterials, Use};
use super::mesh;
use super::style::{PaletteId, Quality};
use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Mesh, PrimitiveTopology};
use bevy::prelude::*;

/// Gravity for droplets (m/s²).
const GRAVITY: f32 = 9.8;
/// Droplets farther than this from the camera are not emitted (m).
pub const EMIT_RANGE: f32 = 40.0;

pub fn liquid_mesh(m: &mesh::MeshData, skinned: bool) -> Mesh {
    let mut out = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, m.positions.clone())
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, m.normals.clone())
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, m.uvs.clone())
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, m.colors.clone());
    if skinned {
        out.insert_attribute(
            Mesh::ATTRIBUTE_JOINT_INDEX,
            bevy::mesh::VertexAttributeValues::Uint16x4(m.joints.clone()),
        );
        out.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, m.weights.clone());
    }
    out
}

struct Drop {
    entity: Entity,
    pos: Vec3,
    vel: Vec3,
    size: f32,
    life: f32,
    max_life: f32,
    ground: f32,
    landed: bool,
    active: bool,
}

struct Puddle {
    entity: Entity,
    age: f32,
    size: f32,
    active: bool,
}

/// A droplet request (world space, metres).
#[derive(Clone, Copy, Debug)]
pub struct Emit {
    pub pos: Vec3,
    pub vel: Vec3,
    pub size: f32,
    pub life: f32,
    /// Height of the ground below (m): drops splat there.
    pub ground: f32,
    pub palette: PaletteId,
}

#[derive(Resource, Default)]
pub struct LiquidFx {
    drops: Vec<Drop>,
    puddles: Vec<Puddle>,
    queue: Vec<Emit>,
    puddle_queue: Vec<(Vec3, f32, PaletteId)>,
    drop_mesh: Option<Handle<Mesh>>,
    puddle_mesh: Option<Handle<Mesh>>,
    /// Emissions dropped because the pool was full (debug panel).
    pub dropped: u64,
    pub camera: Vec3,
    pub quality: Option<Quality>,
}

impl LiquidFx {
    pub fn emit(&mut self, e: Emit) {
        if self.camera.distance(e.pos) < EMIT_RANGE && self.queue.len() < 512 {
            self.queue.push(e);
        }
    }

    /// Temporary puddle (`size` = radius in metres).
    pub fn puddle(&mut self, pos: Vec3, size: f32, palette: PaletteId) {
        self.puddle_queue.push((pos, size, palette));
    }

    pub fn active_drops(&self) -> usize {
        self.drops.iter().filter(|d| d.active).count()
    }

    pub fn pool_size(&self) -> usize {
        self.drops.len()
    }

    pub fn active_puddles(&self) -> usize {
        self.puddles.iter().filter(|p| p.active).count()
    }
}

/// Seconds a puddle stays before drying out.
pub const PUDDLE_LIFE: f32 = 5.0;
const MAX_PUDDLES: usize = 12;

pub fn update_fx(
    mut commands: Commands,
    mut fx: ResMut<LiquidFx>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<LiquidMaterials>,
    mut assets: ResMut<Assets<LiquidMaterial>>,
    style: Res<CharacterStyle>,
    time: Res<Time>,
    mut q: Query<(
        &mut Transform,
        &mut Visibility,
        &mut MeshMaterial3d<LiquidMaterial>,
    )>,
) {
    let dt = time.delta_secs().min(0.05);
    let quality = fx.quality.unwrap_or(Quality::High);
    let cap = quality.max_droplets();
    let fx = &mut *fx;
    if fx.drop_mesh.is_none() {
        fx.drop_mesh = Some(meshes.add(liquid_mesh(&mesh::droplet(), false)));
        fx.puddle_mesh = Some(meshes.add(liquid_mesh(&mesh::puddle(), false)));
    }
    let drop_mesh = fx.drop_mesh.clone().unwrap_or_default();
    let puddle_mesh = fx.puddle_mesh.clone().unwrap_or_default();
    // Shrink the pool when the quality goes down.
    while fx.drops.len() > cap {
        if let Some(d) = fx.drops.pop() {
            commands.entity(d.entity).despawn();
        }
    }
    // New droplets.
    for e in std::mem::take(&mut fx.queue) {
        let slot = match fx.drops.iter().position(|d| !d.active) {
            Some(i) => Some(i),
            None if fx.drops.len() < cap => {
                let entity = commands
                    .spawn((
                        Mesh3d(drop_mesh.clone()),
                        MeshMaterial3d(mats.get(
                            &mut assets,
                            &style.0,
                            e.palette,
                            quality,
                            Use::Drop,
                        )),
                        Transform::from_translation(e.pos).with_scale(Vec3::splat(e.size)),
                        Visibility::Visible,
                        NotShadowCaster,
                    ))
                    .id();
                fx.drops.push(Drop {
                    entity,
                    pos: e.pos,
                    vel: e.vel,
                    size: e.size,
                    life: 0.0,
                    max_life: e.life,
                    ground: e.ground,
                    landed: false,
                    active: false,
                });
                Some(fx.drops.len() - 1)
            }
            None => None,
        };
        let Some(i) = slot else {
            fx.dropped += 1;
            continue;
        };
        let d = &mut fx.drops[i];
        *d = Drop {
            entity: d.entity,
            pos: e.pos,
            vel: e.vel,
            size: e.size,
            life: 0.0,
            max_life: e.life,
            ground: e.ground,
            landed: false,
            active: true,
        };
        if let Ok((_, mut v, mut m)) = q.get_mut(d.entity) {
            *v = Visibility::Visible;
            m.0 = mats.get(&mut assets, &style.0, e.palette, quality, Use::Drop);
        }
    }
    // Simulation.
    for d in fx.drops.iter_mut().filter(|d| d.active) {
        d.life += dt;
        if !d.landed {
            d.vel.y -= GRAVITY * dt;
            d.pos += d.vel * dt;
            if d.pos.y <= d.ground + d.size * 0.3 {
                d.pos.y = d.ground + 0.002;
                d.landed = true;
                d.life = d.life.max(d.max_life - 0.6);
            }
        }
        let fade = 1.0 - ((d.life - (d.max_life - 0.6)) / 0.6).clamp(0.0, 1.0);
        let scale = if d.landed {
            Vec3::new(1.8, 0.25, 1.8) * d.size * fade
        } else {
            // Stretched along its fall.
            let s = (1.0 + d.vel.length() * 0.05).min(1.8);
            Vec3::new(d.size / s.sqrt(), d.size * s, d.size / s.sqrt())
        };
        let done = d.life >= d.max_life;
        if let Ok((mut t, mut v, _)) = q.get_mut(d.entity) {
            t.translation = d.pos;
            t.scale = scale.max(Vec3::splat(1e-4));
            if done {
                *v = Visibility::Hidden;
            }
        }
        if done {
            d.active = false;
        }
    }
    // Puddles.
    for (pos, size, palette) in std::mem::take(&mut fx.puddle_queue) {
        let mat = mats.get(&mut assets, &style.0, palette, quality, Use::Puddle);
        let slot = fx.puddles.iter().position(|p| !p.active).or_else(|| {
            if fx.puddles.len() < MAX_PUDDLES {
                let entity = commands
                    .spawn((
                        Mesh3d(puddle_mesh.clone()),
                        MeshMaterial3d(mat.clone()),
                        Transform::from_translation(pos).with_scale(Vec3::splat(0.001)),
                        Visibility::Visible,
                        NotShadowCaster,
                    ))
                    .id();
                fx.puddles.push(Puddle {
                    entity,
                    age: 0.0,
                    size,
                    active: false,
                });
                Some(fx.puddles.len() - 1)
            } else {
                // Recycle the oldest.
                fx.puddles
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.age.total_cmp(&b.1.age))
                    .map(|(i, _)| i)
            }
        });
        if let Some(i) = slot {
            let p = &mut fx.puddles[i];
            p.age = 0.0;
            p.size = size;
            p.active = true;
            if let Ok((mut t, mut v, mut m)) = q.get_mut(p.entity) {
                t.translation = pos + Vec3::Y * 0.004;
                t.rotation = Quat::from_rotation_y(pos.x * 7.3 + pos.z * 3.1);
                *v = Visibility::Visible;
                m.0 = mat;
            }
        }
    }
    for p in fx.puddles.iter_mut().filter(|p| p.active) {
        p.age += dt;
        let grow = (p.age / 0.8).clamp(0.0, 1.0);
        let grow = 1.0 - (1.0 - grow) * (1.0 - grow);
        let dry = 1.0 - ((p.age - (PUDDLE_LIFE - 1.5)) / 1.5).clamp(0.0, 1.0);
        let r = p.size * grow * (0.55 + 0.45 * dry);
        if let Ok((mut t, mut v, _)) = q.get_mut(p.entity) {
            t.scale = Vec3::new(r, 1.0, r).max(Vec3::splat(1e-4));
            if p.age >= PUDDLE_LIFE {
                *v = Visibility::Hidden;
            }
        }
        if p.age >= PUDDLE_LIFE {
            p.active = false;
        }
    }
}

/// Deterministic pseudo-random sequence for cosmetic effects.
#[derive(Clone, Copy, Debug)]
pub struct Rng(pub u32);

impl Rng {
    pub fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        mesh::hash01(self.0)
    }

    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.next()
    }

    /// Random unit vector, biased toward `dir` by `bias` (0..1).
    pub fn spray(&mut self, dir: Vec3, bias: f32) -> Vec3 {
        let r = Vec3::new(
            self.range(-1.0, 1.0),
            self.range(-1.0, 1.0),
            self.range(-1.0, 1.0),
        );
        (r.normalize_or(Vec3::Y) * (1.0 - bias) + dir * bias).normalize_or(Vec3::Y)
    }
}
