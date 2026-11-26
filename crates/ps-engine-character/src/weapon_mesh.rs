//! Solid low-poly weapons held by the characters (third and first person).
//!
//! Deliberately *not* liquid: matte gunmetal with a few lighter parts, so a
//! weapon always reads apart from the paint body. Origin = where the right
//! hand grips, -z = barrel, game units.

use super::anim::Hold;
use bevy::asset::RenderAssetUsages;
use bevy::math::{Quat, Vec3};
use bevy::mesh::{Mesh, PrimitiveTopology};

const DARK: [f32; 4] = [0.07, 0.075, 0.085, 1.0];
const MID: [f32; 4] = [0.16, 0.17, 0.19, 1.0];
const LIGHT: [f32; 4] = [0.42, 0.44, 0.47, 1.0];
const GRIP: [f32; 4] = [0.11, 0.1, 0.09, 1.0];
const BLADE: [f32; 4] = [0.7, 0.72, 0.75, 1.0];
const OLIVE: [f32; 4] = [0.2, 0.24, 0.14, 1.0];
const SIGNAL: [f32; 4] = [0.85, 0.15, 0.1, 1.0];

#[derive(Default)]
pub struct Solid {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
}

impl Solid {
    fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3, col: [f32; 4]) {
        let n = (b - a).cross(c - a).normalize_or_zero();
        for p in [a, b, c] {
            self.positions.push(p.to_array());
            self.normals.push(n.to_array());
            self.colors.push(col);
        }
    }

    fn quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, col: [f32; 4]) {
        self.tri(a, b, c, col);
        self.tri(a, c, d, col);
    }

    /// Box centred on `c` with half extents `h`, rotated by `r`.
    fn cuboid(&mut self, c: Vec3, h: Vec3, r: Quat, col: [f32; 4]) {
        let p = |x: f32, y: f32, z: f32| c + r * Vec3::new(x * h.x, y * h.y, z * h.z);
        let v = [
            p(-1.0, -1.0, -1.0),
            p(1.0, -1.0, -1.0),
            p(1.0, 1.0, -1.0),
            p(-1.0, 1.0, -1.0),
            p(-1.0, -1.0, 1.0),
            p(1.0, -1.0, 1.0),
            p(1.0, 1.0, 1.0),
            p(-1.0, 1.0, 1.0),
        ];
        let faces = [
            [0, 3, 2, 1],
            [4, 5, 6, 7],
            [0, 4, 7, 3],
            [1, 2, 6, 5],
            [3, 7, 6, 2],
            [0, 1, 5, 4],
        ];
        for f in faces {
            self.quad(v[f[0]], v[f[1]], v[f[2]], v[f[3]], col);
        }
    }

    fn bx(&mut self, c: (f32, f32, f32), h: (f32, f32, f32), col: [f32; 4]) {
        self.cuboid(
            Vec3::new(c.0, c.1, c.2),
            Vec3::new(h.0, h.1, h.2),
            Quat::IDENTITY,
            col,
        );
    }

    fn bx_tilt(&mut self, c: (f32, f32, f32), h: (f32, f32, f32), tilt_deg: f32, col: [f32; 4]) {
        self.cuboid(
            Vec3::new(c.0, c.1, c.2),
            Vec3::new(h.0, h.1, h.2),
            Quat::from_rotation_x(tilt_deg.to_radians()),
            col,
        );
    }

    /// Prism along z (barrels, scopes): `n` sides.
    fn tube(&mut self, c: Vec3, r: f32, half_len: f32, n: usize, col: [f32; 4]) {
        let ring = |z: f32| -> Vec<Vec3> {
            (0..n)
                .map(|i| {
                    let a = std::f32::consts::TAU * (i as f32 + 0.5) / n as f32;
                    c + Vec3::new(a.cos() * r, a.sin() * r, z)
                })
                .collect()
        };
        let (f, b) = (ring(-half_len), ring(half_len));
        for i in 0..n {
            let j = (i + 1) % n;
            self.quad(f[i], f[j], b[j], b[i], col);
            self.tri(c + Vec3::Z * -half_len, f[j], f[i], col);
            self.tri(c + Vec3::Z * half_len, b[i], b[j], col);
        }
    }

    #[cfg(test)]
    pub fn triangle_count(&self) -> usize {
        self.positions.len() / 3
    }

    pub fn into_mesh(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
    }
}

fn rifle(s: &mut Solid, sniper: bool) {
    let len = if sniper { 1.35 } else { 1.0 };
    s.bx((0.0, 2.3, -3.0), (0.95, 1.5, 6.0), MID); // receiver
    s.bx((0.0, 2.2, -11.5), (1.05, 1.25, 3.2 * len), DARK); // handguard
    s.tube(
        Vec3::new(0.0, 2.7, -16.0 - 6.0 * (len - 1.0)),
        0.38,
        5.5 * len,
        6,
        DARK,
    ); // barrel
    s.bx((0.0, 1.9, 6.5), (0.8, 1.7, 3.8), MID); // stock
    s.bx((0.0, 2.9, 10.0), (0.85, 1.0, 0.4), DARK); // butt pad
    s.bx_tilt((0.0, -0.6, 0.0), (0.6, 1.9, 0.85), -15.0, GRIP); // pistol grip
    s.bx((0.0, 4.0, -3.5), (0.35, 0.35, 4.5), LIGHT); // rail
    if sniper {
        s.tube(Vec3::new(0.0, 5.4, -3.0), 0.95, 4.2, 8, DARK);
        s.bx((0.0, 4.5, -3.0), (0.3, 0.6, 2.0), MID);
        s.bx((0.0, -0.4, -6.0), (0.6, 1.6, 1.3), DARK);
    } else {
        s.bx_tilt((0.0, -1.2, -5.8), (0.65, 2.6, 1.15), 12.0, DARK); // magazine
        s.bx((0.0, 4.9, -11.0), (0.25, 0.6, 0.3), LIGHT); // front sight
    }
}

fn pistol(s: &mut Solid) {
    s.bx((0.0, 2.3, -3.0), (0.62, 0.85, 4.2), MID); // slide
    s.bx((0.0, 1.2, -2.6), (0.55, 0.45, 3.4), DARK); // frame
    s.bx_tilt((0.0, -0.8, 0.0), (0.55, 2.0, 0.95), -14.0, GRIP);
    s.bx((0.0, 3.3, -6.7), (0.15, 0.25, 0.2), LIGHT);
}

fn knife(s: &mut Solid) {
    s.bx((0.0, 0.0, 0.6), (0.5, 0.6, 2.2), GRIP); // handle
    s.bx((0.0, 0.0, -1.8), (0.9, 0.85, 0.25), DARK); // guard
    // Blade: tapered.
    let a = Vec3::new(0.0, 0.7, -2.0);
    let b = Vec3::new(0.0, -0.5, -2.0);
    let tip = Vec3::new(0.0, 0.3, -9.5);
    let w = Vec3::X * 0.14;
    s.tri(a + w, b + w, tip, BLADE);
    s.tri(b - w, a - w, tip, BLADE);
    s.quad(a - w, a + w, tip, tip, LIGHT);
    s.quad(b + w, b - w, tip, tip, LIGHT);
}

fn grenade(s: &mut Solid) {
    let c = Vec3::new(0.0, 1.6, -1.0);
    s.tube(c, 1.35, 1.5, 8, OLIVE);
    s.bx((0.0, 3.4, -1.0), (0.45, 0.4, 0.45), MID);
    s.bx_tilt((0.0, 2.2, 0.4), (0.25, 1.6, 0.18), 10.0, LIGHT); // lever
}

fn bomb(s: &mut Solid) {
    s.bx((3.4, 1.0, -1.2), (4.2, 2.4, 2.0), OLIVE);
    s.bx((3.4, 3.6, -1.6), (2.2, 0.25, 1.1), DARK);
    s.bx((2.4, 3.9, -1.6), (0.5, 0.15, 0.4), SIGNAL);
}

pub fn weapon(h: Hold) -> Option<Solid> {
    let mut s = Solid::default();
    match h {
        Hold::Rifle => rifle(&mut s, false),
        Hold::Sniper => rifle(&mut s, true),
        Hold::Pistol => pistol(&mut s),
        Hold::Knife => knife(&mut s),
        Hold::Grenade => grenade(&mut s),
        Hold::Bomb => bomb(&mut s),
        Hold::Empty => return None,
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weapons_are_low_poly() {
        for h in Hold::ALL {
            match weapon(h) {
                Some(s) => {
                    assert!(s.triangle_count() > 10 && s.triangle_count() < 400, "{h:?}");
                }
                None => assert_eq!(h, Hold::Empty),
            }
        }
    }
}
