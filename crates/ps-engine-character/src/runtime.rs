//! ECS side of the characters: game state comes in through
//! [`CharacterFeed`] (filled by `game.rs`), skinned liquid bodies, solid
//! weapons and local effects come out.
//!
//! The feed is read-only presentation data: nothing here changes the
//! networked state, the hitboxes or the movement.

use super::anim::{Action, AnimInput, Animator, Hold};
use super::fx::{Emit, LiquidFx, Rng, liquid_mesh};
use super::liquid::{CharacterStyle, LiquidMaterial, LiquidMaterials, Use};
use super::mesh::{self, Lod, Parts};
use super::rig::{self, JOINT_COUNT, Joint};
use super::style::{PaletteId, Quality};
use super::weapon_mesh;
use crate::world::SCALE;
use bevy::camera::visibility::{DynamicSkinnedMeshBounds, RenderLayers, VisibilityRange};
use bevy::light::NotShadowCaster;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::prelude::*;
use engine_core::Team;
use std::collections::HashMap;

/// Network state of one visible player, as the presentation needs it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharState {
    pub id: u8,
    /// Feet, game units.
    pub origin: Vec3,
    /// World velocity, u/s.
    pub velocity: Vec3,
    /// Degrees.
    pub yaw: f32,
    pub pitch: f32,
    pub duck: f32,
    pub on_ground: bool,
    pub alive: bool,
    /// False when the server stopped sending this player (stale).
    pub visible: bool,
    pub working: bool,
    pub reloading: bool,
    pub team: Team,
    pub hold: Hold,
    /// Changes when the player switches weapon.
    pub weapon_key: u16,
    pub reload_time: f32,
    pub deploy_time: f32,
    /// Forced palette (demo scene only; the game always passes `None`).
    pub palette: Option<PaletteId>,
}

/// The local player, for the first-person arms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FirstPerson {
    pub id: u8,
    pub alive: bool,
    pub team: Team,
    pub hold: Hold,
    pub weapon_key: u16,
    /// Seconds of reload left (None = not reloading).
    pub reload_left: Option<f32>,
    /// Seconds of deploy left.
    pub deploy_left: f32,
    /// View-local velocity, u/s (-z forward).
    pub velocity: Vec3,
    pub on_ground: bool,
}

/// Cosmetic triggers taken from game events.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CharEvent {
    Fire,
    /// `from`: attacker position (game units); `head`: head hit.
    Hit {
        from: Vec3,
        head: bool,
    },
    Footstep,
    Land,
}

#[derive(Resource, Debug)]
pub struct CharacterFeed {
    pub players: Vec<CharState>,
    pub me: Option<FirstPerson>,
    pub viewer: Team,
    pub free_for_all: bool,
    pub events: Vec<(u8, CharEvent)>,
    /// Camera position (m), for effect culling.
    pub eye: Vec3,
}

impl Default for CharacterFeed {
    fn default() -> Self {
        CharacterFeed {
            players: Vec::new(),
            me: None,
            viewer: Team::Spectator,
            free_for_all: false,
            events: Vec::new(),
            eye: Vec3::ZERO,
        }
    }
}

/// Shared meshes (built once, instanced by every character).
#[derive(Resource)]
pub struct CharacterAssets {
    body: [Handle<Mesh>; 3],
    arms: Handle<Mesh>,
    inverse_bindposes: Handle<SkinnedMeshInverseBindposes>,
    weapons: HashMap<Hold, Handle<Mesh>>,
    weapon_mat: Handle<StandardMaterial>,
    /// Triangle counts per LOD (debug panel).
    pub triangles: [usize; 3],
}

fn skinned(m: &mesh::MeshData) -> Mesh {
    let out = liquid_mesh(m, true);
    match out.clone().with_generated_skinned_mesh_bounds() {
        Ok(m) => m,
        Err(e) => {
            warn!("characters: no skinned bounds ({e}); culling falls back to the bind pose");
            out
        }
    }
}

pub fn setup_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let lods = [Lod::High, Lod::Mid, Lod::Far].map(|l| mesh::humanoid(l, Parts::FullBody));
    let triangles = [0, 1, 2].map(|i| lods[i].triangle_count());
    let body = [0, 1, 2].map(|i| meshes.add(skinned(&lods[i])));
    let arms = meshes.add(skinned(&mesh::humanoid(Lod::High, Parts::ArmsOnly)));
    let inv: Vec<Mat4> = rig::BIND
        .iter()
        .map(|b| Mat4::from_translation(-*b))
        .collect();
    let weapons = Hold::ALL
        .into_iter()
        .filter_map(|h| weapon_mesh::weapon(h).map(|s| (h, meshes.add(s.into_mesh()))))
        .collect();
    commands.insert_resource(CharacterAssets {
        body,
        arms,
        inverse_bindposes: bindposes.add(SkinnedMeshInverseBindposes::from(inv)),
        weapons,
        weapon_mat: mats.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.55,
            metallic: 0.35,
            reflectance: 0.3,
            ..default()
        }),
        triangles,
    });
}

#[derive(Component)]
pub struct BodyPart;
#[derive(Component)]
pub struct WeaponPart;
/// Root of the first-person arms (child of `game::Viewmodel`).
#[derive(Component)]
pub struct ViewRig;

/// Body melts between these times after death (s).
const MELT_START: f32 = 0.3;
const MELT_END: f32 = 1.8;
const RESPAWN_TIME: f32 = 0.7;

#[derive(Component)]
pub struct Character {
    pub id: u8,
    pub anim: Animator,
    pub palette: PaletteId,
    pub hold: Hold,
    joints: [Entity; JOINT_COUNT],
    bodies: Vec<Entity>,
    weapon: Entity,
    first_person: bool,
    weapon_key: u16,
    reloading: bool,
    /// Has been seen alive since it was spawned (no melt for a body that
    /// was already dead when it came into view).
    seen_alive: bool,
    was_alive: bool,
    respawn: Option<f32>,
    flash: f32,
    lent: Option<Handle<LiquidMaterial>>,
    shared: Handle<LiquidMaterial>,
    rng: Rng,
    puddled: bool,
    burst: bool,
    drip: f32,
}

impl Character {
    fn needs_private_material(&self) -> bool {
        let dying = !self.was_alive && self.seen_alive && self.anim.death_time() < MELT_END + 0.1;
        dying || self.respawn.is_some() || self.flash > 0.01
    }
}

/// Character entities by player id.
#[derive(Resource, Default)]
pub struct Characters {
    map: HashMap<u8, Entity>,
    view: Option<Entity>,
    quality: Option<Quality>,
}

impl Characters {
    pub fn count(&self) -> usize {
        self.map.len()
    }
}

struct Spawned {
    root: Entity,
    joints: [Entity; JOINT_COUNT],
    bodies: Vec<Entity>,
    weapon: Entity,
}

fn spawn_rig(
    commands: &mut Commands,
    a: &CharacterAssets,
    mat: &Handle<LiquidMaterial>,
    quality: Quality,
    first_person: bool,
) -> Spawned {
    let root = commands
        .spawn((
            Transform::from_scale(Vec3::splat(SCALE)),
            Visibility::Hidden,
        ))
        .id();
    let joints = rig::ALL.map(|j| {
        commands
            .spawn((Transform::from_translation(rig::bind(j)), ChildOf(root)))
            .id()
    });
    let layer = RenderLayers::layer(crate::game::VIEWMODEL_LAYER);
    let skin = SkinnedMesh {
        inverse_bindposes: a.inverse_bindposes.clone(),
        joints: joints.to_vec(),
    };
    let mut bodies = Vec::new();
    if first_person {
        bodies.push(
            commands
                .spawn((
                    BodyPart,
                    Mesh3d(a.arms.clone()),
                    MeshMaterial3d(mat.clone()),
                    skin.clone(),
                    DynamicSkinnedMeshBounds,
                    Transform::IDENTITY,
                    Visibility::Inherited,
                    NotShadowCaster,
                    layer.clone(),
                    ChildOf(root),
                ))
                .id(),
        );
    } else {
        // Abrupt LOD switches: the custom shader does not implement the
        // crossfade dither, and the silhouette is identical anyway.
        let (d0, d1) = quality.lod_distances();
        let ranges = [(0.0, d0), (d0, d1), (d1, 1.0e7)];
        for (i, (start, end)) in ranges.into_iter().enumerate() {
            bodies.push(
                commands
                    .spawn((
                        BodyPart,
                        Mesh3d(a.body[i].clone()),
                        MeshMaterial3d(mat.clone()),
                        skin.clone(),
                        DynamicSkinnedMeshBounds,
                        VisibilityRange::abrupt(start, end),
                        Transform::IDENTITY,
                        Visibility::Inherited,
                        ChildOf(root),
                    ))
                    .id(),
            );
        }
    }
    let mut w = commands.spawn((
        WeaponPart,
        Mesh3d(a.weapons.get(&Hold::Rifle).cloned().unwrap_or_default()),
        MeshMaterial3d(a.weapon_mat.clone()),
        Transform::IDENTITY,
        Visibility::Hidden,
        ChildOf(root),
    ));
    if first_person {
        w.insert((NotShadowCaster, layer));
    }
    let weapon = w.id();
    Spawned {
        root,
        joints,
        bodies,
        weapon,
    }
}

fn new_character(
    id: u8,
    s: &Spawned,
    palette: PaletteId,
    quality: Quality,
    shared: Handle<LiquidMaterial>,
    fp: bool,
) -> Character {
    let mut anim = Animator::default();
    anim.secondary = quality == Quality::Ultra;
    Character {
        id,
        anim,
        palette,
        hold: Hold::Empty,
        joints: s.joints,
        bodies: s.bodies.clone(),
        weapon: s.weapon,
        first_person: fp,
        weapon_key: u16::MAX,
        reloading: false,
        seen_alive: false,
        was_alive: false,
        respawn: None,
        flash: 0.0,
        lent: None,
        shared,
        rng: Rng(0x9E37_79B9 ^ (id as u32) << 8),
        puddled: false,
        burst: false,
        drip: 0.0,
    }
}

fn despawn_character(
    commands: &mut Commands,
    mats: &mut LiquidMaterials,
    e: Entity,
    ch: Option<&mut Character>,
) {
    if let Some(ch) = ch
        && let Some(h) = ch.lent.take()
    {
        mats.give_back(h);
    }
    commands.entity(e).despawn();
}

/// Droplet spray budget for an effect of nominal size `n` at quality `q`.
fn budget(n: f32, q: Quality) -> usize {
    let k = match q {
        Quality::Low => 0.0,
        Quality::Medium => 0.4,
        Quality::High => 1.0,
        Quality::Ultra => 1.5,
    };
    (n * k).round() as usize
}

/// Weapon rest position for each hold in model space (first person anchor).
fn rest_anchor(h: Hold, cache: &mut HashMap<Hold, Vec3>) -> Vec3 {
    *cache.entry(h).or_insert_with(|| {
        let mut a = Animator::default();
        let inp = AnimInput {
            hold: h,
            ..Default::default()
        };
        for _ in 0..90 {
            a.update(&inp, 1.0 / 60.0);
        }
        a.weapon.0
    })
}

/// Where the first-person weapon grip sits relative to `game::Viewmodel` (m).
/// The anchor itself sits 16 cm ahead of the eye (old cube viewmodel); the
/// grip goes further, lower and right so the whole weapon is framed.
const VIEW_GRIP: Vec3 = Vec3::new(0.09, -0.1, -0.14);

#[allow(clippy::too_many_arguments)]
pub fn drive(
    mut commands: Commands,
    mut feed: ResMut<CharacterFeed>,
    mut chars: ResMut<Characters>,
    mut fx: ResMut<LiquidFx>,
    mut mats: ResMut<LiquidMaterials>,
    mut assets: ResMut<Assets<LiquidMaterial>>,
    style: Res<CharacterStyle>,
    settings: Res<crate::settings::Settings>,
    char_assets: Option<Res<CharacterAssets>>,
    time: Res<Time>,
    mut anchors: Local<HashMap<Hold, Vec3>>,
    viewmodel: Query<Entity, With<crate::game::Viewmodel>>,
    mut roots: Query<(&mut Character, &mut Transform, &mut Visibility)>,
    mut xf: Query<&mut Transform, Without<Character>>,
    mut bodies: Query<
        (&mut Visibility, &mut MeshMaterial3d<LiquidMaterial>),
        (With<BodyPart>, Without<Character>, Without<WeaponPart>),
    >,
    mut weapons: Query<
        (&mut Visibility, &mut Mesh3d),
        (With<WeaponPart>, Without<Character>, Without<BodyPart>),
    >,
) {
    let Some(a) = char_assets else { return };
    let dt = time.delta_secs().min(0.1);
    let cs = &settings.characters;
    let quality = cs.quality;
    fx.quality = Some(quality);
    fx.camera = feed.eye;
    let feed = &mut *feed;
    let events = std::mem::take(&mut feed.events);

    // Quality change: rebuild every character (LOD ranges, secondary motion).
    if chars.quality != Some(quality) {
        chars.quality = Some(quality);
        let view = chars.view.take();
        let all: Vec<Entity> = chars.map.drain().map(|(_, e)| e).chain(view).collect();
        for e in all {
            let ch = roots.get_mut(e).ok().map(|(c, _, _)| c);
            despawn_character(&mut commands, &mut mats, e, ch.map(|c| c.into_inner()));
        }
        return;
    }

    // Spawn / despawn third-person characters.
    let ids: Vec<u8> = feed.players.iter().map(|p| p.id).collect();
    let gone: Vec<u8> = chars
        .map
        .keys()
        .copied()
        .filter(|id| !ids.contains(id))
        .collect();
    for id in gone {
        if let Some(e) = chars.map.remove(&id) {
            let ch = roots.get_mut(e).ok().map(|(c, _, _)| c);
            despawn_character(&mut commands, &mut mats, e, ch.map(|c| c.into_inner()));
        }
    }
    for p in &feed.players {
        if let std::collections::hash_map::Entry::Vacant(slot) = chars.map.entry(p.id) {
            let pal = p
                .palette
                .unwrap_or_else(|| cs.palette_in(p.team, feed.viewer, feed.free_for_all));
            let shared = mats.get(&mut assets, &style.0, pal, quality, Use::Body);
            let s = spawn_rig(&mut commands, &a, &shared, quality, false);
            commands
                .entity(s.root)
                .insert(new_character(p.id, &s, pal, quality, shared, false));
            slot.insert(s.root);
        }
    }
    // First-person arms.
    match (feed.me, chars.view, viewmodel.single()) {
        (Some(me), None, Ok(vm)) => {
            let pal = cs.palette_in(me.team, me.team, false);
            let shared = mats.get(&mut assets, &style.0, pal, quality, Use::Viewmodel);
            let s = spawn_rig(&mut commands, &a, &shared, quality, true);
            commands.entity(s.root).insert((
                new_character(me.id, &s, pal, quality, shared, true),
                ViewRig,
                ChildOf(vm),
                RenderLayers::layer(crate::game::VIEWMODEL_LAYER),
            ));
            chars.view = Some(s.root);
        }
        (None, Some(e), _) => {
            chars.view = None;
            let ch = roots.get_mut(e).ok().map(|(c, _, _)| c);
            despawn_character(&mut commands, &mut mats, e, ch.map(|c| c.into_inner()));
        }
        _ => {}
    }

    let by_id: HashMap<u8, &CharState> = feed.players.iter().map(|p| (p.id, p)).collect();
    for (mut ch, mut root_t, mut root_v) in &mut roots {
        let ch = &mut *ch;
        let fp = ch.first_person;
        // Gather this frame's input.
        let (
            inp,
            team,
            origin_m,
            yaw,
            visible,
            weapon_key,
            reloading,
            reload_time,
            deploy_time,
            forced,
        ) = if fp {
            let Some(me) = feed.me else { continue };
            let reload_time = me.reload_left.unwrap_or(0.0);
            (
                AnimInput {
                    velocity: me.velocity * 0.4,
                    on_ground: me.on_ground,
                    duck: 0.0,
                    pitch: 0.0,
                    hold: me.hold,
                    working: false,
                    alive: me.alive,
                },
                me.team,
                Vec3::ZERO,
                0.0,
                true,
                me.weapon_key,
                me.reload_left.is_some(),
                reload_time,
                me.deploy_left,
                None,
            )
        } else {
            let Some(p) = by_id.get(&ch.id) else { continue };
            let local_vel = Quat::from_rotation_y(-p.yaw.to_radians()) * p.velocity;
            (
                AnimInput {
                    velocity: local_vel,
                    on_ground: p.on_ground,
                    duck: p.duck,
                    pitch: p.pitch,
                    hold: p.hold,
                    working: p.working,
                    alive: p.alive,
                },
                p.team,
                p.origin * SCALE,
                p.yaw,
                p.visible,
                p.weapon_key,
                p.reloading,
                p.reload_time,
                p.deploy_time,
                p.palette,
            )
        };
        let alive = inp.alive;

        // Palette follows settings / team changes.
        let want = if fp {
            cs.palette_in(team, team, false)
        } else {
            forced.unwrap_or_else(|| cs.palette_in(team, feed.viewer, feed.free_for_all))
        };
        if want != ch.palette {
            ch.palette = want;
            ch.shared = mats.get(
                &mut assets,
                &style.0,
                want,
                quality,
                if fp { Use::Viewmodel } else { Use::Body },
            );
            if let Some(h) = ch.lent.take() {
                mats.give_back(h);
            }
        }

        // State transitions.
        if alive && !ch.was_alive {
            if ch.seen_alive {
                ch.respawn = Some(0.0);
                if !fp {
                    let n = budget(8.0, quality);
                    for i in 0..n {
                        let ang = i as f32 / n.max(1) as f32 * std::f32::consts::TAU;
                        fx.emit(Emit {
                            pos: origin_m + Vec3::new(ang.cos() * 0.35, 0.05, ang.sin() * 0.35),
                            vel: Vec3::new(-ang.cos() * 0.6, 2.2, -ang.sin() * 0.6),
                            size: 0.03,
                            life: 0.5,
                            ground: origin_m.y,
                            palette: ch.palette,
                        });
                    }
                }
            }
            ch.seen_alive = true;
            ch.puddled = false;
            ch.burst = false;
        }
        ch.was_alive = alive;
        if alive {
            if weapon_key != ch.weapon_key {
                if ch.weapon_key != u16::MAX {
                    ch.anim.trigger(Action::Switch {
                        duration: deploy_time.max(0.2),
                    });
                }
                ch.weapon_key = weapon_key;
            }
            if reloading && !ch.reloading {
                ch.anim.trigger(Action::Reload {
                    duration: reload_time.max(0.3),
                });
            }
        }
        ch.reloading = reloading;
        ch.hold = inp.hold;

        // Events.
        for (_, ev) in events.iter().filter(|(id, _)| *id == ch.id) {
            match *ev {
                CharEvent::Fire => ch.anim.trigger(match ch.hold {
                    Hold::Knife => Action::Slash,
                    Hold::Grenade => Action::Throw,
                    _ => Action::Fire,
                }),
                CharEvent::Hit { from, head } if !fp => {
                    let at = if head { Joint::Head } else { Joint::Chest };
                    let hit_m = root_t.transform_point(ch.anim.pose.get(at).pos);
                    let away = (hit_m - from * SCALE).normalize_or(Vec3::Y);
                    let push = Quat::from_rotation_y(-yaw.to_radians()) * away;
                    ch.anim.trigger(Action::Hit { push });
                    ch.flash = 1.0;
                    for _ in 0..budget(style.0.splash, quality) {
                        let dir = ch.rng.spray(away, 0.55);
                        let speed = ch.rng.range(1.2, 3.2);
                        fx.emit(Emit {
                            pos: hit_m,
                            vel: dir * speed + Vec3::Y * 0.8,
                            size: ch.rng.range(0.018, 0.04),
                            life: 1.6,
                            ground: origin_m.y,
                            palette: ch.palette,
                        });
                    }
                }
                CharEvent::Footstep if !fp && quality >= Quality::High => {
                    let l = ch.anim.pose.get(Joint::AnkleL).pos;
                    let r = ch.anim.pose.get(Joint::AnkleR).pos;
                    let foot = root_t.transform_point(if l.y < r.y { l } else { r });
                    for _ in 0..2 {
                        let v = Vec3::new(
                            ch.rng.range(-0.5, 0.5),
                            ch.rng.range(0.6, 1.2),
                            ch.rng.range(-0.5, 0.5),
                        );
                        fx.emit(Emit {
                            pos: foot + Vec3::Y * 0.03,
                            vel: v,
                            size: 0.016,
                            life: 0.9,
                            ground: origin_m.y,
                            palette: ch.palette,
                        });
                    }
                }
                CharEvent::Land if !fp => {
                    ch.anim.trigger(Action::Land { strength: 0.8 });
                    for _ in 0..budget(6.0, quality) {
                        let a = ch.rng.range(0.0, std::f32::consts::TAU);
                        fx.emit(Emit {
                            pos: origin_m + Vec3::Y * 0.04,
                            vel: Vec3::new(a.cos() * 1.4, ch.rng.range(0.8, 1.6), a.sin() * 1.4),
                            size: 0.02,
                            life: 1.0,
                            ground: origin_m.y,
                            palette: ch.palette,
                        });
                    }
                }
                _ => {}
            }
        }

        // Animation.
        ch.anim.secondary = quality == Quality::Ultra;
        ch.anim.update(&inp, dt);
        ch.flash = (ch.flash - dt * 7.0).max(0.0);
        if let Some(t) = &mut ch.respawn {
            *t += dt;
            if *t > RESPAWN_TIME {
                ch.respawn = None;
            }
        }

        // Root transform.
        if fp {
            let anchor = rest_anchor(inp.hold, &mut anchors);
            root_t.translation = VIEW_GRIP - anchor * SCALE;
            root_t.rotation = Quat::IDENTITY;
        } else {
            root_t.translation = origin_m;
            root_t.rotation = Quat::from_rotation_y(yaw.to_radians());
        }
        root_t.scale = Vec3::splat(SCALE);
        let dead_for = ch.anim.death_time();
        let melted = !alive && (!ch.seen_alive || dead_for > MELT_END);
        let show = visible && !melted && !(fp && !alive);
        let vis = if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *root_v != vis {
            *root_v = vis;
        }
        if !show {
            if let Some(h) = ch.lent.take() {
                mats.give_back(h);
                for &b in &ch.bodies {
                    if let Ok((_, mut m)) = bodies.get_mut(b) {
                        m.0 = ch.shared.clone();
                    }
                }
            }
            continue;
        }

        // Joints and weapon.
        for (j, &je) in ch.joints.iter().enumerate() {
            if let Ok(mut t) = xf.get_mut(je) {
                let p = &ch.anim.pose.0[j];
                t.translation = p.pos;
                t.rotation = p.rot;
                t.scale = p.scale;
            }
        }
        if let Ok(mut t) = xf.get_mut(ch.weapon) {
            t.translation = ch.anim.weapon.0;
            t.rotation = ch.anim.weapon.1;
        }
        if let Ok((mut v, mut m)) = weapons.get_mut(ch.weapon) {
            let mesh = a.weapons.get(&ch.hold);
            let wv = match mesh {
                Some(h) if alive => {
                    if m.0 != *h {
                        m.0 = h.clone();
                    }
                    Visibility::Inherited
                }
                _ => Visibility::Hidden,
            };
            if *v != wv {
                *v = wv;
            }
        }

        // Death effects.
        if !alive && ch.seen_alive && !fp {
            let chest = root_t.transform_point(ch.anim.pose.get(Joint::Chest).pos);
            if !ch.burst && dead_for > 0.05 {
                ch.burst = true;
                for _ in 0..budget(style.0.splash * 1.2, quality) {
                    let dir = ch.rng.spray(Vec3::Y, 0.3);
                    fx.emit(Emit {
                        pos: chest,
                        vel: dir * ch.rng.range(0.8, 2.4),
                        size: ch.rng.range(0.02, 0.05),
                        life: 1.8,
                        ground: origin_m.y,
                        palette: ch.palette,
                    });
                }
            }
            if !ch.puddled && dead_for > 0.45 {
                ch.puddled = true;
                fx.puddle(origin_m, 0.6, ch.palette);
            }
        }
        // Idle drips (alive, from the hands and chin).
        if alive && !fp && quality >= Quality::Medium {
            ch.drip += dt * style.0.drip_rate;
            if ch.drip >= 1.0 {
                ch.drip -= 1.0 + ch.rng.range(0.0, 0.5);
                let from = [Joint::FingersL, Joint::FingersR, Joint::Head]
                    [(ch.rng.next() * 2.99) as usize];
                let p = root_t.transform_point(ch.anim.pose.get(from).pos);
                fx.emit(Emit {
                    pos: p - Vec3::Y * 0.04,
                    vel: Vec3::new(0.0, -0.2, 0.0),
                    size: 0.014,
                    life: 1.4,
                    ground: origin_m.y,
                    palette: ch.palette,
                });
            }
        }

        // Material: shared, or a private copy while an effect is running.
        if ch.needs_private_material() {
            if ch.lent.is_none() {
                let h = mats.lend(&mut assets, &style.0, ch.palette, quality);
                if fp && let Some(mut m) = assets.get_mut(&h) {
                    m.extension.params.motion.y *= 0.5;
                }
                ch.lent = Some(h);
            }
            if let Some(h) = &ch.lent
                && let Some(mut m) = assets.get_mut(h)
            {
                let melt = if alive {
                    0.0
                } else {
                    ((dead_for - MELT_START) / (MELT_END - MELT_START)).clamp(0.0, 1.0)
                };
                let front = ch.respawn.map_or(-1.0, |t| t / RESPAWN_TIME * 1.1);
                m.extension.params.state = Vec4::new(melt, front, ch.flash, ch.id as f32 * 0.37);
            }
        } else if let Some(h) = ch.lent.take() {
            mats.give_back(h);
        }
        let want_mat = ch.lent.clone().unwrap_or_else(|| ch.shared.clone());
        for &b in &ch.bodies {
            if let Ok((_, mut m)) = bodies.get_mut(b)
                && m.0 != want_mat
            {
                m.0 = want_mat.clone();
            }
        }
    }
}
