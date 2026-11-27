//! Humanoid skeleton shared by the third-person body, the first-person arms and
//! the glTF export.
//!
//! Units are game units (1 u = 2.54 cm), axes are those of the hitboxes
//! (`engine_hitreg`): x right, y up, z backward (forward is -z). The skeleton is
//! *flat*: every joint is posed in model space and every joint entity is a
//! direct child of the model root, so poses can be blended, collapsed (death)
//! and squashed without parent/child scale skew. Bind rotations are identity,
//! hence an inverse bind pose is a plain translation by `-BIND[j]`.

#[cfg(test)]
use bevy::math::Mat4;
use bevy::math::{Mat3, Quat, Vec3};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum Joint {
    Root,
    Pelvis,
    Spine,
    Chest,
    Neck,
    Head,
    ShoulderL,
    ElbowL,
    WristL,
    FingersL,
    ShoulderR,
    ElbowR,
    WristR,
    FingersR,
    HipL,
    KneeL,
    AnkleL,
    ToeL,
    HipR,
    KneeR,
    AnkleR,
    ToeR,
}

pub const JOINT_COUNT: usize = 22;

pub const ALL: [Joint; JOINT_COUNT] = [
    Joint::Root,
    Joint::Pelvis,
    Joint::Spine,
    Joint::Chest,
    Joint::Neck,
    Joint::Head,
    Joint::ShoulderL,
    Joint::ElbowL,
    Joint::WristL,
    Joint::FingersL,
    Joint::ShoulderR,
    Joint::ElbowR,
    Joint::WristR,
    Joint::FingersR,
    Joint::HipL,
    Joint::KneeL,
    Joint::AnkleL,
    Joint::ToeL,
    Joint::HipR,
    Joint::KneeR,
    Joint::AnkleR,
    Joint::ToeR,
];

impl Joint {
    pub const fn i(self) -> usize {
        self as usize
    }

    #[cfg(test)]
    pub const fn name(self) -> &'static str {
        match self {
            Joint::Root => "root",
            Joint::Pelvis => "pelvis",
            Joint::Spine => "spine",
            Joint::Chest => "chest",
            Joint::Neck => "neck",
            Joint::Head => "head",
            Joint::ShoulderL => "upperarm_l",
            Joint::ElbowL => "forearm_l",
            Joint::WristL => "hand_l",
            Joint::FingersL => "fingers_l",
            Joint::ShoulderR => "upperarm_r",
            Joint::ElbowR => "forearm_r",
            Joint::WristR => "hand_r",
            Joint::FingersR => "fingers_r",
            Joint::HipL => "thigh_l",
            Joint::KneeL => "shin_l",
            Joint::AnkleL => "foot_l",
            Joint::ToeL => "toe_l",
            Joint::HipR => "thigh_r",
            Joint::KneeR => "shin_r",
            Joint::AnkleR => "foot_r",
            Joint::ToeR => "toe_r",
        }
    }

    /// Parent in the logical hierarchy (used for the glTF export and the
    /// forward kinematics of the procedural animation).
    pub const fn parent(self) -> Option<Joint> {
        use Joint::*;
        match self {
            Root => None,
            Pelvis => Some(Root),
            Spine => Some(Pelvis),
            Chest => Some(Spine),
            Neck => Some(Chest),
            Head => Some(Neck),
            ShoulderL | ShoulderR => Some(Chest),
            ElbowL => Some(ShoulderL),
            WristL => Some(ElbowL),
            FingersL => Some(WristL),
            ElbowR => Some(ShoulderR),
            WristR => Some(ElbowR),
            FingersR => Some(WristR),
            HipL | HipR => Some(Pelvis),
            KneeL => Some(HipL),
            AnkleL => Some(KneeL),
            ToeL => Some(AnkleL),
            KneeR => Some(HipR),
            AnkleR => Some(KneeR),
            ToeR => Some(AnkleR),
        }
    }
}

/// Bind pose (model space, game units). Arms hang in a relaxed A-pose, legs
/// are straight under the hips at the hitbox leg axes (x = ±5).
pub const BIND: [Vec3; JOINT_COUNT] = [
    Vec3::new(0.0, 0.0, 0.0),    // root
    Vec3::new(0.0, 36.0, 0.0),   // pelvis
    Vec3::new(0.0, 42.0, 0.3),   // spine
    Vec3::new(0.0, 49.0, 0.3),   // chest
    Vec3::new(0.0, 58.6, 0.2),   // neck
    Vec3::new(0.0, 61.8, -0.3),  // head (skull base)
    Vec3::new(-7.8, 56.0, 0.4),  // shoulder l
    Vec3::new(-9.8, 44.0, 0.8),  // elbow l
    Vec3::new(-10.6, 33.6, 0.2), // wrist l
    Vec3::new(-10.8, 30.4, 0.0), // fingers l
    Vec3::new(7.8, 56.0, 0.4),   // shoulder r
    Vec3::new(9.8, 44.0, 0.8),   // elbow r
    Vec3::new(10.6, 33.6, 0.2),  // wrist r
    Vec3::new(10.8, 30.4, 0.0),  // fingers r
    Vec3::new(-4.6, 35.0, 0.0),  // hip l
    Vec3::new(-4.9, 19.0, -0.6), // knee l
    Vec3::new(-5.0, 3.4, 0.6),   // ankle l
    Vec3::new(-5.1, 0.9, -4.6),  // toe l
    Vec3::new(4.6, 35.0, 0.0),   // hip r
    Vec3::new(4.9, 19.0, -0.6),  // knee r
    Vec3::new(5.0, 3.4, 0.6),    // ankle r
    Vec3::new(5.1, 0.9, -4.6),   // toe r
];

/// Fingertip distance beyond the `Fingers` joint, along the hand.
pub const FINGER_LENGTH: f32 = 3.3;

pub fn bind(j: Joint) -> Vec3 {
    BIND[j.i()]
}

/// Distance between a joint and its parent in the bind pose.
pub fn bone_length(j: Joint) -> f32 {
    j.parent().map_or(0.0, |p| bind(j).distance(bind(p)))
}

/// Bind direction of the bone that starts at `j` (towards its main child).
pub fn bind_dir(j: Joint) -> Vec3 {
    use Joint::*;
    let child = match j {
        Pelvis => Spine,
        Spine => Chest,
        Chest => Neck,
        Neck => Head,
        Head => return Vec3::Y,
        ShoulderL => ElbowL,
        ElbowL => WristL,
        WristL => FingersL,
        FingersL => return (bind(FingersL) - bind(WristL)).normalize(),
        ShoulderR => ElbowR,
        ElbowR => WristR,
        WristR => FingersR,
        FingersR => return (bind(FingersR) - bind(WristR)).normalize(),
        HipL => KneeL,
        KneeL => AnkleL,
        AnkleL => ToeL,
        ToeL => return Vec3::NEG_Z,
        HipR => KneeR,
        KneeR => AnkleR,
        AnkleR => ToeR,
        ToeR => return Vec3::NEG_Z,
        Root => return Vec3::Y,
    };
    (bind(child) - bind(j)).normalize()
}

/// Bind "side" axis of a bone: the hinge axis for limbs (flexion turns the
/// child towards `dir × side`), the palm normal for hands.
pub fn bind_side(j: Joint) -> Vec3 {
    use Joint::*;
    match j {
        // Legs: knees flex forward, the shin swings backward.
        HipL | KneeL | HipR | KneeR => Vec3::NEG_X,
        // Palms face the thighs in the A-pose.
        WristL | FingersL => Vec3::X,
        WristR | FingersR => Vec3::NEG_X,
        _ => Vec3::X,
    }
}

/// Model-space transform of one joint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointXf {
    pub pos: Vec3,
    pub rot: Quat,
    pub scale: Vec3,
}

impl Default for JointXf {
    fn default() -> Self {
        JointXf {
            pos: Vec3::ZERO,
            rot: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

impl JointXf {
    #[cfg(test)]
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rot, self.pos)
    }

    pub fn lerp(&self, o: &JointXf, t: f32) -> JointXf {
        JointXf {
            pos: self.pos.lerp(o.pos, t),
            rot: self.rot.slerp(o.rot, t),
            scale: self.scale.lerp(o.scale, t),
        }
    }
}

/// A full pose: model-space transform of every joint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose(pub [JointXf; JOINT_COUNT]);

impl Pose {
    pub fn bind() -> Pose {
        let mut p = [JointXf::default(); JOINT_COUNT];
        for j in ALL {
            p[j.i()].pos = bind(j);
        }
        Pose(p)
    }

    pub fn get(&self, j: Joint) -> &JointXf {
        &self.0[j.i()]
    }

    pub fn get_mut(&mut self, j: Joint) -> &mut JointXf {
        &mut self.0[j.i()]
    }

    /// Skinning matrix of a joint (joint transform × inverse bind).
    #[cfg(test)]
    pub fn skin_matrix(&self, j: usize) -> Mat4 {
        self.0[j].matrix() * Mat4::from_translation(-BIND[j])
    }

    /// Point of the skin attached to joint `j`, given in bind space.
    #[cfg(test)]
    pub fn skin_point(&self, j: Joint, bind_point: Vec3) -> Vec3 {
        let x = &self.0[j.i()];
        x.pos + x.rot * (x.scale * (bind_point - bind(j)))
    }

    pub fn lerp(&self, o: &Pose, t: f32) -> Pose {
        let mut out = *self;
        for (i, x) in out.0.iter_mut().enumerate() {
            *x = self.0[i].lerp(&o.0[i], t);
        }
        out
    }

    /// Re-places the children of `j` after `j` moved or turned (forward
    /// kinematics of one sub-tree, keeping each bone at its bind length).
    pub fn propagate(&mut self, j: Joint) {
        for c in ALL {
            if c.parent() == Some(j) {
                let p = self.0[j.i()];
                self.0[c.i()].pos = p.pos + p.rot * (p.scale * (bind(c) - bind(j)));
                self.propagate(c);
            }
        }
    }
}

/// Rotation that takes the bone frame `(bind_dir, bind_side)` to
/// `(dir, side)`. `side` is orthogonalised against `dir`; when they are
/// parallel the shortest arc is used instead.
pub fn frame_rot(bind_dir: Vec3, bind_side: Vec3, dir: Vec3, side: Vec3) -> Quat {
    let basis = |d: Vec3, s: Vec3| -> Option<Mat3> {
        let d = d.normalize_or_zero();
        let s = (s - d * s.dot(d)).normalize_or_zero();
        if d == Vec3::ZERO || s == Vec3::ZERO {
            return None;
        }
        Some(Mat3::from_cols(d, s, d.cross(s)))
    };
    match (basis(bind_dir, bind_side), basis(dir, side)) {
        (Some(b), Some(t)) => Quat::from_mat3(&(t * b.transpose())).normalize(),
        _ => Quat::from_rotation_arc(bind_dir.normalize(), dir.normalize_or(bind_dir)),
    }
}

/// Analytic two-bone IK. Returns the middle joint (elbow/knee) and the
/// reachable end point. `pole` is the direction the middle joint bends to.
pub fn two_bone(a: Vec3, target: Vec3, l1: f32, l2: f32, pole: Vec3) -> (Vec3, Vec3) {
    let d = target - a;
    let len = d.length();
    let dir = if len > 1e-5 { d / len } else { Vec3::NEG_Y };
    let reach = (l1 + l2) * 0.9995;
    let dist = len.clamp((l1 - l2).abs() + 1e-3, reach);
    let cos_a = ((l1 * l1 + dist * dist - l2 * l2) / (2.0 * l1 * dist)).clamp(-1.0, 1.0);
    let along = l1 * cos_a;
    let h = (l1 * l1 - along * along).max(0.0).sqrt();
    let mut bend = pole - dir * pole.dot(dir);
    if bend.length_squared() < 1e-8 {
        bend = dir.any_orthonormal_vector();
    }
    let mid = a + dir * along + bend.normalize() * h;
    (mid, a + dir * dist)
}

/// Poses a limb (`upper` → `lower` → `end`) with two-bone IK: positions the
/// three joints and orients both bones. The hinge axis follows the bend
/// plane so flexion never twists the limb.
pub fn solve_limb(
    pose: &mut Pose,
    upper: Joint,
    lower: Joint,
    end: Joint,
    target: Vec3,
    pole: Vec3,
) {
    let a = pose.get(upper).pos;
    let l1 = bone_length(lower);
    let l2 = bone_length(end);
    let (mid, reached) = two_bone(a, target, l1, l2, pole);
    let d1 = (mid - a).normalize_or(bind_dir(upper));
    let d2 = (reached - mid).normalize_or(d1);
    // Hinge: normal of the bend plane, signed like the bind side axis.
    // For a bent limb d1 × d2 is along pole × dir; a straight limb uses that
    // directly so the twist stays stable.
    let mut hinge = d1.cross(d2);
    if hinge.length_squared() < 1e-6 {
        hinge = pole.cross(d1);
    }
    let hinge = hinge.normalize_or(bind_side(upper));
    pose.get_mut(upper).rot = frame_rot(bind_dir(upper), bind_side(upper), d1, hinge);
    pose.get_mut(lower).rot = frame_rot(bind_dir(lower), bind_side(lower), d2, hinge);
    pose.get_mut(lower).pos = mid;
    pose.get_mut(end).pos = reached;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hierarchy_is_consistent() {
        for (i, j) in ALL.iter().enumerate() {
            assert_eq!(j.i(), i);
            if let Some(p) = j.parent() {
                assert!(p.i() < j.i(), "{} before its parent", j.name());
                assert!(bone_length(*j) > 0.5);
            }
        }
    }

    #[test]
    fn proportions_are_human() {
        // 72 u (1.83 m) hull: head top ≈ 71.5, hips ≈ half height.
        let upper = bone_length(Joint::ElbowR);
        let fore = bone_length(Joint::WristR);
        let thigh = bone_length(Joint::KneeR);
        let shin = bone_length(Joint::AnkleR);
        assert!((11.0..13.5).contains(&upper), "upper arm {upper}");
        assert!((9.5..11.5).contains(&fore), "forearm {fore}");
        assert!((15.0..17.0).contains(&thigh), "thigh {thigh}");
        assert!((14.5..16.5).contains(&shin), "shin {shin}");
        let shoulders = bind(Joint::ShoulderR).x - bind(Joint::ShoulderL).x;
        assert!((14.0..17.0).contains(&shoulders), "shoulders {shoulders}");
    }

    #[test]
    fn two_bone_reaches_and_keeps_lengths() {
        let a = Vec3::new(7.8, 56.0, 0.4);
        let t = Vec3::new(3.6, 50.0, -10.0);
        let (m, e) = two_bone(a, t, 12.2, 10.4, Vec3::new(0.5, -1.0, 0.2));
        assert!((m.distance(a) - 12.2).abs() < 1e-3);
        assert!((e.distance(m) - 10.4).abs() < 1e-3);
        assert!(e.distance(t) < 1e-3);
        // The elbow went towards the pole (down).
        assert!(m.y < (a.y + t.y) * 0.5);
        // Out of reach: stretched straight towards the target.
        let far = a + Vec3::new(0.0, 0.0, -40.0);
        let (_, e) = two_bone(a, far, 12.2, 10.4, Vec3::NEG_Y);
        assert!((e.distance(a) - 22.6).abs() < 0.05);
    }

    #[test]
    fn frame_rot_maps_frames() {
        let r = frame_rot(Vec3::NEG_Y, Vec3::X, Vec3::NEG_Z, Vec3::X);
        assert!((r * Vec3::NEG_Y).distance(Vec3::NEG_Z) < 1e-5);
        assert!((r * Vec3::X).distance(Vec3::X) < 1e-5);
    }

    #[test]
    fn limb_solve_orients_bones() {
        let mut p = Pose::bind();
        let target = Vec3::new(3.6, 50.0, -10.0);
        solve_limb(
            &mut p,
            Joint::ShoulderR,
            Joint::ElbowR,
            Joint::WristR,
            target,
            Vec3::new(0.5, -1.0, 0.2),
        );
        assert!(p.get(Joint::WristR).pos.distance(target) < 1e-3);
        // Skinning the bind elbow and wrist with the new rotations lands on the solved joints.
        let elbow = p.skin_point(Joint::ShoulderR, bind(Joint::ElbowR));
        assert!(elbow.distance(p.get(Joint::ElbowR).pos) < 1e-3);
        let wrist = p.skin_point(Joint::ElbowR, bind(Joint::WristR));
        assert!(wrist.distance(target) < 1e-3);
    }
}
