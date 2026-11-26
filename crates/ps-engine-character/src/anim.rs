//! Procedural animation controller: locomotion, aim, weapon holds, actions
//! and death, computed every frame from the networked state only.
//!
//! The poses are designed around the hitboxes of `engine-hitreg`: legs stay
//! on the leg capsules (stride capped, feet under the hips), the torso drops
//! by `DUCK_DROP` when crouching and the hands sit where the arm capsules
//! are (`tests::skin_stays_in_hitboxes`). The animation never changes the
//! hitboxes: they remain a function of (origin, yaw, duck).
//!
//! Model space: game units, origin at the feet, x right, y up, -z forward.

use super::rig::{self, Joint, Pose, bind, bind_dir, bind_side, frame_rot, solve_limb};
use bevy::math::{Quat, Vec2, Vec3};
use engine_weapons::WeaponKind;
use std::f32::consts::{PI, TAU};

/// How the weapon is held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Hold {
    #[default]
    Rifle,
    Sniper,
    Pistol,
    Knife,
    Grenade,
    Bomb,
    /// No weapon: relaxed arms.
    Empty,
}

impl Hold {
    pub const ALL: [Hold; 7] = [
        Hold::Rifle,
        Hold::Sniper,
        Hold::Pistol,
        Hold::Knife,
        Hold::Grenade,
        Hold::Bomb,
        Hold::Empty,
    ];

    pub fn from_kind(k: Option<WeaponKind>) -> Hold {
        match k {
            Some(WeaponKind::Rifle | WeaponKind::Smg) => Hold::Rifle,
            Some(WeaponKind::SniperLight | WeaponKind::SniperHeavy) => Hold::Sniper,
            Some(WeaponKind::Pistol) => Hold::Pistol,
            Some(WeaponKind::Knife) => Hold::Knife,
            Some(WeaponKind::Grenade) => Hold::Grenade,
            Some(WeaponKind::Bomb) => Hold::Bomb,
            None => Hold::Empty,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Hold::Rifle => "Fusil",
            Hold::Sniper => "Précision",
            Hold::Pistol => "Pistolet",
            Hold::Knife => "Couteau",
            Hold::Grenade => "Grenade",
            Hold::Bomb => "Bombe",
            Hold::Empty => "Mains nues",
        }
    }
}

/// Hand placement for a hold, in the weapon frame (-z = barrel).
#[derive(Clone, Copy, Debug)]
pub struct HoldDef {
    /// Right wrist, from the aim pivot.
    pub right: Vec3,
    pub r_dir: Vec3,
    pub r_palm: Vec3,
    /// Left wrist relative to the right one; `None` = free left hand.
    pub left: Option<Vec3>,
    pub l_dir: Vec3,
    pub l_palm: Vec3,
    /// Weapon origin (its grip) relative to the right wrist.
    pub grip: Vec3,
    /// Torso twist (degrees, negative = left shoulder forward).
    pub twist: f32,
    /// Finger curl (degrees).
    pub curl: f32,
}

pub fn hold_def(h: Hold) -> HoldDef {
    let n = |x: f32, y: f32, z: f32| Vec3::new(x, y, z).normalize();
    match h {
        Hold::Rifle | Hold::Sniper => HoldDef {
            right: Vec3::new(3.6, -6.0, if h == Hold::Sniper { -8.0 } else { -9.0 }),
            r_dir: n(-0.25, -0.35, -0.9),
            r_palm: n(-1.0, 0.0, 0.1),
            left: Some(Vec3::new(-3.4, 0.8, -11.5)),
            l_dir: n(0.35, 0.05, -0.94),
            l_palm: n(0.25, 1.0, 0.0),
            grip: Vec3::new(-0.6, 1.4, -1.6),
            twist: -22.0,
            curl: 70.0,
        },
        Hold::Pistol => HoldDef {
            right: Vec3::new(1.0, -5.0, -16.0),
            r_dir: n(-0.3, -0.25, -0.92),
            r_palm: n(-1.0, 0.0, 0.0),
            left: Some(Vec3::new(-1.8, -0.7, 0.4)),
            l_dir: n(0.45, 0.2, -0.87),
            l_palm: n(1.0, 0.25, 0.0),
            grip: Vec3::new(-0.6, 1.4, -1.6),
            twist: -6.0,
            curl: 75.0,
        },
        Hold::Knife => HoldDef {
            right: Vec3::new(5.0, -10.0, -10.0),
            r_dir: n(-0.2, -0.1, -0.97),
            r_palm: n(-1.0, 0.0, 0.0),
            left: None,
            l_dir: Vec3::NEG_Z,
            l_palm: Vec3::X,
            grip: Vec3::new(-0.5, 0.4, -1.8),
            twist: -10.0,
            curl: 85.0,
        },
        Hold::Grenade => HoldDef {
            right: Vec3::new(5.5, -6.0, -8.0),
            r_dir: n(-0.2, 0.3, -0.93),
            r_palm: n(-1.0, 0.0, 0.0),
            left: None,
            l_dir: Vec3::NEG_Z,
            l_palm: Vec3::X,
            grip: Vec3::new(-0.6, 0.4, -1.8),
            twist: -8.0,
            curl: 80.0,
        },
        Hold::Bomb => HoldDef {
            right: Vec3::new(3.4, -14.0, -10.0),
            r_dir: n(-0.3, -0.2, -0.93),
            r_palm: n(-1.0, 0.0, 0.0),
            left: Some(Vec3::new(-6.8, 0.0, 0.0)),
            l_dir: n(0.3, -0.2, -0.93),
            l_palm: n(1.0, 0.0, 0.0),
            grip: Vec3::new(-3.4, 0.0, -1.6),
            twist: 0.0,
            curl: 45.0,
        },
        Hold::Empty => HoldDef {
            right: Vec3::new(9.0, -22.0, -3.0),
            r_dir: n(0.05, -1.0, -0.15),
            r_palm: n(-1.0, 0.0, 0.0),
            left: None,
            l_dir: Vec3::NEG_Y,
            l_palm: Vec3::X,
            grip: Vec3::ZERO,
            twist: 0.0,
            curl: 25.0,
        },
    }
}

/// One-shot actions, triggered from game events.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Fire,
    Reload {
        duration: f32,
    },
    Switch {
        duration: f32,
    },
    Slash,
    Throw,
    /// `push`: model-space direction the hit pushes the body to.
    Hit {
        push: Vec3,
    },
    Land {
        strength: f32,
    },
}

/// Networked state the animation is built from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimInput {
    /// Model-space velocity (u/s), -z forward.
    pub velocity: Vec3,
    pub on_ground: bool,
    pub duck: f32,
    /// View pitch (degrees, positive = up).
    pub pitch: f32,
    pub hold: Hold,
    /// Planting or defusing.
    pub working: bool,
    pub alive: bool,
}

impl Default for AnimInput {
    fn default() -> Self {
        AnimInput {
            velocity: Vec3::ZERO,
            on_ground: true,
            duck: 0.0,
            pitch: 0.0,
            hold: Hold::Rifle,
            working: false,
            alive: true,
        }
    }
}

/// Seconds for the body to collapse.
pub const COLLAPSE_TIME: f32 = 0.75;

#[derive(Clone, Copy, Debug)]
struct Timed {
    t: f32,
    dur: f32,
}

impl Timed {
    fn new(dur: f32) -> Self {
        Timed {
            t: 0.0,
            dur: dur.max(0.05),
        }
    }
    fn s(&self) -> f32 {
        (self.t / self.dur).clamp(0.0, 1.0)
    }
}

/// Per-character animation state.
#[derive(Clone, Debug)]
pub struct Animator {
    pub pose: Pose,
    /// Weapon origin and rotation in model space.
    pub weapon: (Vec3, Quat),
    /// Enables secondary motion (Ultra quality).
    pub secondary: bool,
    /// Gait cycle (0..1, one cycle = two steps).
    pub phase: f32,
    speed: f32,
    dir: Vec2,
    duck: f32,
    pitch: f32,
    air: f32,
    land: f32,
    fire: f32,
    hit: f32,
    push: Vec3,
    reload: Option<Timed>,
    switch: Option<Timed>,
    slash: Option<Timed>,
    throw: Option<Timed>,
    hold: Hold,
    prev_vel: Vec3,
    spring: Vec3,
    spring_v: Vec3,
    death: Option<(f32, Pose)>,
    fresh: bool,
}

impl Default for Animator {
    fn default() -> Self {
        Animator {
            pose: Pose::bind(),
            weapon: (Vec3::ZERO, Quat::IDENTITY),
            secondary: false,
            phase: 0.0,
            speed: 0.0,
            dir: Vec2::new(0.0, -1.0),
            duck: 0.0,
            pitch: 0.0,
            air: 0.0,
            land: 0.0,
            fire: 0.0,
            hit: 0.0,
            push: Vec3::ZERO,
            reload: None,
            switch: None,
            slash: None,
            throw: None,
            hold: Hold::Rifle,
            prev_vel: Vec3::ZERO,
            spring: Vec3::ZERO,
            spring_v: Vec3::ZERO,
            death: None,
            fresh: true,
        }
    }
}

fn approach(cur: f32, target: f32, dt: f32, rate: f32) -> f32 {
    cur + (target - cur) * (1.0 - (-dt * rate).exp())
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 0 → 1 → 0 over [a, b] with smooth edges.
fn bump(s: f32, a: f32, b: f32) -> f32 {
    let m = (b - a) * 0.25;
    smoothstep(a, a + m, s) * (1.0 - smoothstep(b - m, b, s))
}

fn rx(deg: f32) -> Quat {
    Quat::from_rotation_x(deg.to_radians())
}
fn ry(deg: f32) -> Quat {
    Quat::from_rotation_y(deg.to_radians())
}
fn rz(deg: f32) -> Quat {
    Quat::from_rotation_z(deg.to_radians())
}

/// Foot offset along the walking direction and lift, for a leg at `p`.
fn gait_foot(p: f32, step: f32, lift: f32) -> (f32, f32, f32) {
    if p < 0.5 {
        let u = p / 0.5;
        (step * (0.5 - u), 0.0, 0.0)
    } else {
        let u = (p - 0.5) / 0.5;
        let e = u * u * (3.0 - 2.0 * u);
        let toe = (u * PI).sin() * 18.0 - (u * TAU).sin().max(0.0) * 6.0;
        (step * (e - 0.5), (u * PI).sin() * lift, toe)
    }
}

impl Animator {
    pub fn trigger(&mut self, a: Action) {
        match a {
            Action::Fire => self.fire = 1.0,
            Action::Reload { duration } => self.reload = Some(Timed::new(duration)),
            Action::Switch { duration } => {
                self.switch = Some(Timed::new(duration.min(1.0)));
                self.reload = None;
            }
            Action::Slash => self.slash = Some(Timed::new(0.45)),
            Action::Throw => self.throw = Some(Timed::new(0.6)),
            Action::Hit { push } => {
                self.hit = 1.0;
                self.push = push.normalize_or_zero();
            }
            Action::Land { strength } => self.land = self.land.max(strength.clamp(0.0, 1.0)),
        }
    }

    #[cfg(test)]
    pub fn is_dead(&self) -> bool {
        self.death.is_some()
    }

    /// Seconds since death (0 if alive).
    pub fn death_time(&self) -> f32 {
        self.death.as_ref().map_or(0.0, |d| d.0)
    }

    pub fn update(&mut self, inp: &AnimInput, dt: f32) -> &Pose {
        let dt = dt.clamp(0.0, 0.1);
        if !inp.alive {
            let from = self.death.map_or(self.pose, |d| d.1);
            let t = self.death.map_or(0.0, |d| d.0) + dt;
            self.death = Some((t, from));
            self.pose = collapse(&from, t);
            return &self.pose;
        }
        if self.death.take().is_some() {
            self.fresh = true;
        }
        let target = self.target(inp, dt);
        if self.fresh {
            self.pose = target;
            self.fresh = false;
        } else {
            self.pose = self.pose.lerp(&target, 1.0 - (-dt * 28.0).exp());
        }
        // The weapon follows the (smoothed) right hand.
        let d = hold_def(self.hold);
        let fh = frame_rot(
            bind_dir(Joint::WristR),
            bind_side(Joint::WristR),
            d.r_dir,
            d.r_palm,
        );
        let wr = self.pose.get(Joint::WristR);
        let w_rot = (wr.rot * fh.inverse()).normalize();
        self.weapon = (wr.pos + w_rot * d.grip, w_rot);
        &self.pose
    }

    fn tick_timers(&mut self, dt: f32) {
        for t in [
            &mut self.reload,
            &mut self.switch,
            &mut self.slash,
            &mut self.throw,
        ] {
            if let Some(x) = t {
                x.t += dt;
                if x.t >= x.dur {
                    *t = None;
                }
            }
        }
        self.fire = (self.fire - dt * 9.0).max(0.0);
        self.hit = (self.hit - dt * 5.0).max(0.0);
        self.land = (self.land - dt * 4.0).max(0.0);
    }

    fn target(&mut self, inp: &AnimInput, dt: f32) -> Pose {
        use Joint::*;
        self.tick_timers(dt);
        if inp.hold != self.hold {
            self.hold = inp.hold;
        }
        let v = inp.velocity;
        let planar = Vec2::new(v.x, v.z);
        let speed = planar.length();
        self.speed = approach(self.speed, speed, dt, 10.0);
        if speed > 5.0 {
            let d = planar / speed;
            self.dir = self.dir.lerp(d, 1.0 - (-dt * 12.0).exp()).normalize_or(d);
        }
        self.duck = approach(self.duck, inp.duck.clamp(0.0, 1.0), dt, 18.0);
        self.pitch = approach(self.pitch, inp.pitch.clamp(-89.0, 89.0), dt, 30.0);
        self.air = approach(self.air, if inp.on_ground { 0.0 } else { 1.0 }, dt, 12.0);
        if self.secondary {
            let acc = (v - self.prev_vel) / dt.max(1e-3);
            let f = -acc * 0.00035 - self.spring * 90.0 - self.spring_v * 12.0;
            self.spring_v += f * dt;
            self.spring = (self.spring + self.spring_v * dt).clamp_length_max(1.2);
        } else {
            self.spring = Vec3::ZERO;
            self.spring_v = Vec3::ZERO;
        }
        self.prev_vel = v;

        let duck = self.duck;
        let grounded = 1.0 - self.air;
        let walk = smoothstep(4.0, 40.0, self.speed) * grounded;
        // Stride capped so the feet stay near the leg hitboxes; above the
        // cadence cap the feet slide a little instead of over-striding.
        let step = (8.0 + self.speed * 0.07).min(26.0) * (1.0 - 0.35 * duck);
        let cadence = (self.speed / step).min(4.4 - 1.2 * duck);
        if walk > 0.01 {
            self.phase = (self.phase + cadence * 0.5 * dt).fract();
        }
        let p = self.phase;
        let dir3 = Vec3::new(self.dir.x, 0.0, self.dir.y);
        let fwd_speed = (-v.z / 250.0).clamp(-1.0, 1.0);
        let side_speed = (v.x / 250.0).clamp(-1.0, 1.0);

        let mut pose = Pose::bind();
        // --- Pelvis and torso -------------------------------------------
        let bob = walk * (0.6 + self.speed * 0.004) * (p * 2.0 * TAU).cos();
        let pelvis = Vec3::new(
            (p * TAU).sin() * 0.5 * walk,
            36.0 - 18.0 * duck - 3.5 * self.land - bob,
            4.0 * duck,
        ) + self.spring * Vec3::new(1.0, 0.3, 1.0);
        let hip_twist = (p * TAU).sin() * 7.0 * walk * (1.0 - 0.5 * duck);
        let lean = 10.0 * duck + 6.0 * fwd_speed * walk + 3.0 * self.land;
        let roll = -4.0 * side_speed * walk;
        let d = hold_def(self.hold);
        let mut twist = d.twist;
        let mut flinch = Quat::IDENTITY;
        if self.hit > 0.0 && self.push != Vec3::ZERO {
            let axis = Vec3::Y.cross(self.push).normalize_or_zero();
            if axis != Vec3::ZERO {
                flinch = Quat::from_axis_angle(axis, (9.0 * self.hit).to_radians());
            }
        }
        if let Some(s) = self.slash {
            twist += 14.0 * bump(s.s(), 0.0, 1.0);
        }
        if let Some(s) = self.throw {
            twist += 18.0 * bump(s.s(), 0.0, 0.5) - 12.0 * bump(s.s(), 0.35, 0.9);
        }
        let pitch = self.pitch;
        pose.get_mut(Pelvis).pos = pelvis;
        pose.get_mut(Pelvis).rot = ry(hip_twist) * rx(-lean * 0.3) * rz(roll * 0.5);
        let spine =
            flinch * ry(twist * 0.45 - hip_twist * 0.6) * rx(pitch * 0.12 - lean * 0.8) * rz(roll);
        let chest = flinch * ry(twist) * rx(pitch * 0.22 - lean) * rz(roll);
        pose.get_mut(Spine).rot = spine;
        pose.get_mut(Chest).rot = chest;
        pose.get_mut(Neck).rot = ry(twist * 0.5) * rx(pitch * 0.4 - lean * 0.6);
        let head_pitch = (pitch * 0.55).clamp(-30.0, 30.0) - lean * 0.2;
        pose.get_mut(Head).rot = ry(-hip_twist * 0.2) * rx(head_pitch);
        for j in [ShoulderL, ShoulderR] {
            pose.get_mut(j).rot = chest;
        }
        for j in [HipL, HipR] {
            pose.get_mut(j).rot = pose.get(Pelvis).rot;
        }
        pose.propagate(Pelvis);
        if self.secondary {
            // Liquid wobble: the head lags behind the chest.
            pose.get_mut(Head).pos += self.spring * 0.6;
        }

        // --- Legs ------------------------------------------------------------
        let lift = (2.2 + self.speed * 0.012).min(5.0) * (1.0 - 0.4 * duck);
        let vy = v.y;
        let tuck = self.air * (0.35 + vy / 300.0).clamp(0.1, 1.0);
        for (left, ph, hip, knee, ankle, toe) in [
            (true, p, HipL, KneeL, AnkleL, ToeL),
            (false, (p + 0.5).fract(), HipR, KneeR, AnkleR, ToeR),
        ] {
            let side = if left { -1.0 } else { 1.0 };
            let (along, up, toe_pitch) = gait_foot(ph, step, lift);
            let mut base = Vec3::new(side * (5.0 + 1.4 * duck), rig::BIND[AnkleL.i()].y, 0.6);
            base.z += if left { 3.5 * duck } else { -1.5 * duck };
            base.x += pelvis.x * 0.3;
            let leg_tuck = tuck * if left { 1.0 } else { 0.7 };
            let mut target = base + dir3 * (along * walk) + Vec3::Y * (up * walk);
            target += Vec3::new(
                0.0,
                7.0 * leg_tuck,
                if left { -1.5 } else { 1.0 } * leg_tuck * 2.0,
            );
            let forward = pose.get(Pelvis).rot * Vec3::NEG_Z;
            let pole = (forward + Vec3::X * side * 0.15).normalize();
            solve_limb(&mut pose, hip, knee, ankle, target, pole);
            let foot_rot =
                ry(-side * 7.0 + hip_twist * 0.3) * rx(toe_pitch * walk + 12.0 * leg_tuck);
            let a = pose.get(ankle).pos;
            pose.get_mut(ankle).rot = foot_rot;
            pose.get_mut(toe).pos = a + foot_rot * (bind(toe) - bind(ankle));
            pose.get_mut(toe).rot = foot_rot;
        }

        // --- Arms and weapon ----------------------------------------------
        let pivot = pose.get(Chest).pos + Vec3::new(0.0, 7.0, -0.3);
        let sway = (p * TAU).sin() * 1.6 * walk;
        let mut w_rot = ry(sway) * rx(pitch);
        let mut w_off = Vec3::ZERO;
        if self.fire > 0.0 {
            w_off += Vec3::new(0.0, 0.3, 1.6) * self.fire;
            w_rot *= rx(4.0 * self.fire);
        }
        if let Some(s) = self.switch {
            let lower = 1.0 - smoothstep(0.2, 1.0, s.s());
            w_rot *= rx(-45.0 * lower);
            w_off += Vec3::new(0.0, -5.0, 3.0) * lower;
        }
        let mut left_override: Option<(Vec3, f32)> = None;
        if let Some(r) = self.reload {
            let s = r.s();
            let tilt = bump(s, 0.0, 0.95);
            w_rot *= rz(22.0 * tilt) * rx(-10.0 * tilt);
            w_off += Vec3::new(0.0, -1.5, 1.0) * tilt;
            // Left hand: magazine → belt → magazine → support.
            let mag = Vec3::new(-1.0, -5.0, -4.0);
            let belt = Vec3::new(-7.0, -16.0, 4.0);
            let m = bump(s, 0.08, 0.85);
            let b = bump(s, 0.28, 0.6);
            left_override = Some((mag.lerp(belt, b), m));
        }
        if let Some(s) = self.slash {
            let s = s.s();
            let swing = smoothstep(0.0, 0.45, s) * (1.0 - smoothstep(0.6, 1.0, s));
            let yaw = (40.0 * smoothstep(0.0, 0.15, s) - 95.0 * smoothstep(0.15, 0.45, s))
                * (1.0 - smoothstep(0.6, 1.0, s));
            w_rot = ry(sway + yaw) * rx(pitch - 15.0 * swing);
            w_off += Vec3::new(-2.0, 2.0, -6.0) * swing;
        }
        let mut right_world = pivot + w_rot * (d.right + w_off);
        if let Some(s) = self.throw {
            let s = s.s();
            let wind = bump(s, 0.0, 0.5);
            let release = bump(s, 0.35, 1.0);
            right_world += pose.get(Chest).rot
                * (Vec3::new(1.0, 12.0, 9.0) * wind + Vec3::new(-3.0, 4.0, -10.0) * release);
        }
        if inp.working {
            right_world = Vec3::new(3.0, pose.get(Pelvis).pos.y - 10.0, -13.0);
        }
        let r_pole = (w_rot * Vec3::new(0.9, -1.0, 0.5)).normalize();
        solve_limb(&mut pose, ShoulderR, ElbowR, WristR, right_world, r_pole);
        let r_hand = frame_rot(
            bind_dir(WristR),
            bind_side(WristR),
            w_rot * d.r_dir,
            w_rot * d.r_palm,
        );
        set_hand(&mut pose, WristR, FingersR, r_hand, d.curl);
        let right_at = pose.get(WristR).pos;

        let chest_rot = pose.get(Chest).rot;
        let chest_pos = pose.get(Chest).pos;
        let (mut left_world, mut l_dir, mut l_palm, mut l_curl) = match d.left {
            Some(off) => (
                right_at + w_rot * off,
                w_rot * d.l_dir,
                w_rot * d.l_palm,
                55.0,
            ),
            None => {
                let guard = if self.hold == Hold::Knife {
                    Vec3::new(-6.0, -7.0, -9.0)
                } else {
                    Vec3::new(-9.5, -19.0, -3.0)
                };
                (
                    chest_pos + chest_rot * guard,
                    chest_rot * Vec3::new(0.05, -1.0, -0.3).normalize(),
                    chest_rot * Vec3::X,
                    30.0,
                )
            }
        };
        if let Some((off, m)) = left_override {
            left_world = left_world.lerp(right_at + w_rot * off, m);
            l_dir = l_dir.lerp(w_rot * Vec3::NEG_Y, m).normalize_or(l_dir);
            l_curl += 20.0 * m;
        }
        if inp.working {
            left_world = Vec3::new(-3.0, pose.get(Pelvis).pos.y - 10.0, -13.0);
            l_dir = Vec3::new(0.0, -0.4, -1.0).normalize();
            l_palm = Vec3::NEG_Y;
        }
        let l_pole = (w_rot * Vec3::new(-0.9, -1.0, 0.3)).normalize();
        solve_limb(&mut pose, ShoulderL, ElbowL, WristL, left_world, l_pole);
        let l_hand = frame_rot(bind_dir(WristL), bind_side(WristL), l_dir, l_palm);
        set_hand(&mut pose, WristL, FingersL, l_hand, l_curl);
        pose
    }
}

/// Orients a hand and curls its fingers towards the palm.
fn set_hand(pose: &mut Pose, wrist: Joint, fingers: Joint, rot: Quat, curl: f32) {
    pose.get_mut(wrist).rot = rot;
    let axis = bind_dir(fingers).cross(bind_side(fingers)).normalize();
    let w = pose.get(wrist).pos;
    pose.get_mut(fingers).pos = w + rot * (bind(fingers) - bind(wrist));
    pose.get_mut(fingers).rot = rot * Quat::from_axis_angle(axis, -curl.to_radians());
}

/// Death: the body loses cohesion and slumps into a flattened heap.
pub fn collapse(from: &Pose, t: f32) -> Pose {
    let s = (t / COLLAPSE_TIME).clamp(0.0, 1.0);
    // Accelerating fall, then spreading.
    let fall = s * s;
    let spread = smoothstep(0.3, 1.0, s);
    let c = from.get(Joint::Pelvis).pos;
    let mut out = *from;
    for (i, x) in out.0.iter_mut().enumerate() {
        let f = from.0[i];
        let off = Vec3::new(f.pos.x - c.x, 0.0, f.pos.z - c.z);
        let ground = 0.8 + f.pos.y * 0.06;
        x.pos = Vec3::new(c.x, 0.0, c.z)
            + off * (1.0 + 0.8 * spread)
            + Vec3::Y * (f.pos.y + (ground - f.pos.y) * fall);
        x.scale = Vec3::new(1.0 + 0.7 * spread, 1.0 - 0.7 * fall, 1.0 + 0.7 * spread);
        x.rot = f.rot;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::mesh::{self, Lod, Parts};
    use super::*;

    fn run(inp: AnimInput, secs: f32) -> Animator {
        let mut a = Animator::default();
        let mut t = 0.0;
        while t < secs {
            a.update(&inp, 1.0 / 128.0);
            t += 1.0 / 128.0;
        }
        a
    }

    fn skinned(pose: &Pose, m: &mesh::MeshData) -> Vec<Vec3> {
        let mats: Vec<_> = (0..rig::JOINT_COUNT).map(|j| pose.skin_matrix(j)).collect();
        m.positions
            .iter()
            .zip(m.joints.iter().zip(&m.weights))
            .map(|(p, (j, w))| {
                let p = Vec3::from_array(*p);
                (0..4)
                    .map(|k| mats[j[k] as usize].transform_point3(p) * w[k])
                    .sum()
            })
            .collect()
    }

    fn capsule_dist(p: Vec3, c: &engine_hitreg::Capsule) -> f32 {
        let ab = c.b - c.a;
        let t = ((p - c.a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
        (p - (c.a + ab * t)).length() - c.radius
    }

    /// Share of skin vertices within `margin` of a hitbox.
    fn coverage(pose: &Pose, duck: f32, margin: f32) -> f32 {
        let m = mesh::humanoid(Lod::Mid, Parts::FullBody);
        let hb = engine_hitreg::hitboxes(&engine_hitreg::Pose {
            origin: Vec3::ZERO,
            yaw: 0.0,
            duck,
        });
        let pts = skinned(pose, &m);
        let inside = pts
            .iter()
            .filter(|p| hb.capsules.iter().any(|c| capsule_dist(**p, c) < margin))
            .count();
        inside as f32 / pts.len() as f32
    }

    #[test]
    fn skin_stays_in_hitboxes() {
        let cases = [
            ("debout", AnimInput::default(), 0.0),
            (
                "accroupi",
                AnimInput {
                    duck: 1.0,
                    ..Default::default()
                },
                1.0,
            ),
            (
                "course",
                AnimInput {
                    velocity: Vec3::new(0.0, 0.0, -250.0),
                    ..Default::default()
                },
                0.0,
            ),
            (
                "pistolet",
                AnimInput {
                    hold: Hold::Pistol,
                    ..Default::default()
                },
                0.0,
            ),
        ];
        for (name, inp, duck) in cases {
            let a = run(inp, 1.3);
            let c = coverage(&a.pose, duck, 3.0);
            assert!(
                c > 0.9,
                "{name}: {:.1}% de la peau près des hitboxes",
                c * 100.0
            );
        }
    }

    #[test]
    fn limbs_keep_their_length() {
        for inp in [
            AnimInput {
                velocity: Vec3::new(120.0, 0.0, -180.0),
                ..Default::default()
            },
            AnimInput {
                duck: 1.0,
                pitch: -60.0,
                ..Default::default()
            },
        ] {
            let mut a = Animator::default();
            for _ in 0..200 {
                a.update(&inp, 1.0 / 128.0);
                for j in [Joint::KneeL, Joint::AnkleR, Joint::ElbowR, Joint::WristL] {
                    let p = j.parent().unwrap();
                    let len = a.pose.get(j).pos.distance(a.pose.get(p).pos);
                    let err = (len - rig::bone_length(j)).abs();
                    assert!(err < 0.6, "{j:?}: {len} vs {}", rig::bone_length(j));
                }
            }
        }
    }

    #[test]
    fn feet_stay_above_ground_while_running() {
        let mut a = Animator::default();
        let inp = AnimInput {
            velocity: Vec3::new(0.0, 0.0, -250.0),
            ..Default::default()
        };
        let mut min_y = f32::MAX;
        for _ in 0..512 {
            a.update(&inp, 1.0 / 128.0);
            for j in [Joint::AnkleL, Joint::AnkleR] {
                min_y = min_y.min(a.pose.get(j).pos.y);
            }
        }
        assert!(min_y > 2.8, "cheville à {min_y}");
        assert!(a.phase > 0.0);
    }

    #[test]
    fn hands_reach_the_weapon() {
        for h in Hold::ALL {
            let a = run(
                AnimInput {
                    hold: h,
                    ..Default::default()
                },
                0.5,
            );
            let d = hold_def(h);
            let (wpos, wrot) = a.weapon;
            let right = a.pose.get(Joint::WristR).pos;
            assert!(right.distance(wpos - wrot * d.grip) < 0.5, "{h:?}");
            if let Some(off) = d.left {
                let want = right + wrot * off;
                let got = a.pose.get(Joint::WristL).pos;
                assert!(
                    got.distance(want) < 1.0,
                    "{h:?}: main gauche à {}",
                    got.distance(want)
                );
            }
        }
    }

    #[test]
    fn death_collapses_to_the_ground() {
        let mut a = run(AnimInput::default(), 0.3);
        let dead = AnimInput {
            alive: false,
            ..Default::default()
        };
        for _ in 0..150 {
            a.update(&dead, 1.0 / 128.0);
        }
        let head = a.pose.get(Joint::Head).pos.y;
        assert!(head < 6.0, "tête à {head}");
        assert!(a.is_dead());
        a.update(&AnimInput::default(), 1.0 / 128.0);
        assert!(!a.is_dead());
        assert!(a.pose.get(Joint::Head).pos.y > 55.0);
    }
}
