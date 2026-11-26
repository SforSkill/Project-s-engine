//! Procedural low-poly humanoid: lofted rings around the bind skeleton,
//! flat-shaded (one normal per facet) and skinned with blended weights.
//!
//! This file *is* the source of the character asset: every proportion is a
//! number below, in game units around the bind pose of `rig.rs`. Alternate
//! rings are rotated half a step so the surface breaks into triangular
//! facets instead of quads. `--export-character` writes the result as glTF
//! for Blender (see `export.rs`).
//!
//! Vertex data consumed by `liquid.wesl`:
//! - `UV_0.x`: bind height / 72 (drives melting from the top and the colour
//!   gradient), `UV_0.y`: one random value per facet (tone variation);
//! - `COLOR`: bind-space position (x / 80 + 0.5, y / 80, z / 80 + 0.5) so the
//!   internal ripples are attached to the body instead of sliding in world
//!   space, alpha = region (0 body, 0.5 hands, 1 head).

use super::rig::{self, Joint, bind};
use bevy::math::Vec3;

/// Skin influence of one vertex (up to four joints).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skin {
    pub joints: [u16; 4],
    pub weights: [f32; 4],
}

impl Skin {
    pub fn one(j: Joint) -> Skin {
        Skin {
            joints: [j as u16, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        }
    }

    /// Builds a skin from weighted joints, keeping the four strongest and
    /// normalising the sum to one.
    pub fn mix(list: &[(Joint, f32)]) -> Skin {
        let mut acc: Vec<(Joint, f32)> = Vec::with_capacity(list.len());
        for &(j, w) in list {
            if w <= 1e-4 {
                continue;
            }
            match acc.iter_mut().find(|(k, _)| *k == j) {
                Some(e) => e.1 += w,
                None => acc.push((j, w)),
            }
        }
        acc.sort_by(|a, b| b.1.total_cmp(&a.1));
        acc.truncate(4);
        let sum: f32 = acc.iter().map(|e| e.1).sum();
        let mut s = Skin {
            joints: [0; 4],
            weights: [0.0; 4],
        };
        if sum <= 0.0 {
            return Skin::one(Joint::Root);
        }
        for (i, (j, w)) in acc.iter().enumerate() {
            s.joints[i] = *j as u16;
            s.weights[i] = w / sum;
        }
        s
    }
}

/// Region code written in the vertex colour alpha.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Region {
    Body,
    Hand,
    Head,
}

impl Region {
    fn code(self) -> f32 {
        match self {
            Region::Body => 0.0,
            Region::Hand => 0.5,
            Region::Head => 1.0,
        }
    }
}

/// Level of detail of the generated body.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Lod {
    /// Close range and first-person arms.
    High,
    /// Medium range.
    Mid,
    /// Far range: fewer sides, intermediate rings dropped.
    Far,
}

impl Lod {
    /// Sides of (torso and head, limbs, hands).
    fn sides(self) -> (usize, usize, usize) {
        match self {
            Lod::High => (20, 12, 8),
            Lod::Mid => (14, 9, 6),
            Lod::Far => (10, 6, 4),
        }
    }
}

/// Which parts to build.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Parts {
    FullBody,
    /// First-person: forearms and hands only. The upper arms would cross
    /// the camera (the rig keeps the third-person rest pose); the open elbow
    /// end stays below the frame.
    ArmsOnly,
}

/// Flat-shaded triangle soup, ready for a Bevy mesh or a glTF buffer.
#[derive(Clone, Debug, Default)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 4]>,
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
}

impl MeshData {
    pub fn triangle_count(&self) -> usize {
        self.positions.len() / 3
    }

    #[cfg(test)]
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for p in &self.positions {
            let p = Vec3::from_array(*p);
            lo = lo.min(p);
            hi = hi.max(p);
        }
        (lo, hi)
    }
}

/// Deterministic hash → [0, 1).
pub fn hash01(mut x: u32) -> f32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// Symmetric jitter: the same for a point and its mirror across x = 0.
fn jitter(p: Vec3) -> f32 {
    let q = |v: f32| (v * 4.0).round() as i32 as u32;
    let h = q(p.x.abs()).wrapping_mul(73_856_093)
        ^ q(p.y).wrapping_mul(19_349_663)
        ^ q(p.z).wrapping_mul(83_492_791);
    hash01(h) * 2.0 - 1.0
}

struct Tri {
    v: [u32; 3],
    /// A point known to be inside the part: the winding is fixed so the
    /// normal points away from it.
    inside: Vec3,
}

#[derive(Default)]
struct Builder {
    pos: Vec<Vec3>,
    skin: Vec<Skin>,
    region: Vec<Region>,
    tris: Vec<Tri>,
}

#[derive(Clone, Copy)]
struct RingSpec {
    center: Vec3,
    /// Lateral axis (radius `ru`) and second axis (radius `rv`).
    u: Vec3,
    v: Vec3,
    ru: f32,
    rv: f32,
    /// Dropped at `Lod::Far`.
    detail: bool,
}

impl Builder {
    fn vert(&mut self, p: Vec3, s: Skin, r: Region) -> u32 {
        self.pos.push(p);
        self.skin.push(s);
        self.region.push(r);
        (self.pos.len() - 1) as u32
    }

    fn ring(
        &mut self,
        r: &RingSpec,
        n: usize,
        phase: f32,
        organic: f32,
        skin: &dyn Fn(Vec3) -> Skin,
        region: Region,
    ) -> Vec<u32> {
        (0..n)
            .map(|i| {
                let a = std::f32::consts::TAU * (i as f32 + phase) / n as f32;
                let base = r.center + r.u * (a.cos() * r.ru) + r.v * (a.sin() * r.rv);
                let k = 1.0 + organic * jitter(base);
                let p = r.center + (base - r.center) * k;
                self.vert(p, skin(p), region)
            })
            .collect()
    }

    /// Joins two rings of the same size; `b` is offset by half a step when
    /// `staggered`, which gives triangular facets.
    fn bridge(&mut self, a: &[u32], b: &[u32], staggered: bool, inside: Vec3) {
        let n = a.len();
        for i in 0..n {
            let i1 = (i + 1) % n;
            if staggered {
                self.tris.push(Tri {
                    v: [a[i], a[i1], b[i]],
                    inside,
                });
                self.tris.push(Tri {
                    v: [b[i], a[i1], b[i1]],
                    inside,
                });
            } else {
                self.tris.push(Tri {
                    v: [a[i], a[i1], b[i1]],
                    inside,
                });
                self.tris.push(Tri {
                    v: [a[i], b[i1], b[i]],
                    inside,
                });
            }
        }
    }

    fn cap(&mut self, ring: &[u32], apex: u32, inside: Vec3) {
        let n = ring.len();
        for i in 0..n {
            self.tris.push(Tri {
                v: [ring[i], ring[(i + 1) % n], apex],
                inside,
            });
        }
    }

    /// Lofts a list of rings with alternating phase, capped at both ends.
    fn loft(
        &mut self,
        rings: &[RingSpec],
        n: usize,
        lod: Lod,
        organic: f32,
        start_apex: Option<Vec3>,
        end_apex: Option<Vec3>,
        skin: &dyn Fn(Vec3) -> Skin,
        region: Region,
    ) {
        let kept: Vec<&RingSpec> = rings
            .iter()
            .enumerate()
            .filter(|(i, r)| !(lod == Lod::Far && r.detail && *i != 0 && *i != rings.len() - 1))
            .map(|(_, r)| r)
            .collect();
        let mut prev: Option<(Vec<u32>, Vec3)> = None;
        let mut first: Option<(Vec<u32>, Vec3)> = None;
        for (k, r) in kept.iter().enumerate() {
            let phase = if k % 2 == 0 { 0.0 } else { 0.5 };
            let ids = self.ring(r, n, phase, organic, skin, region);
            if let Some((p, c)) = &prev {
                let inside = (*c + r.center) * 0.5;
                self.bridge(p, &ids, true, inside);
            }
            if first.is_none() {
                first = Some((ids.clone(), r.center));
            }
            prev = Some((ids, r.center));
        }
        if let (Some(apex), Some((ids, c))) = (start_apex, &first) {
            let a = self.vert(apex, skin(apex), region);
            let inside = *c + (*c - apex) * 0.5;
            self.cap(ids, a, inside);
        }
        if let (Some(apex), Some((ids, c))) = (end_apex, &prev) {
            let a = self.vert(apex, skin(apex), region);
            let inside = *c + (*c - apex) * 0.5;
            self.cap(ids, a, inside);
        }
    }

    fn finish(self) -> MeshData {
        let mut m = MeshData::default();
        let cap = self.tris.len() * 3;
        m.positions.reserve(cap);
        m.normals.reserve(cap);
        m.uvs.reserve(cap);
        m.colors.reserve(cap);
        m.joints.reserve(cap);
        m.weights.reserve(cap);
        // Oriented face normals, then area-weighted vertex normals: the
        // shading is mostly smooth (rounded body) with a little of the facet
        // kept, so the low-poly look stays without blocky limbs.
        let faces: Vec<Option<(Vec3, [u32; 3])>> = self
            .tris
            .iter()
            .map(|t| {
                let [a, b, c] = t.v.map(|i| self.pos[i as usize]);
                let n = (b - a).cross(c - a);
                if n.length_squared() < 1e-12 {
                    return None;
                }
                let centroid = (a + b + c) / 3.0;
                Some(if n.dot(centroid - t.inside) < 0.0 {
                    (-n, [t.v[0], t.v[2], t.v[1]])
                } else {
                    (n, t.v)
                })
            })
            .collect();
        let mut smooth = vec![Vec3::ZERO; self.pos.len()];
        for (n, v) in faces.iter().flatten() {
            for i in v {
                smooth[*i as usize] += *n;
            }
        }
        for (fi, f) in faces.iter().enumerate() {
            let Some((n, order)) = *f else { continue };
            let n = n.normalize();
            let facet = hash01(fi as u32 ^ 0x9e37_79b9);
            for i in order {
                let p = self.pos[i as usize];
                let s = self.skin[i as usize];
                let vn = smooth[i as usize].normalize_or(n);
                m.positions.push(p.to_array());
                m.normals.push(
                    (vn * FACET_SMOOTHING + n * (1.0 - FACET_SMOOTHING))
                        .normalize_or(n)
                        .to_array(),
                );
                m.uvs.push([p.y / 72.0, facet]);
                m.colors.push([
                    p.x / 80.0 + 0.5,
                    p.y / 80.0,
                    p.z / 80.0 + 0.5,
                    self.region[i as usize].code(),
                ]);
                m.joints.push(s.joints);
                m.weights.push(s.weights);
            }
        }
        m
    }
}

/// Share of the smooth vertex normal in the shading normal (1 = fully
/// smooth, 0 = flat facets).
const FACET_SMOOTHING: f32 = 0.8;

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Torso, neck and head skinning by height, with shoulder and hip influence
/// on the sides so raised arms and bent legs do not tear the surface.
fn torso_skin(p: Vec3) -> Skin {
    let keys = [
        (Joint::Pelvis, 36.0),
        (Joint::Spine, 42.0),
        (Joint::Chest, 49.5),
        (Joint::Neck, 58.8),
        (Joint::Head, 62.0),
    ];
    let mut w: Vec<(Joint, f32)> = Vec::with_capacity(6);
    let mut below = 1.0;
    for k in 1..keys.len() {
        let (j, y) = keys[k];
        let b = if j == Joint::Head { 0.8 } else { 2.0 };
        let up = smooth(y - b, y + b, p.y);
        w.push((keys[k - 1].0, below * (1.0 - up)));
        below *= up;
        if k == keys.len() - 1 {
            w.push((j, below));
        }
    }
    let side = if p.x < 0.0 { -1.0 } else { 1.0 };
    let sh = smooth(4.8, 7.4, p.x.abs()) * smooth(50.0, 55.0, p.y) * 0.6;
    if sh > 0.0 {
        let j = if side < 0.0 {
            Joint::ShoulderL
        } else {
            Joint::ShoulderR
        };
        for e in w.iter_mut() {
            e.1 *= 1.0 - sh;
        }
        w.push((j, sh));
    }
    let hip = smooth(2.0, 5.5, p.x.abs()) * (1.0 - smooth(31.5, 36.0, p.y)) * 0.55;
    if hip > 0.0 {
        let j = if side < 0.0 { Joint::HipL } else { Joint::HipR };
        for e in w.iter_mut() {
            e.1 *= 1.0 - hip;
        }
        w.push((j, hip));
    }
    Skin::mix(&w)
}

/// Rings along a bone segment `a → b` at parameters `t`, radii `(ru, rv)` and
/// an offset along the second axis (calf, forearm bulge).
fn segment_rings(
    a: Vec3,
    b: Vec3,
    lateral: Vec3,
    spec: &[(f32, f32, f32, f32, bool)],
) -> Vec<RingSpec> {
    let dir = (b - a).normalize();
    let u = (lateral - dir * lateral.dot(dir)).normalize();
    let v = dir.cross(u);
    spec.iter()
        .map(|&(t, ru, rv, off, detail)| RingSpec {
            center: a + (b - a) * t + v * off,
            u,
            v,
            ru,
            rv,
            detail,
        })
        .collect()
}

/// Weight of `child` along a segment parameter: the transition is centred
/// on the joint (t = 1) and spans `±w`.
fn seg_skin(parent: Joint, child: Joint, t: f32, w: f32) -> Skin {
    let c = smooth(1.0 - w, 1.0 + w * 0.5, t);
    Skin::mix(&[(parent, 1.0 - c), (child, c)])
}

fn param(a: Vec3, b: Vec3, p: Vec3) -> f32 {
    let d = b - a;
    (p - a).dot(d) / d.length_squared()
}

fn build_torso(bd: &mut Builder, lod: Lod) {
    let (n, _, _) = lod.sides();
    // (y, half-width, half-depth, z centre, detail)
    // The pelvis is as wide as the thigh tops (hips at x = ±4.6, thigh
    // radius ~3.2) so the legs come out of it instead of sticking out.
    let body: [(f32, f32, f32, f32, bool); 14] = [
        (30.2, 3.4, 2.9, 0.2, false),
        (31.8, 6.4, 3.7, 0.2, true),
        (34.0, 7.5, 4.1, 0.3, false),
        (36.8, 7.1, 4.3, 0.4, true),
        (39.5, 6.2, 4.0, 0.2, true),
        (43.0, 5.8, 4.0, 0.1, false),
        (47.0, 6.5, 4.5, 0.0, true),
        (51.0, 7.1, 4.8, -0.1, false),
        (54.5, 7.4, 4.5, 0.1, true),
        // Trapezius slope down to the neck, under the deltoids.
        (56.4, 6.8, 4.0, 0.2, false),
        (57.6, 5.0, 3.4, 0.3, true),
        (58.6, 3.6, 2.9, 0.4, false),
        (59.8, 2.45, 2.45, 0.3, true),
        (61.6, 2.3, 2.4, 0.2, false),
    ];
    let head: [(f32, f32, f32, f32, bool); 6] = [
        (62.6, 3.0, 3.2, -0.6, false),
        (64.0, 3.35, 3.9, -0.8, true),
        (66.0, 3.4, 4.1, -0.6, false),
        (68.2, 3.3, 4.0, -0.3, true),
        (70.2, 2.7, 3.3, -0.1, false),
        (71.3, 1.5, 1.8, 0.0, false),
    ];
    let mk = |(y, ru, rv, z, d): (f32, f32, f32, f32, bool)| RingSpec {
        center: Vec3::new(0.0, y, z),
        u: Vec3::X,
        v: Vec3::Z,
        ru,
        rv,
        detail: d,
    };
    // Body (pelvis → neck) and head are two lofts sharing the last neck ring
    // position, so the seam is closed and the head carries its own region code.
    let skin = |p: Vec3| torso_skin(p);
    let lower: Vec<RingSpec> = body.iter().map(|r| mk(*r)).collect();
    bd.loft(
        &lower,
        n,
        lod,
        0.035,
        Some(Vec3::new(0.0, 29.2, 0.2)),
        None,
        &skin,
        Region::Body,
    );
    // Head loft starts at the last neck ring so the seam is closed.
    let mut hr: Vec<RingSpec> = vec![mk(body[body.len() - 1])];
    hr.extend(head.iter().map(|r| mk(*r)));
    bd.loft(
        &hr,
        n,
        lod,
        0.03,
        None,
        Some(Vec3::new(0.0, 71.75, 0.0)),
        &skin,
        Region::Head,
    );
}

fn build_arm(bd: &mut Builder, lod: Lod, left: bool, upper_arm: bool) {
    let (_, n, nh) = lod.sides();
    let (sh, el, wr, fi) = if left {
        (
            Joint::ShoulderL,
            Joint::ElbowL,
            Joint::WristL,
            Joint::FingersL,
        )
    } else {
        (
            Joint::ShoulderR,
            Joint::ElbowR,
            Joint::WristR,
            Joint::FingersR,
        )
    };
    let (s, e, w, f) = (bind(sh), bind(el), bind(wr), bind(fi));
    // Upper arm (deltoid cap above the joint, inside the torso).
    let upper = segment_rings(
        s,
        e,
        Vec3::X,
        &[
            (-0.1, 1.9, 2.0, 0.0, false),
            (0.04, 2.35, 2.45, 0.0, true),
            (0.25, 2.25, 2.2, -0.1, false),
            (0.55, 1.95, 1.9, 0.0, true),
            (0.85, 1.7, 1.6, 0.0, false),
            (1.0, 1.62, 1.55, 0.0, false),
        ],
    );
    let up_skin = move |p: Vec3| {
        let t = param(s, e, p);
        let base = seg_skin(sh, el, t, 0.18);
        if t < 0.12 {
            // Top of the deltoid partly follows the chest.
            let c = 0.35 * (1.0 - (t + 0.1) / 0.22).clamp(0.0, 1.0);
            let mut list: Vec<(Joint, f32)> = (0..4)
                .filter(|i| base.weights[*i] > 0.0)
                .map(|i| {
                    (
                        rig::ALL[base.joints[i] as usize],
                        base.weights[i] * (1.0 - c),
                    )
                })
                .collect();
            list.push((Joint::Chest, c));
            Skin::mix(&list)
        } else {
            base
        }
    };
    let apex = s + (s - e).normalize() * 2.6;
    if upper_arm {
        bd.loft(
            &upper,
            n,
            lod,
            0.04,
            Some(apex),
            None,
            &up_skin,
            Region::Body,
        );
    }
    // Forearm.
    let fore = segment_rings(
        e,
        w,
        Vec3::X,
        &[
            (-0.04, 1.62, 1.55, 0.0, false),
            (0.18, 1.78, 1.62, -0.15, true),
            (0.5, 1.55, 1.35, 0.0, false),
            (0.85, 1.2, 0.98, 0.0, true),
            (1.0, 1.08, 0.86, 0.0, false),
        ],
    );
    let fo_skin = move |p: Vec3| seg_skin(el, wr, param(e, w, p), 0.15);
    bd.loft(&fore, n, lod, 0.04, None, None, &fo_skin, Region::Body);
    // Hand: palm, merged fingers (mitten) and thumb.
    let palm_n = rig::bind_side(wr);
    let dir = (f - w).normalize();
    let width_axis = palm_n.cross(dir).normalize();
    let palm = segment_rings(
        w,
        f,
        width_axis,
        &[
            (-0.05, 1.05, 0.78, 0.0, false),
            (0.45, 1.72, 0.82, 0.0, true),
            (1.0, 1.78, 0.66, 0.0, false),
        ],
    );
    let palm_skin = move |p: Vec3| {
        let t = param(w, f, p);
        let c = smooth(0.8, 1.1, t) * 0.5;
        Skin::mix(&[(wr, 1.0 - c), (fi, c)])
    };
    bd.loft(
        &palm,
        nh,
        lod,
        0.03,
        Some(w - dir * 0.6),
        None,
        &palm_skin,
        Region::Hand,
    );
    let tip = f + dir * rig::FINGER_LENGTH;
    let fingers = segment_rings(
        f,
        tip,
        width_axis,
        &[
            (0.0, 1.7, 0.6, 0.0, false),
            (0.55, 1.55, 0.52, 0.0, true),
            (1.0, 1.25, 0.42, 0.0, false),
        ],
    );
    let fi_skin = move |p: Vec3| {
        let t = param(f, tip, p);
        let c = smooth(-0.3, 0.15, t);
        Skin::mix(&[(wr, 1.0 - c), (fi, c)])
    };
    bd.loft(
        &fingers,
        nh,
        lod,
        0.03,
        None,
        Some(tip + dir * 0.35),
        &fi_skin,
        Region::Hand,
    );
    // Thumb, on the forward edge of the palm in the A-pose.
    let t0 = w + dir * 1.0 + Vec3::NEG_Z * 1.25 + palm_n * 0.45;
    let td = (dir * 0.7 + Vec3::NEG_Z * 0.45 + palm_n * 0.55).normalize();
    let t1 = t0 + td * 3.0;
    let thumb = segment_rings(
        t0,
        t1,
        palm_n,
        &[
            (0.0, 0.62, 0.55, 0.0, false),
            (0.6, 0.52, 0.48, 0.0, true),
            (1.0, 0.42, 0.4, 0.0, false),
        ],
    );
    let th_skin = move |_p: Vec3| Skin::one(wr);
    bd.loft(
        &thumb,
        nh.min(5),
        lod,
        0.02,
        Some(t0 - td * 0.5),
        Some(t1 + td * 0.3),
        &th_skin,
        Region::Hand,
    );
}

fn build_leg(bd: &mut Builder, lod: Lod, left: bool) {
    let (_, n, _) = lod.sides();
    let (hi, kn, an, to) = if left {
        (Joint::HipL, Joint::KneeL, Joint::AnkleL, Joint::ToeL)
    } else {
        (Joint::HipR, Joint::KneeR, Joint::AnkleR, Joint::ToeR)
    };
    let (h, k, a) = (bind(hi), bind(kn), bind(an));
    let thigh = segment_rings(
        h,
        k,
        Vec3::X,
        &[
            (-0.06, 3.3, 3.4, 0.0, false),
            (0.2, 3.3, 3.4, 0.0, true),
            (0.5, 2.9, 3.0, -0.1, false),
            (0.8, 2.35, 2.5, 0.0, true),
            (1.0, 2.15, 2.35, 0.0, false),
        ],
    );
    let th_skin = move |p: Vec3| {
        let t = param(h, k, p);
        let base = seg_skin(hi, kn, t, 0.15);
        let c = 0.4 * (1.0 - smooth(0.0, 0.2, t));
        if c > 0.0 {
            let mut list: Vec<(Joint, f32)> = (0..4)
                .filter(|i| base.weights[*i] > 0.0)
                .map(|i| {
                    (
                        rig::ALL[base.joints[i] as usize],
                        base.weights[i] * (1.0 - c),
                    )
                })
                .collect();
            list.push((Joint::Pelvis, c));
            Skin::mix(&list)
        } else {
            base
        }
    };
    bd.loft(
        &thigh,
        n,
        lod,
        0.04,
        Some(h + (h - k).normalize() * 2.5),
        None,
        &th_skin,
        Region::Body,
    );
    let shin = segment_rings(
        k,
        a,
        Vec3::X,
        &[
            (-0.03, 2.15, 2.35, 0.0, false),
            (0.14, 2.2, 2.4, 0.15, true),
            (0.33, 2.1, 2.55, 0.45, false),
            (0.65, 1.62, 1.82, 0.2, true),
            (0.9, 1.25, 1.38, 0.0, false),
            (1.0, 1.28, 1.45, 0.0, false),
        ],
    );
    let sh_skin = move |p: Vec3| seg_skin(kn, an, param(k, a, p), 0.1);
    bd.loft(&shin, n, lod, 0.04, None, None, &sh_skin, Region::Body);
    // Foot: rings across the forward axis, heel → toe.
    let x = a.x;
    let out = if left { -0.25 } else { 0.25 };
    let foot: [(f32, f32, f32, f32, bool); 5] = [
        // (z, half-width, half-height, centre y, detail)
        (2.5, 1.3, 1.35, 1.9, false),
        (0.8, 1.75, 2.25, 2.4, false),
        (-1.8, 2.0, 1.6, 1.75, true),
        (-4.6, 2.05, 0.95, 0.98, false),
        (-6.3, 1.6, 0.66, 0.72, false),
    ];
    let rings: Vec<RingSpec> = foot
        .iter()
        .map(|&(z, w, hh, y, d)| RingSpec {
            center: Vec3::new(x + out * smooth(0.0, -5.0, z), y, z),
            u: Vec3::X,
            v: Vec3::Y,
            ru: w,
            rv: hh,
            detail: d,
        })
        .collect();
    let ft_skin = move |p: Vec3| {
        let c = smooth(-2.6, -4.8, p.z);
        Skin::mix(&[(an, 1.0 - c), (to, c)])
    };
    bd.loft(
        &rings,
        n,
        lod,
        0.03,
        Some(Vec3::new(x, 1.8, 3.2)),
        Some(Vec3::new(x + out, 0.62, -7.1)),
        &ft_skin,
        Region::Body,
    );
}

/// Generates the humanoid at a level of detail.
pub fn humanoid(lod: Lod, parts: Parts) -> MeshData {
    let mut bd = Builder::default();
    if parts == Parts::FullBody {
        build_torso(&mut bd, lod);
        build_leg(&mut bd, lod, true);
        build_leg(&mut bd, lod, false);
    }
    let full = parts == Parts::FullBody;
    build_arm(&mut bd, lod, true, full);
    build_arm(&mut bd, lod, false, full);
    bd.finish()
}

/// Flat, irregular disc (death puddle), radius 1, normal up. Not skinned.
pub fn puddle() -> MeshData {
    let mut bd = Builder::default();
    let c = bd.vert(
        Vec3::new(0.0, 0.04, 0.0),
        Skin::one(Joint::Root),
        Region::Body,
    );
    let n = 14;
    let inner: Vec<u32> = (0..n)
        .map(|i| {
            let a = std::f32::consts::TAU * (i as f32 + 0.5) / n as f32;
            let r = 0.55 + 0.12 * hash01(i as u32 + 11);
            bd.vert(
                Vec3::new(a.cos() * r, 0.035, a.sin() * r),
                Skin::one(Joint::Root),
                Region::Body,
            )
        })
        .collect();
    let outer: Vec<u32> = (0..n)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / n as f32;
            let r = 0.85 + 0.25 * hash01(i as u32 + 97);
            bd.vert(
                Vec3::new(a.cos() * r, 0.0, a.sin() * r),
                Skin::one(Joint::Root),
                Region::Body,
            )
        })
        .collect();
    let below = Vec3::new(0.0, -1.0, 0.0);
    bd.cap(&inner, c, below);
    bd.bridge(&outer, &inner, true, below);
    bd.finish()
}

/// Low-poly droplet (icosahedron), radius 1. Not skinned.
pub fn droplet() -> MeshData {
    let t = (1.0 + 5f32.sqrt()) / 2.0;
    let raw = [
        (-1.0, t, 0.0),
        (1.0, t, 0.0),
        (-1.0, -t, 0.0),
        (1.0, -t, 0.0),
        (0.0, -1.0, t),
        (0.0, 1.0, t),
        (0.0, -1.0, -t),
        (0.0, 1.0, -t),
        (t, 0.0, -1.0),
        (t, 0.0, 1.0),
        (-t, 0.0, -1.0),
        (-t, 0.0, 1.0),
    ];
    let faces: [[u32; 3]; 20] = [
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    let mut bd = Builder::default();
    for (x, y, z) in raw {
        // Slight teardrop: the top is pulled up.
        let mut p = Vec3::new(x, y, z).normalize();
        if p.y > 0.3 {
            p.y *= 1.35;
        }
        bd.vert(p, Skin::one(Joint::Root), Region::Body);
    }
    for f in faces {
        bd.tris.push(Tri {
            v: f,
            inside: Vec3::ZERO,
        });
    }
    bd.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_is_low_poly_and_human_sized() {
        let hi = humanoid(Lod::High, Parts::FullBody);
        let mid = humanoid(Lod::Mid, Parts::FullBody);
        let far = humanoid(Lod::Far, Parts::FullBody);
        let (lo, top) = hi.bounds();
        assert!(top.y > 70.5 && top.y < 72.0, "head top {}", top.y);
        assert!(lo.y > -0.1 && lo.y < 0.8, "sole {}", lo.y);
        assert!(hi.triangle_count() < 3000, "{} tris", hi.triangle_count());
        assert!(mid.triangle_count() < hi.triangle_count());
        assert!(far.triangle_count() * 2 < hi.triangle_count());
        assert!(far.triangle_count() > 300);
    }

    #[test]
    fn mesh_is_symmetric_and_closed_outward() {
        let m = humanoid(Lod::High, Parts::FullBody);
        let (lo, hi) = m.bounds();
        assert!((lo.x + hi.x).abs() < 0.05, "x {} .. {}", lo.x, hi.x);
        // Normals point away from the body axis for the torso facets.
        let mut outward = 0;
        let mut total = 0;
        for (p, n) in m.positions.iter().zip(&m.normals) {
            let p = Vec3::from_array(*p);
            if (40.0..56.0).contains(&p.y) && p.x.abs() < 5.0 {
                let radial = Vec3::new(p.x, 0.0, p.z);
                total += 1;
                if Vec3::from_array(*n).dot(radial) > 0.0 {
                    outward += 1;
                }
            }
        }
        assert!(outward as f32 > total as f32 * 0.97, "{outward}/{total}");
    }

    #[test]
    fn skin_weights_are_normalised() {
        for lod in [Lod::High, Lod::Far] {
            let m = humanoid(lod, Parts::FullBody);
            for (w, j) in m.weights.iter().zip(&m.joints) {
                let s: f32 = w.iter().sum();
                assert!((s - 1.0).abs() < 1e-4);
                for (k, jj) in j.iter().enumerate() {
                    assert!((*jj as usize) < rig::JOINT_COUNT || w[k] == 0.0);
                }
            }
        }
    }

    #[test]
    fn arms_only_has_no_torso() {
        let m = humanoid(Lod::High, Parts::ArmsOnly);
        for p in &m.positions {
            assert!(p[0].abs() > 4.0, "vertex near the body axis: {p:?}");
        }
    }
}
